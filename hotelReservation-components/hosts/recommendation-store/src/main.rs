use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;

use host_lib::{
    make_engine, make_store_wasi_ctx, RecommendationStoreHostWorld, StoreData,
    recommendation_store::{
        recommendation_store_server::{RecommendationStore, RecommendationStoreServer},
        Hotel as ProtoHotel, InitRequest, InitResponse, LoadHotelsRequest, LoadHotelsResponse,
    },
};

type SharedStore = Arc<Mutex<Store<StoreData>>>;
type SharedInstance = Arc<RecommendationStoreHostWorld>;

struct StoreGrpcService {
    store: SharedStore,
    instance: SharedInstance,
}

#[tonic::async_trait]
impl RecommendationStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_recommendation_data_recommendation_store()
            .call_init(&mut *store)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_hotels(
        &self,
        _req: Request<LoadHotelsRequest>,
    ) -> Result<Response<LoadHotelsResponse>, Status> {
        let mut store = self.store.lock().await;
        let wit_hotels = self
            .instance
            .hotel_recommendation_data_recommendation_store()
            .call_load_hotels(&mut *store)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let hotels = wit_hotels
            .into_iter()
            .map(|h| ProtoHotel {
                id: h.id,
                lat: h.lat,
                lon: h.lon,
                rate: h.rate,
                price: h.price,
            })
            .collect();

        Ok(Response::new(LoadHotelsResponse { hotels }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8086".into());
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "recommendation-store.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(
        mongo
            .database("recommendation-db")
            .collection("recommendation"),
    );

    let engine = make_engine()?;
    let mut linker: Linker<StoreData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    RecommendationStoreHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = StoreData {
        wasi: make_store_wasi_ctx(&data_dir)?,
        table: ResourceTable::new(),
        collection,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance =
        RecommendationStoreHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance
        .hotel_recommendation_data_recommendation_store()
        .call_init(&mut store)
        .await?;

    let store = Arc::new(Mutex::new(store));
    let instance = Arc::new(instance);

    println!("recommendation-store-host listening on {listen_addr}");

    Server::builder()
        .add_service(RecommendationStoreServer::new(StoreGrpcService {
            store,
            instance,
        }))
        .serve(listen_addr.parse()?)
        .await?;

    Ok(())
}
