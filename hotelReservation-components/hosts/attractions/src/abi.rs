use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use host_lib::{make_engine, make_store_wasi_ctx, StoreData};

use crate::grpc::AttractionsComponent;

wasmtime::component::bindgen!({
    path: "../../components/attractions/wit",
    world: "attractions-composed-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl AttractionsComponent for AttractionsComposedHostWorld {
    type Data = StoreData;

    async fn nearby_rest(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_attractions_attractions()
            .call_nearby_rest(store, &hotel_id)
            .await?)
    }

    async fn nearby_mus(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_attractions_attractions()
            .call_nearby_mus(store, &hotel_id)
            .await?)
    }

    async fn nearby_cinema(
        &self,
        store: &mut Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_attractions_attractions()
            .call_nearby_cinema(store, &hotel_id)
            .await?)
    }
}

pub async fn run() -> Result<()> {
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8087".into())
        .parse()?;
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "attractions-composed.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(
        mongo.database("attractions-db").collection("attractions"),
    );

    let engine = make_engine()?;
    let mut linker: Linker<StoreData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    AttractionsComposedHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = StoreData {
        wasi: make_store_wasi_ctx(&data_dir)?,
        table: wasmtime_wasi::ResourceTable::new(),
        collection,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance =
        AttractionsComposedHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance.hotel_attractions_attractions()
        .call_init(&mut store)
        .await?;

    println!("attractions-host [abi] listening on {listen_addr}");

    crate::grpc::serve(
        Arc::new(Mutex::new(store)),
        Arc::new(instance),
        listen_addr,
    ).await
}
