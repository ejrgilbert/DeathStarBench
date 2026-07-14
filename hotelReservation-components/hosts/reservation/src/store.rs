use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::{component::{Component, Linker}, Store};
use wasmtime_wasi::{WasiCtx, ResourceTable};

mod proto {
    tonic::include_proto!("reservation_store");
}
use proto::{
    reservation_store_server::{ReservationStore, ReservationStoreServer},
    InitRequest, InitResponse,
    LoadNumbersRequest, LoadNumbersResponse,
    LoadReservationsRequest, LoadReservationsResponse,
    InsertReservationRequest, InsertReservationResponse,
    NumberRec as ProtoNumberRec, ReservationRec as ProtoReservationRec,
};

wasmtime::component::bindgen!({
    path: "../../components/reservation_store/wit",
    world: "reservation-store-host-world",
    async: true,
});

pub struct ReservationStoreData {
    pub wasi:             WasiCtx,
    pub table:            ResourceTable,
    pub numbers_col:      Arc<mongodb::Collection<bson::Document>>,
    pub reservations_col: Arc<mongodb::Collection<bson::Document>>,
}

impl wasmtime_wasi::WasiView for ReservationStoreData {
    fn ctx(&mut self)   -> &mut WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::reservation_data::numbers_col::Host for ReservationStoreData {
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
impl hotel::reservation_data::reservations_col::Host for ReservationStoreData {
    async fn find_all(&mut self) -> Vec<Vec<u8>> {
        host_lib::mongo_find_all(&self.reservations_col).await.unwrap_or_default()
    }
    async fn insert_one(&mut self, doc: Vec<u8>) {
        host_lib::mongo_insert_one(&self.reservations_col, doc).await.unwrap()
    }
}

type SharedStore    = Arc<Mutex<Store<ReservationStoreData>>>;
type SharedInstance = Arc<ReservationStoreHostWorld>;

struct StoreGrpcService {
    store:    SharedStore,
    instance: SharedInstance,
}

#[tonic::async_trait]
impl ReservationStore for StoreGrpcService {
    async fn init(
        &self, _: Request<InitRequest>,
    ) -> Result<Response<InitResponse>, Status> {
        let mut s = self.store.lock().await;
        self.instance.hotel_reservation_data_reservation_store()
            .call_init(&mut *s).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_numbers(
        &self, _: Request<LoadNumbersRequest>,
    ) -> Result<Response<LoadNumbersResponse>, Status> {
        let mut s = self.store.lock().await;
        let nums = self.instance.hotel_reservation_data_reservation_store()
            .call_load_numbers(&mut *s).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadNumbersResponse {
            numbers: nums.into_iter().map(|n| ProtoNumberRec {
                hotel_id:       n.hotel_id,
                number_of_room: n.number_of_room,
            }).collect(),
        }))
    }

    async fn load_reservations(
        &self, _: Request<LoadReservationsRequest>,
    ) -> Result<Response<LoadReservationsResponse>, Status> {
        let mut s = self.store.lock().await;
        let recs = self.instance.hotel_reservation_data_reservation_store()
            .call_load_reservations(&mut *s).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadReservationsResponse {
            reservations: recs.into_iter().map(|r| ProtoReservationRec {
                hotel_id:      r.hotel_id,
                customer_name: r.customer_name,
                in_date:       r.in_date,
                out_date:      r.out_date,
                number:        r.number,
            }).collect(),
        }))
    }

    async fn insert_reservation(
        &self, req: Request<InsertReservationRequest>,
    ) -> Result<Response<InsertReservationResponse>, Status> {
        let r = req.into_inner();
        let mut s = self.store.lock().await;
        self.instance.hotel_reservation_data_reservation_store()
            .call_insert_reservation(&mut *s,
                &exports::hotel::reservation_data::reservation_store::ReservationRec {
                    hotel_id:      r.hotel_id,
                    customer_name: r.customer_name,
                    in_date:       r.in_date,
                    out_date:      r.out_date,
                    number:        r.number,
                },
            ).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InsertReservationResponse {}))
    }
}

pub async fn run() -> anyhow::Result<()> {
    let mongo_uri   = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8101".into());
    let data_dir    = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file   = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "reservation-store.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let db    = mongo.database("reservation-db");
    let numbers_col      = Arc::new(db.collection::<bson::Document>("number"));
    let reservations_col = Arc::new(db.collection::<bson::Document>("reservation"));

    let engine     = host_lib::make_engine()?;
    let mut linker: Linker<ReservationStoreData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReservationStoreHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = ReservationStoreData {
        wasi:             host_lib::make_store_wasi_ctx(&data_dir)?,
        table:            wasmtime_wasi::ResourceTable::new(),
        numbers_col,
        reservations_col,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance  = ReservationStoreHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_reservation_data_reservation_store()
        .call_init(&mut store).await?;

    let store    = Arc::new(Mutex::new(store));
    let instance = Arc::new(instance);

    println!("reservation-host [store] listening on {listen_addr}");

    Server::builder()
        .add_service(ReservationStoreServer::new(StoreGrpcService { store, instance }))
        .serve(listen_addr.parse()?)
        .await?;
    Ok(())
}
