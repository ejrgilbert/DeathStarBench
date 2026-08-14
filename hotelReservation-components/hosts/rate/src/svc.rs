use anyhow::Result;
use crate::grpc::{RateComponent, RatePlan, RoomType};

mod store_proto {
    tonic::include_proto!("rate_store");
}
use store_proto::{rate_store_client::RateStoreClient, LoadRatesRequest};

wasmtime::component::bindgen!({
    path: "../../components/rate/wit",
    world: "rate-host-world",
    imports: { default: async },
    exports: { default: async },
});

host_lib::svc_host_data!(RateStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_svc_grpc!(HostData);

impl hotel::store::rate_store::Host for HostData {
    async fn load_rates(&mut self) -> Vec<hotel::store::rate_store::RatePlan> {
        let resp = self.store_client.clone()
            .load_rates(tonic::Request::new(LoadRatesRequest {})).await
            .expect("gRPC rate-store LoadRates failed").into_inner();
        resp.rates.into_iter().map(|r| {
            let rt = r.room_type.unwrap_or_default();
            hotel::store::rate_store::RatePlan {
                hotel_id: r.hotel_id,
                code: r.code,
                in_date: r.in_date,
                out_date: r.out_date,
                room_type: hotel::store::rate_store::RoomType {
                    bookable_rate:        rt.bookable_rate,
                    code:                 rt.code,
                    room_description:     rt.room_description,
                    total_rate:           rt.total_rate,
                    total_rate_inclusive: rt.total_rate_inclusive,
                },
            }
        }).collect()
    }
}

type RateSvcHost = host_lib::PooledSvcHost<RateStoreClient<tonic::transport::Channel>, RateHostWorld>;

#[async_trait::async_trait]
impl RateComponent for RateSvcHost {
    async fn get_rates(&self, hotel_ids: Vec<String>, in_date: String, out_date: String) -> Result<Vec<RatePlan>> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let wit_plans = instance.hotel_api_rate()
            .call_get_rates(&mut *store, &hotel_ids, &in_date, &out_date).await?;
        checked.commit();
        Ok(wit_plans.into_iter().map(|p| RatePlan {
            hotel_id: p.hotel_id,
            code:     p.code,
            in_date:  p.in_date,
            out_date: p.out_date,
            room_type: Some(RoomType {
                bookable_rate:        p.room_type.bookable_rate,
                code:                 p.room_type.code,
                room_description:     p.room_type.room_description,
                total_rate:           p.room_type.total_rate,
                total_rate_inclusive: p.room_type.total_rate_inclusive,
            }),
        }).collect())
    }
}

host_lib::run_svc_pre!(
    RateHostWorld, RateHostWorldPre<HostData>, RateStoreClient<tonic::transport::Channel>,
    "http://localhost:8094", "0.0.0.0:8093", "rate.wasm", "rate-host [svc]"
);
