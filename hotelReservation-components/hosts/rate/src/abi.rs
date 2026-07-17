use anyhow::Result;
use wasmtime::Store;
use crate::grpc::{RateComponent, RatePlan, RoomType};

wasmtime::component::bindgen!({
    path: "../../components/rate/wit",
    world: "rate-composed-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

use host_lib::StoreData;

host_lib::impl_collection_host!(StoreData);
host_lib::impl_cache_host!(StoreData);

#[async_trait::async_trait]
impl RateComponent for RateComposedHostWorld {
    type Data = StoreData;

    async fn get_rates(
        &self,
        store: &mut Store<StoreData>,
        hotel_ids: Vec<String>,
        in_date: String,
        out_date: String,
    ) -> Result<Vec<RatePlan>> {
        let wit_plans = self.hotel_api_rate()
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

host_lib::run_abi!(
    RateComposedHostWorld, hotel_api_rate,
    "rate-db",
    "0.0.0.0:8093", "rate-composed.wasm", "rate-host"
);
