use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;

use exports::hotel::recommendation::recommendation::Requirement;

use host_lib::{
    make_engine, make_wasi_ctx,
    recommendation::{
        recommendation_server::{Recommendation, RecommendationServer},
        Request as RecommendRequest, Result as RecommendResult,
    },
    recommendation_store::{
        recommendation_store_client::RecommendationStoreClient, LoadHotelsRequest,
        InitRequest,
    },
};

wasmtime::component::bindgen!({
    path: "../../components/recommendation/wit",
    world: "recommendation-host-world",
    async: true,
});

// Host-side data threaded through the wasmtime Store.
// The recommendation component imports hotel:recommendation-data/recommendation-store,
// which this host satisfies via gRPC calls to the store host.
struct RecommendationHostData {
    wasi: wasmtime_wasi::WasiCtx,
    table: ResourceTable,
    store_client: Arc<Mutex<RecommendationStoreClient<tonic::transport::Channel>>>,
}

impl wasmtime_wasi::WasiView for RecommendationHostData {
    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx {
        &mut self.wasi
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

// Implement hotel:recommendation-data/recommendation-store as a gRPC bridge.
impl hotel::recommendation_data::recommendation_store::Host for RecommendationHostData {
    async fn init(&mut self) {
        self.store_client
            .lock()
            .await
            .init(tonic::Request::new(InitRequest {}))
            .await
            .expect("gRPC store Init failed");
    }

    async fn load_hotels(
        &mut self,
    ) -> Vec<hotel::recommendation_data::recommendation_store::Hotel> {
        let resp = self
            .store_client
            .lock()
            .await
            .load_hotels(tonic::Request::new(LoadHotelsRequest {}))
            .await
            .expect("gRPC store LoadHotels failed")
            .into_inner();

        resp.hotels
            .into_iter()
            .map(|h| hotel::recommendation_data::recommendation_store::Hotel {
                id: h.id,
                lat: h.lat,
                lon: h.lon,
                rate: h.rate,
                price: h.price,
            })
            .collect()
    }
}

// --- gRPC service for the outward-facing Recommendation interface ---

type SharedStore = Arc<Mutex<Store<RecommendationHostData>>>;
type SharedInstance = Arc<RecommendationHostWorld>;

struct RecommendationGrpcService {
    store: SharedStore,
    instance: SharedInstance,
}

#[tonic::async_trait]
impl Recommendation for RecommendationGrpcService {
    async fn get_recommendations(
        &self,
        req: Request<RecommendRequest>,
    ) -> Result<Response<RecommendResult>, Status> {
        let r = req.into_inner();

        // Map the string requirement field to the WIT enum.
        let requirement = match r.require.as_str() {
            "dis" | "distance" => Requirement::Distance,
            "rate"             => Requirement::Rate,
            "price"            => Requirement::Price,
            other => return Err(Status::invalid_argument(format!("unknown require: {other}"))),
        };

        let mut store = self.store.lock().await;
        let hotel_ids = self
            .instance
            .hotel_recommendation_recommendation()
            .call_recommend(&mut *store, requirement, r.lat, r.lon)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(RecommendResult {
            hotel_ids,
        }))
    }
}

// --- main ---

#[tokio::main]
async fn main() -> Result<()> {
    let store_addr = std::env::var("STORE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8086".into());
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8085".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "recommendation.wasm".into());

    // Connect gRPC client to recommendation-store host.
    let store_client = RecommendationStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));

    // Set up wasmtime.
    let engine = make_engine()?;
    let mut linker: Linker<RecommendationHostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    RecommendationHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = RecommendationHostData {
        wasi: make_wasi_ctx(),
        table: ResourceTable::new(),
        store_client,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance =
        RecommendationHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    // init(): triggers gRPC LoadHotels → component populates in-memory map.
    instance
        .hotel_recommendation_recommendation()
        .call_init(&mut store)
        .await?;

    let store = Arc::new(Mutex::new(store));
    let instance = Arc::new(instance);

    println!("recommendation-tcp-host listening on {listen_addr}");

    Server::builder()
        .add_service(RecommendationServer::new(RecommendationGrpcService {
            store,
            instance,
        }))
        .serve(listen_addr.parse()?)
        .await?;

    Ok(())
}
