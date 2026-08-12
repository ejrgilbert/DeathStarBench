use anyhow::Result;
use crate::grpc::{Requirement, RecommendComponent};

mod store_proto {
    tonic::include_proto!("recommendation_store");
}
use store_proto::{
    recommendation_store_client::RecommendationStoreClient,
    LoadHotelsRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/recommendation/wit",
    world: "recommendation-host-world",
    imports: { default: async },
    exports: { default: async },
});

use exports::hotel::api::recommendation::Requirement as WitRequirement;

host_lib::svc_host_data!(RecommendationStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

impl hotel::store::recommendation_store::Host for HostData {
    async fn load_hotels(&mut self) -> Vec<hotel::store::recommendation_store::Hotel> {
        let resp = self.store_client.lock().await
            .load_hotels(tonic::Request::new(LoadHotelsRequest {})).await
            .expect("gRPC store LoadHotels failed").into_inner();
        resp.hotels.into_iter().map(|h| hotel::store::recommendation_store::Hotel {
            id: h.id, lat: h.lat, lon: h.lon, rate: h.rate, price: h.price,
        }).collect()
    }
}

type RecommendSvcHost = host_lib::SvcHost<RecommendationHostWorldPre<HostData>, RecommendationStoreClient<tonic::transport::Channel>>;

#[async_trait::async_trait]
impl RecommendComponent for RecommendSvcHost {
    async fn recommend(&self, req: Requirement, lat: f64, lon: f64) -> Result<Vec<String>> {
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
        let r = match req {
            Requirement::Distance => WitRequirement::Distance,
            Requirement::Rate     => WitRequirement::Rate,
            Requirement::Price    => WitRequirement::Price,
        };
        Ok(instance.hotel_api_recommendation().call_recommend(&mut store, r, lat, lon).await?)
    }
}

host_lib::run_svc_pre!(
    RecommendationHostWorld, RecommendationHostWorldPre<HostData>, RecommendationStoreClient<tonic::transport::Channel>,
    "http://localhost:8086", "0.0.0.0:8085", "recommendation.wasm", "recommendation-host [svc]"
);
