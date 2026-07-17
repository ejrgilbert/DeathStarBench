use anyhow::Result;
use tonic::{Request, Response, Status};

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
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

use host_lib::StoreData;

host_lib::impl_collection_host!(StoreData);

host_lib::define_store_service!(ReservationStoreHostWorld);

#[tonic::async_trait]
impl ReservationStore for StoreGrpcService {
    async fn init(
        &self, _: Request<InitRequest>,
    ) -> Result<Response<InitResponse>, Status> {
        let mut s = self.store.lock().await;
        self.instance.hotel_store_reservation_store()
            .call_init(&mut *s).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_numbers(
        &self, _: Request<LoadNumbersRequest>,
    ) -> Result<Response<LoadNumbersResponse>, Status> {
        let mut s = self.store.lock().await;
        let nums = self.instance.hotel_store_reservation_store()
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
        let recs = self.instance.hotel_store_reservation_store()
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
        self.instance.hotel_store_reservation_store()
            .call_insert_reservation(&mut *s,
                &exports::hotel::store::reservation_store::ReservationRec {
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

host_lib::run_store!(
    ReservationStoreHostWorld, ReservationStoreServer,
    hotel_store_reservation_store,
    "reservation-db",
    "0.0.0.0:8101", "reservation-store.wasm", "reservation-host"
);
