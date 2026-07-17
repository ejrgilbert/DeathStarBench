use anyhow::Result;
use wasmtime::Store;
use crate::grpc::ReservationComponent;

wasmtime::component::bindgen!({
    path: "../../components/reservation/wit",
    world: "reservation-composed-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

use host_lib::StoreData;

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl ReservationComponent for ReservationComposedHostWorld {
    type Data = StoreData;

    async fn check_availability(
        &self,
        store:       &mut Store<StoreData>,
        hotel_ids:   Vec<String>,
        in_date:     String,
        out_date:    String,
        room_number: i32,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_reservation()
            .call_check_availability(store, &hotel_ids, &in_date, &out_date, room_number)
            .await?)
    }

    async fn make_reservation(
        &self,
        store:         &mut Store<StoreData>,
        hotel_id:      String,
        customer_name: String,
        in_date:       String,
        out_date:      String,
        room_number:   i32,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_reservation()
            .call_make_reservation(store, &hotel_id, &customer_name, &in_date, &out_date, room_number)
            .await?)
    }
}

host_lib::run_abi!(
    ReservationComposedHostWorld, hotel_api_reservation,
    "reservation-db",
    "0.0.0.0:8100", "reservation-composed.wasm", "reservation-host"
);
