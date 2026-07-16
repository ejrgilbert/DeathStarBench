use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::{Component, Linker};
use crate::grpc::ReservationComponent;

wasmtime::component::bindgen!({
    path: "../../components/reservation/wit",
    world: "reservation-composed-host-world",
    async: true,
});

pub struct AbiData {
    pub wasi:             wasmtime_wasi::WasiCtx,
    pub table:            wasmtime_wasi::ResourceTable,
    pub numbers_col:      Arc<mongodb::Collection<bson::Document>>,
    pub reservations_col: Arc<mongodb::Collection<bson::Document>>,
}

impl wasmtime_wasi::WasiView for AbiData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::reservation_data::numbers_col::Host for AbiData {
    async fn count(&mut self) -> u64 {
        host_lib::mongo_count(&self.numbers_col).await.unwrap_or(0)
    }
    async fn find_all(&mut self) -> Vec<Vec<u8>> {
        host_lib::mongo_find_all(&self.numbers_col).await.unwrap_or_default()
    }
    async fn insert_many(&mut self, docs: Vec<Vec<u8>>) {
        host_lib::mongo_insert_many(&self.numbers_col, docs).await.unwrap()
    }
}

#[async_trait::async_trait]
impl hotel::reservation_data::reservations_col::Host for AbiData {
    async fn find_all(&mut self) -> Vec<Vec<u8>> {
        host_lib::mongo_find_all(&self.reservations_col).await.unwrap_or_default()
    }
    async fn insert_one(&mut self, doc: Vec<u8>) {
        host_lib::mongo_insert_one(&self.reservations_col, doc).await.unwrap()
    }
}

#[async_trait::async_trait]
impl ReservationComponent for ReservationComposedHostWorld {
    type Data = AbiData;

    async fn check_availability(
        &self,
        store:       &mut Store<AbiData>,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_reservation_reservation()
            .call_check_availability(store, &hotel_ids, &in_date, &out_date, room_number)
            .await?)
    }

    async fn make_reservation(
        &self,
        store:         &mut Store<AbiData>,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_reservation_reservation()
            .call_make_reservation(store, &hotel_id, &customer_name, &in_date, &out_date, room_number)
            .await?)
    }
}

pub async fn run() -> anyhow::Result<()> {
    let mongo_uri   = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8100".into())
        .parse()?;
    let data_dir    = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file   = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "reservation-composed.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let db    = mongo.database("reservation-db");
    let numbers_col      = Arc::new(db.collection::<bson::Document>("number"));
    let reservations_col = Arc::new(db.collection::<bson::Document>("reservation"));

    let engine     = host_lib::make_engine()?;
    let mut linker: Linker<AbiData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReservationComposedHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = AbiData {
        wasi:             host_lib::make_store_wasi_ctx(&data_dir)?,
        table:            wasmtime_wasi::ResourceTable::new(),
        numbers_col,
        reservations_col,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance  = ReservationComposedHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_reservation_reservation().call_init(&mut store).await?;

    println!("reservation-host [abi] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
