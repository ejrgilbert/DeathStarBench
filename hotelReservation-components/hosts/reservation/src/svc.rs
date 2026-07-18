use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
use crate::grpc::ReservationComponent;

mod store_proto {
    tonic::include_proto!("reservation_store");
}
use store_proto::{
    reservation_store_client::ReservationStoreClient,
    LoadNumbersRequest, LoadReservationsRequest, InsertReservationRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/reservation/wit",
    world: "reservation-host-world",
    async: true,
});

pub struct HostData {
    pub wasi:         wasmtime_wasi::WasiCtx,
    pub table:        wasmtime_wasi::ResourceTable,
    pub store_client: Arc<Mutex<ReservationStoreClient<tonic::transport::Channel>>>,
    pub cache:        Arc<host_lib::Cache>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
impl hotel::store::reservation_store::Host for HostData {
    async fn load_numbers(&mut self) -> Vec<hotel::store::reservation_store::NumberRec> {
        let resp = self.store_client.lock().await
            .load_numbers(tonic::Request::new(LoadNumbersRequest {})).await
            .expect("gRPC reservation-store LoadNumbers failed").into_inner();
        resp.numbers.into_iter().map(|n| hotel::store::reservation_store::NumberRec {
            hotel_id:       n.hotel_id,
            number_of_room: n.number_of_room,
        }).collect()
    }

    async fn load_reservations(&mut self) -> Vec<hotel::store::reservation_store::ReservationRec> {
        let resp = self.store_client.lock().await
            .load_reservations(tonic::Request::new(LoadReservationsRequest {})).await
            .expect("gRPC reservation-store LoadReservations failed").into_inner();
        resp.reservations.into_iter().map(|r| hotel::store::reservation_store::ReservationRec {
            hotel_id:      r.hotel_id,
            customer_name: r.customer_name,
            in_date:       r.in_date,
            out_date:      r.out_date,
            number:        r.number,
        }).collect()
    }

    async fn insert_reservation(&mut self, r: hotel::store::reservation_store::ReservationRec) {
        self.store_client.lock().await
            .insert_reservation(tonic::Request::new(InsertReservationRequest {
                hotel_id:      r.hotel_id,
                customer_name: r.customer_name,
                in_date:       r.in_date,
                out_date:      r.out_date,
                number:        r.number,
            })).await
            .expect("gRPC reservation-store InsertReservation failed");
    }
}

struct ReservationSvcHost {
    engine:       Arc<wasmtime::Engine>,
    pre:          Arc<ReservationHostWorldPre<HostData>>,
    store_client: Arc<Mutex<ReservationStoreClient<tonic::transport::Channel>>>,
    cache:        Arc<host_lib::Cache>,
}

#[async_trait::async_trait]
impl ReservationComponent for ReservationSvcHost {
    async fn check_availability(
        &self,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>> {
        let (mut store, instance) = self.make_instance().await?;
        Ok(instance.hotel_api_reservation()
            .call_check_availability(&mut store, &hotel_ids, &in_date, &out_date, room_number).await?)
    }

    async fn make_reservation(
        &self,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Result<Vec<String>> {
        let (mut store, instance) = self.make_instance().await?;
        Ok(instance.hotel_api_reservation()
            .call_make_reservation(&mut store, &hotel_id, &customer_name, &in_date, &out_date, room_number).await?)
    }
}

impl ReservationSvcHost {
    async fn make_instance(&self) -> Result<(Store<HostData>, ReservationHostWorld)> {
        let data = HostData {
            wasi:         host_lib::make_wasi_ctx(),
            table:        ResourceTable::new(),
            store_client: self.store_client.clone(),
            cache:        self.cache.clone(),
        };
        let mut store = Store::new(&self.engine, data);
        let instance = self.pre.instantiate_async(&mut store).await?;
        Ok((store, instance))
    }
}

pub async fn run() -> anyhow::Result<()> {
    let store_addr  = std::env::var("STORE_ADDR").unwrap_or("http://localhost:8101".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or("0.0.0.0:8100".into()).parse()?;
    let wasm_file = std::env::var("WASM_FILE").unwrap_or("reservation.wasm".into());

    let store_client = ReservationStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache        = Arc::new(host_lib::Cache::new(Default::default()));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReservationHostWorld::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(ReservationHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    println!("reservation-host [svc] listening on {listen_addr}");
    crate::grpc::serve(Arc::new(ReservationSvcHost { engine, pre, store_client, cache }), listen_addr).await
}
