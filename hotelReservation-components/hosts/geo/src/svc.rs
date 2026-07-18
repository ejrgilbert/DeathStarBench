use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
use crate::grpc::GeoComponent;

mod store_proto {
    tonic::include_proto!("geo_store");
}
use store_proto::{geo_store_client::GeoStoreClient, LoadRequest};

wasmtime::component::bindgen!({
    path: "../../components/geo/wit",
    world: "geo-host-world",
    async: true,
});

host_lib::svc_host_data!(GeoStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
impl hotel::store::geo_store::Host for HostData {
    async fn load_geo(&mut self) -> Vec<hotel::store::geo_store::Point> {
        let resp = self.store_client.lock().await
            .load_geo(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC geo-store LoadGeo failed")
            .into_inner();
        resp.geo.into_iter().map(|p| hotel::store::geo_store::Point {
            id: p.id, lat: p.lat, lon: p.lon,
        }).collect()
    }
}

struct GeoSvcHost {
    engine:       Arc<wasmtime::Engine>,
    pre:          Arc<GeoHostWorldPre<HostData>>,
    store_client: Arc<Mutex<GeoStoreClient<tonic::transport::Channel>>>,
    cache:        Arc<host_lib::Cache>,
}

#[async_trait::async_trait]
impl GeoComponent for GeoSvcHost {
    async fn nearby(&self, lat: f64, lon: f64) -> Result<Vec<String>> {
        let data = HostData {
            wasi:         host_lib::make_wasi_ctx(),
            table:        ResourceTable::new(),
            store_client: self.store_client.clone(),
            cache:        self.cache.clone(),
        };
        let mut store = Store::new(&self.engine, data);
        let instance = self.pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_geo().call_nearby(&mut store, lat, lon).await?)
    }
}

pub async fn run() -> Result<()> {
    let store_addr  = std::env::var("STORE_ADDR").unwrap_or("http://localhost:8090".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or("0.0.0.0:8089".into()).parse()?;
    let wasm_file = std::env::var("WASM_FILE").unwrap_or("geo.wasm".into());

    let store_client = GeoStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache        = Arc::new(host_lib::Cache::new(Default::default()));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    GeoHostWorld::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(GeoHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    println!("geo-host [svc] listening on {listen_addr}");
    crate::grpc::serve(Arc::new(GeoSvcHost { engine, pre, store_client, cache }), listen_addr).await
}
