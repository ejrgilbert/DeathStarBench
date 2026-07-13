use anyhow::Result;
use wasmtime::Store;
use host_lib::StoreData;
use crate::grpc::{Requirement, RecommendComponent};

wasmtime::component::bindgen!({
    path: "../../components/recommendation/wit",
    world: "recommendation-composed-host-world",
    async: true,
});

use exports::hotel::recommendation::recommendation::Requirement as WitRequirement;

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl RecommendComponent for RecommendationComposedHostWorld {
    type Data = StoreData;

    async fn recommend(
        &self,
        store: &mut Store<Self::Data>,
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

host_lib::run_abi!(
    RecommendationComposedHostWorld, hotel_recommendation_recommendation,
    "recommendation-db", "recommendation",
    "0.0.0.0:8085", "recommendation-composed.wasm", "recommendation-host"
);
