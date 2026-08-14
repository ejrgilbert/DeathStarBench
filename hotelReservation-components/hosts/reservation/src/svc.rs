use anyhow::Result;
use crate::grpc::ReservationComponent;

mod store_proto {
    tonic::include_proto!("reservation_store");
}
use store_proto::{
    reservation_store_client::ReservationStoreClient,
    LoadNumbersRequest, LoadReservationsRequest, InsertReservationRequest,
    GetNumberRequest, GetNumbersRequest, GetReservationsRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/reservation/wit",
    world: "reservation-host-world",
    imports: { default: async },
    exports: { default: async },
});

host_lib::svc_host_data!(ReservationStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_svc_grpc!(HostData);

impl hotel::store::reservation_store::Host for HostData {
    async fn load_numbers(&mut self) -> Vec<hotel::store::reservation_store::NumberRec> {
        let resp = self.store_client.clone()
            .load_numbers(tonic::Request::new(LoadNumbersRequest {})).await
            .expect("gRPC reservation-store LoadNumbers failed").into_inner();
        resp.numbers.into_iter().map(|n| hotel::store::reservation_store::NumberRec {
            hotel_id:       n.hotel_id,
            number_of_room: n.number_of_room,
        }).collect()
    }

    async fn load_reservations(&mut self) -> Vec<hotel::store::reservation_store::ReservationRec> {
        let resp = self.store_client.clone()
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
        self.store_client.clone()
            .insert_reservation(tonic::Request::new(InsertReservationRequest {
                hotel_id:      r.hotel_id,
                customer_name: r.customer_name,
                in_date:       r.in_date,
                out_date:      r.out_date,
                number:        r.number,
            })).await
            .expect("gRPC reservation-store InsertReservation failed");
    }

    // Targeted per-request lookups via the store's GetNumber/GetReservations RPCs
    // (Mongo FindOne/Find), matching native. The ABI path uses the composed
    // store's `get-number`/`get-reservations` directly.
    async fn get_number(&mut self, hotel_id: String) -> Option<hotel::store::reservation_store::NumberRec> {
        let resp = self.store_client.clone()
            .get_number(tonic::Request::new(GetNumberRequest { hotel_id })).await
            .ok()?
            .into_inner();
        if !resp.found {
            return None;
        }
        let n = resp.num?;
        Some(hotel::store::reservation_store::NumberRec {
            hotel_id:       n.hotel_id,
            number_of_room: n.number_of_room,
        })
    }

    async fn get_numbers(
        &mut self,
        ids: Vec<String>,
    ) -> Vec<hotel::store::reservation_store::NumberRec> {
        let resp = match self.store_client.clone()
            .get_numbers(tonic::Request::new(GetNumbersRequest { hotel_ids: ids })).await
        {
            Ok(r) => r.into_inner(),
            Err(_) => return Vec::new(),
        };
        resp.numbers.into_iter().map(|n| hotel::store::reservation_store::NumberRec {
            hotel_id:       n.hotel_id,
            number_of_room: n.number_of_room,
        }).collect()
    }

    async fn get_reservations(
        &mut self,
        hotel_id: String,
        in_date: String,
        out_date: String,
    ) -> Vec<hotel::store::reservation_store::ReservationRec> {
        let resp = match self.store_client.clone()
            .get_reservations(tonic::Request::new(GetReservationsRequest { hotel_id, in_date, out_date })).await
        {
            Ok(r) => r.into_inner(),
            Err(_) => return Vec::new(),
        };
        resp.reservations.into_iter().map(|r| hotel::store::reservation_store::ReservationRec {
            hotel_id:      r.hotel_id,
            customer_name: r.customer_name,
            in_date:       r.in_date,
            out_date:      r.out_date,
            number:        r.number,
        }).collect()
    }
}

type ReservationSvcHost = host_lib::PooledSvcHost<ReservationStoreClient<tonic::transport::Channel>, ReservationHostWorld>;

#[async_trait::async_trait]
impl ReservationComponent for ReservationSvcHost {
    async fn check_availability(
        &self,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let out = instance.hotel_api_reservation()
            .call_check_availability(&mut *store, &hotel_ids, &in_date, &out_date, room_number).await?;
        checked.commit();
        Ok(out)
    }

    async fn make_reservation(
        &self,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Result<Vec<String>> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let out = instance.hotel_api_reservation()
            .call_make_reservation(&mut *store, &hotel_id, &customer_name, &in_date, &out_date, room_number).await?;
        checked.commit();
        Ok(out)
    }
}

host_lib::run_svc_pre!(
    ReservationHostWorld, ReservationHostWorldPre<HostData>, ReservationStoreClient<tonic::transport::Channel>,
    "http://localhost:8101", "0.0.0.0:8100", "reservation.wasm", "reservation-host [svc]"
);
