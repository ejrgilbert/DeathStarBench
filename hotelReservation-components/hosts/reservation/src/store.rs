use anyhow::Result;
use tonic::{Request, Response, Status};

mod proto {
    tonic::include_proto!("reservation_store");
}
use proto::{
    reservation_store_server::{ReservationStore, ReservationStoreServer},
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

host_lib::define_store_service!(ReservationStoreHostWorld, ReservationStoreHostWorldPre<host_lib::StoreData>);

#[tonic::async_trait]
impl ReservationStore for StoreGrpcService {
    async fn load_numbers(
        &self, _: Request<LoadNumbersRequest>,
    ) -> Result<Response<LoadNumbersResponse>, Status> {
        let (mut store, instance) = self.new_instance().await
            .map_err(|e| Status::internal(e.to_string()))?;
        let nums = instance.hotel_store_reservation_store()
            .call_load_numbers(&mut store).await
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
        let (mut store, instance) = self.new_instance().await
            .map_err(|e| Status::internal(e.to_string()))?;
        let recs = instance.hotel_store_reservation_store()
            .call_load_reservations(&mut store).await
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
        let (mut store, instance) = self.new_instance().await
            .map_err(|e| Status::internal(e.to_string()))?;
        instance.hotel_store_reservation_store()
            .call_insert_reservation(&mut store,
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
    ReservationStoreHostWorld, ReservationStoreHostWorldPre<host_lib::StoreData>, ReservationStoreServer,
    "reservation-db",
    "0.0.0.0:8101", "reservation-store.wasm", "reservation-host"
);
