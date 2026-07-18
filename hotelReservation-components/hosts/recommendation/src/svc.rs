use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
use crate::grpc::{Requirement, RecommendComponent};

mod store_proto {
    tonic::include_proto!("recommendation_store");
}
use store_proto::{
    recommendation_store_client::RecommendationStoreClient,
    LoadHotelsRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/recommendation/wit",
    world: "recommendation-host-world",
    async: true,
});

use exports::hotel::api::recommendation::Requirement as WitRequirement;

host_lib::svc_host_data!(RecommendationStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
impl hotel::store::recommendation_store::Host for HostData {
    async fn load_hotels(&mut self) -> Vec<hotel::store::recommendation_store::Hotel> {
        let resp = self.store_client.lock().await
            .load_hotels(tonic::Request::new(LoadHotelsRequest {})).await
            .expect("gRPC store LoadHotels failed").into_inner();
        resp.hotels.into_iter().map(|h| hotel::store::recommendation_store::Hotel {
            id: h.id, lat: h.lat, lon: h.lon, rate: h.rate, price: h.price,
        }).collect()
    }
}

struct RecommendSvcHost {
    engine:       Arc<wasmtime::Engine>,
    pre:          Arc<RecommendationHostWorldPre<HostData>>,
    store_client: Arc<Mutex<RecommendationStoreClient<tonic::transport::Channel>>>,
    cache:        Arc<host_lib::Cache>,
}

#[async_trait::async_trait]
impl RecommendComponent for RecommendSvcHost {
    async fn recommend(&self, req: Requirement, lat: f64, lon: f64) -> Result<Vec<String>> {
        let data = HostData {
            wasi:         host_lib::make_wasi_ctx(),
            table:        ResourceTable::new(),
            store_client: self.store_client.clone(),
            cache:        self.cache.clone(),
        };
        let mut store = Store::new(&self.engine, data);
        let instance = self.pre.instantiate_async(&mut store).await?;
        let r = match req {
            Requirement::Distance => WitRequirement::Distance,
            Requirement::Rate     => WitRequirement::Rate,
            Requirement::Price    => WitRequirement::Price,
        };
        Ok(instance.hotel_api_recommendation().call_recommend(&mut store, r, lat, lon).await?)
    }
}

pub async fn run() -> Result<()> {
    let store_addr  = std::env::var("STORE_ADDR").unwrap_or("http://localhost:8086".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or("0.0.0.0:8085".into()).parse()?;
    let wasm_file = std::env::var("WASM_FILE").unwrap_or("recommendation.wasm".into());

    let store_client = RecommendationStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache        = Arc::new(host_lib::Cache::new(Default::default()));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    RecommendationHostWorld::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(RecommendationHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    println!("recommendation-host [svc] listening on {listen_addr}");
    crate::grpc::serve(Arc::new(RecommendSvcHost { engine, pre, store_client, cache }), listen_addr).await
}
