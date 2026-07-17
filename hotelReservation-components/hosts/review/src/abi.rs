use anyhow::Result;
use wasmtime::Store;
use crate::grpc::{ReviewComponent, ReviewComm, Image};

wasmtime::component::bindgen!({
    path: "../../components/review/wit",
    world: "review-composed-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

use host_lib::StoreData;

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl ReviewComponent for ReviewComposedHostWorld {
    type Data = StoreData;

    async fn get_reviews(
        &self,
        store: &mut Store<StoreData>,
        hotel_id: String,
    ) -> Result<Vec<ReviewComm>> {
        let wit_reviews = self.hotel_api_review()
            .call_get_reviews(store, &hotel_id).await?;
        Ok(wit_reviews.into_iter().map(|r| ReviewComm {
            review_id:   r.review_id,
            hotel_id:    r.hotel_id,
            name:        r.name,
            rating:      r.rating,
            description: r.description,
            images: Some(Image {
                url:     r.image.url,
                default: r.image.default,
            }),
        }).collect())
    }
}

host_lib::run_abi!(
    ReviewComposedHostWorld, hotel_api_review,
    "review-db",
    "0.0.0.0:8098", "review-composed.wasm", "review-host"
);
