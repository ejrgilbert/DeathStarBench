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
    GetNumberRequest, GetNumberResponse,
    GetNumbersRequest, GetNumbersResponse,
    GetReservationsRequest, GetReservationsResponse,
    NumberRec as ProtoNumberRec, ReservationRec as ProtoReservationRec,
};

wasmtime::component::bindgen!({
    path: "../../components/reservation_store/wit",
    world: "reservation-store-host-world",
    imports: { default: async },
    exports: { default: async },
    with: {
        "host:storage/collection.connection": host_lib::MongoCollection,
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
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let nums = instance.hotel_store_reservation_store()
            .call_load_numbers(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
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
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let recs = instance.hotel_store_reservation_store()
            .call_load_reservations(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
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
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        instance.hotel_store_reservation_store()
            .call_insert_reservation(&mut *store,
                &exports::hotel::store::reservation_store::ReservationRec {
                    hotel_id:      r.hotel_id,
                    customer_name: r.customer_name,
                    in_date:       r.in_date,
                    out_date:      r.out_date,
                    number:        r.number,
                },
            ).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(InsertReservationResponse {}))
    }

    // Targeted per-request lookups: real Mongo FindOne/Find in the store guest,
    // matching native. No full-collection load / preload.
    async fn get_number(
        &self, req: Request<GetNumberRequest>,
    ) -> Result<Response<GetNumberResponse>, Status> {
        let hotel_id = req.into_inner().hotel_id;
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let opt = instance.hotel_store_reservation_store()
            .call_get_number(&mut *store, &hotel_id).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(match opt {
            Some(n) => GetNumberResponse {
                found: true,
                num: Some(ProtoNumberRec { hotel_id: n.hotel_id, number_of_room: n.number_of_room }),
            },
            None => GetNumberResponse { found: false, num: None },
        }))
    }

    async fn get_numbers(
        &self, req: Request<GetNumbersRequest>,
    ) -> Result<Response<GetNumbersResponse>, Status> {
        let ids = req.into_inner().hotel_ids;
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let nums = instance.hotel_store_reservation_store()
            .call_get_numbers(&mut *store, &ids).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(GetNumbersResponse {
            numbers: nums.into_iter().map(|n| ProtoNumberRec {
                hotel_id:       n.hotel_id,
                number_of_room: n.number_of_room,
            }).collect(),
        }))
    }

    async fn get_reservations(
        &self, req: Request<GetReservationsRequest>,
    ) -> Result<Response<GetReservationsResponse>, Status> {
        let r = req.into_inner();
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let recs = instance.hotel_store_reservation_store()
            .call_get_reservations(&mut *store, &r.hotel_id, &r.in_date, &r.out_date).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(GetReservationsResponse {
            reservations: recs.into_iter().map(|r| ProtoReservationRec {
                hotel_id:      r.hotel_id,
                customer_name: r.customer_name,
                in_date:       r.in_date,
                out_date:      r.out_date,
                number:        r.number,
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    ReservationStoreHostWorld, ReservationStoreHostWorldPre<host_lib::StoreData>, ReservationStoreServer,
    "reservation-db",
    "0.0.0.0:8101", "reservation-store.wasm", "reservation-host"
);
