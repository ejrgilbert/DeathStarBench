use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use wasmtime::component::Linker;
use wasmtime_wasi::ResourceTable;
use host_lib::make_wasi_ctx;

use crate::grpc::GeoComponent;

mod store_proto {
    tonic::include_proto!("geo_store");
}
use store_proto::{
    geo_store_client::GeoStoreClient,
    InitRequest, LoadRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/geo/wit",
    world: "geo-host-world",
    async: true,
});

pub struct HostData {
    wasi:         wasmtime_wasi::WasiCtx,
    table:        ResourceTable,
    store_client: Arc<Mutex<GeoStoreClient<tonic::transport::Channel>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx { &mut self.wasi }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::geo_data::geo_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client
            .lock().await
            .init(tonic::Request::new(InitRequest {}))
            .await
            .expect("gRPC geo-store Init failed");
    }

    async fn load_geo(&mut self) -> Vec<hotel::geo_data::geo_store::Point> {
        let resp = self.store_client
            .lock().await
            .load_geo(tonic::Request::new(LoadRequest {}))
            .await
            .expect("gRPC geo-store LoadGeo failed")
            .into_inner();
        resp.geo.into_iter().map(|p| hotel::geo_data::geo_store::Point {
            id: p.id, lat: p.lat, lon: p.lon,
        }).collect()
    }
}

#[async_trait::async_trait]
impl GeoComponent for GeoHostWorld {
    type Data = HostData;

    async fn nearby(
        &self,
        store: &mut wasmtime::Store<Self::Data>,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_geo_geo()
            .call_nearby(store, lat, lon)
            .await?)
    }
}

pub async fn run() -> Result<()> {
    let store_addr = std::env::var("STORE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8090".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8089".into())
        .parse()?;
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "geo.wasm".into());

    let store_client = GeoStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    GeoHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = HostData {
        wasi: make_wasi_ctx(),
        table: ResourceTable::new(),
        store_client,
    };
    let mut store = wasmtime::Store::new(&engine, data);

    let component = wasmtime::component::Component::from_file(&engine, &wasm_file)?;
    let instance =
        GeoHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance.hotel_geo_geo()
        .call_init(&mut store)
        .await?;

    println!("geo-host [svc] listening on {listen_addr}");

    crate::grpc::serve(
        Arc::new(Mutex::new(store)),
        Arc::new(instance),
        listen_addr,
    ).await
}
