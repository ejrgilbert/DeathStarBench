use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
use host_lib::{make_engine, make_wasi_ctx};

use crate::grpc::{Requirement, RecommendComponent};

mod store_proto {
    tonic::include_proto!("recommendation_store");
}
use store_proto::{
    recommendation_store_client::RecommendationStoreClient,
    InitRequest, LoadHotelsRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/recommendation/wit",
    world: "recommendation-host-world",
    async: true,
});

use exports::hotel::recommendation::recommendation::Requirement as WitRequirement;

pub struct HostData {
    wasi: wasmtime_wasi::WasiCtx,
    table: ResourceTable,
    store_client: Arc<Mutex<RecommendationStoreClient<tonic::transport::Channel>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx { &mut self.wasi }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::recommendation_data::recommendation_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client
            .lock().await
            .init(tonic::Request::new(InitRequest {}))
            .await
            .expect("gRPC store Init failed");
    }

    async fn load_hotels(&mut self) -> Vec<hotel::recommendation_data::recommendation_store::Hotel> {
        let resp = self.store_client
            .lock().await
            .load_hotels(tonic::Request::new(LoadHotelsRequest {}))
            .await
            .expect("gRPC store LoadHotels failed")
            .into_inner();

        resp.hotels.into_iter().map(|h| hotel::recommendation_data::recommendation_store::Hotel {
            id: h.id, lat: h.lat, lon: h.lon, rate: h.rate, price: h.price,
        }).collect()
    }
}

#[async_trait::async_trait]
impl RecommendComponent for RecommendationHostWorld {
    type Data = HostData;

    async fn recommend(
        &self,
        store: &mut Store<Self::Data>,
        req: Requirement,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>> {
        let r = match req {
            Requirement::Distance => WitRequirement::Distance,
            Requirement::Rate    => WitRequirement::Rate,
            Requirement::Price   => WitRequirement::Price,
        };
        Ok(self.hotel_recommendation_recommendation()
            .call_recommend(store, r, lat, lon)
            .await?)
    }
}

pub async fn run() -> Result<()> {
    let store_addr = std::env::var("STORE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8086".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8085".into())
        .parse()?;
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "recommendation.wasm".into());

    let store_client = RecommendationStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));

    let engine = make_engine()?;
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    RecommendationHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = HostData {
        wasi: make_wasi_ctx(),
        table: ResourceTable::new(),
        store_client,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance =
        RecommendationHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance.hotel_recommendation_recommendation()
        .call_init(&mut store)
        .await?;

    println!("recommendation-host [tcp] listening on {listen_addr}");

    crate::grpc::serve(
        Arc::new(Mutex::new(store)),
        Arc::new(instance),
        listen_addr,
    ).await
}
