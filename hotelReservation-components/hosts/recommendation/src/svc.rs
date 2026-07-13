use anyhow::Result;
use wasmtime::Store;
use crate::grpc::{Requirement, RecommendComponent};

mod store_proto {
    tonic::include_proto!("recommendation_store");
}
use store_proto::{
    recommendation_store_client::RecommendationStoreClient,
    InitRequest, LoadHotelsRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/recommendation/wit",
    world: "recommendation-host-world",
    async: true,
});

use exports::hotel::recommendation::recommendation::Requirement as WitRequirement;

host_lib::svc_host_data!(RecommendationStoreClient<tonic::transport::Channel>);

#[async_trait::async_trait]
impl hotel::recommendation_data::recommendation_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client.lock().await
            .init(tonic::Request::new(InitRequest {})).await
            .expect("gRPC store Init failed");
    }

    async fn load_hotels(&mut self) -> Vec<hotel::recommendation_data::recommendation_store::Hotel> {
        let resp = self.store_client.lock().await
            .load_hotels(tonic::Request::new(LoadHotelsRequest {})).await
            .expect("gRPC store LoadHotels failed")
            .into_inner();
        resp.hotels.into_iter().map(|h| hotel::recommendation_data::recommendation_store::Hotel {
            id: h.id, lat: h.lat, lon: h.lon, rate: h.rate, price: h.price,
        }).collect()
    }
}

#[async_trait::async_trait]
impl RecommendComponent for RecommendationHostWorld {
    type Data = HostData;

    async fn recommend(
        &self,
        store: &mut Store<HostData>,
        req: Requirement,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>> {
        let r = match req {
            Requirement::Distance => WitRequirement::Distance,
            Requirement::Rate    => WitRequirement::Rate,
            Requirement::Price   => WitRequirement::Price,
        };
        Ok(self.hotel_recommendation_recommendation()
            .call_recommend(store, r, lat, lon).await?)
    }
}

host_lib::run_svc!(
    RecommendationHostWorld,
    RecommendationStoreClient<tonic::transport::Channel>,
    hotel_recommendation_recommendation,
    "http://localhost:8086",
    "0.0.0.0:8085",
    "recommendation.wasm",
    "recommendation-host"
);
