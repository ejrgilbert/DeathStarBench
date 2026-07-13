use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::{Component, Linker};
use crate::grpc::{RateComponent, RatePlan, RoomType};

wasmtime::component::bindgen!({
    path: "../../components/rate/wit",
    world: "rate-composed-host-world",
    async: true,
});

pub struct AbiData {
    pub wasi:       wasmtime_wasi::WasiCtx,
    pub table:      wasmtime_wasi::ResourceTable,
    pub collection: Arc<mongodb::Collection<bson::Document>>,
    pub cache:      Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl wasmtime_wasi::WasiView for AbiData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

host_lib::impl_collection_host!(AbiData);

#[async_trait::async_trait]
impl host::cache::keyvalue::Host for AbiData {
    async fn get(&mut self, key: String) -> Option<Vec<u8>> {
        self.cache.lock().await.get(&key).cloned()
    }

    async fn set(&mut self, key: String, value: Vec<u8>) {
        self.cache.lock().await.insert(key, value);
    }
}

#[async_trait::async_trait]
impl RateComponent for RateComposedHostWorld {
    type Data = AbiData;

    async fn get_rates(
        &self,
        store: &mut Store<AbiData>,
        hotel_ids: Vec<String>,
        in_date: String,
        out_date: String,
    ) -> Result<Vec<RatePlan>> {
        let wit_plans = self.hotel_rate_rate()
            .call_get_rates(store, &hotel_ids, &in_date, &out_date).await?;
        Ok(wit_plans.into_iter().map(|p| RatePlan {
            hotel_id: p.hotel_id,
            code: p.code,
            in_date: p.in_date,
            out_date: p.out_date,
            room_type: Some(RoomType {
                bookable_rate: p.room_type.bookable_rate,
                code: p.room_type.code,
                room_description: p.room_type.room_description,
                total_rate: p.room_type.total_rate,
                total_rate_inclusive: p.room_type.total_rate_inclusive,
            }),
        }).collect())
    }
}

pub async fn run() -> anyhow::Result<()> {
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8093".into())
        .parse()?;
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "rate-composed.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(mongo.database("rate-db").collection("inventory"));
    let cache: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<AbiData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    RateComposedHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = AbiData {
        wasi:       host_lib::make_store_wasi_ctx(&data_dir)?,
        table:      wasmtime_wasi::ResourceTable::new(),
        collection,
        cache,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance = RateComposedHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_rate_rate().call_init(&mut store).await?;

    println!("rate-host [abi] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
