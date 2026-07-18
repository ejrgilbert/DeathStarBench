use anyhow::Result;
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

host_lib::svc_host_data!(ReservationStoreClient<tonic::transport::Channel>);
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

type ReservationSvcHost = host_lib::SvcHost<ReservationHostWorldPre<HostData>, ReservationStoreClient<tonic::transport::Channel>>;

#[async_trait::async_trait]
impl ReservationComponent for ReservationSvcHost {
    async fn check_availability(
        &self,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>> {
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
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
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_reservation()
            .call_make_reservation(&mut store, &hotel_id, &customer_name, &in_date, &out_date, room_number).await?)
    }
}

host_lib::run_svc_pre!(
    ReservationHostWorld, ReservationHostWorldPre<HostData>, ReservationStoreClient<tonic::transport::Channel>,
    "http://localhost:8101", "0.0.0.0:8100", "reservation.wasm", "reservation-host [svc]"
);
