use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use host_lib::{make_engine, make_store_wasi_ctx, StoreData};

use crate::grpc::{Requirement, RecommendComponent};

wasmtime::component::bindgen!({
    path: "../../components/recommendation/wit",
    world: "recommendation-composed-host-world",
    async: true,
});

use exports::hotel::recommendation::recommendation::Requirement as WitRequirement;

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl RecommendComponent for RecommendationComposedHostWorld {
    type Data = StoreData;

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
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8085".into())
        .parse()?;
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "recommendation-composed.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(
        mongo.database("recommendation-db").collection("recommendation"),
    );

    let engine = make_engine()?;
    let mut linker: Linker<StoreData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    RecommendationComposedHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = StoreData {
        wasi: make_store_wasi_ctx(&data_dir)?,
        table: wasmtime_wasi::ResourceTable::new(),
        collection,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance =
        RecommendationComposedHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance.hotel_recommendation_recommendation()
        .call_init(&mut store)
        .await?;

    println!("recommendation-host [abi] listening on {listen_addr}");

    crate::grpc::serve(
        Arc::new(Mutex::new(store)),
        Arc::new(instance),
        listen_addr,
    ).await
}
