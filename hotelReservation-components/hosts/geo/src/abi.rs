use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use host_lib::{make_engine, make_store_wasi_ctx, StoreData};

use crate::grpc::GeoComponent;

wasmtime::component::bindgen!({
    path: "../../components/geo/wit",
    world: "geo-composed-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl GeoComponent for GeoComposedHostWorld {
    type Data = StoreData;

    async fn nearby(
        &self,
        store: &mut Store<Self::Data>,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_geo_geo()
            .call_nearby(store, lat, lon)
            .await?)
    }
}

pub async fn run() -> Result<()> {
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8089".into())
        .parse()?;
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "geo-composed.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(
        mongo.database("geo-db").collection("geo"),
    );

    let engine = make_engine()?;
    let mut linker: Linker<StoreData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    GeoComposedHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = StoreData {
        wasi: make_store_wasi_ctx(&data_dir)?,
        table: wasmtime_wasi::ResourceTable::new(),
        collection,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance =
        GeoComposedHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance.hotel_geo_geo()
        .call_init(&mut store)
        .await?;

    println!("geo-host [abi] listening on {listen_addr}");

    crate::grpc::serve(
        Arc::new(Mutex::new(store)),
        Arc::new(instance),
        listen_addr,
    ).await
}
