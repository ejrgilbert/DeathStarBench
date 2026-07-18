use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("review_store");
}
use proto::{
    review_store_server::{ReviewStore, ReviewStoreServer},
    Review as ProtoReview, Image as ProtoImage,
    LoadReviewsRequest, LoadReviewsResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/review_store/wit",
    world: "review-store-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(ReviewStoreHostWorld, ReviewStoreHostWorldPre<host_lib::StoreData>);

#[tonic::async_trait]
impl ReviewStore for StoreGrpcService {
    async fn load_reviews(
        &self,
        _req: Request<LoadReviewsRequest>,
    ) -> Result<Response<LoadReviewsResponse>, Status> {
        let (mut store, instance) = self.new_instance().await
            .map_err(|e| Status::internal(e.to_string()))?;
        let wit_reviews = instance
            .hotel_store_review_store()
            .call_load_reviews(&mut store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadReviewsResponse {
            reviews: wit_reviews.into_iter().map(|r| ProtoReview {
                review_id:   r.review_id,
                hotel_id:    r.hotel_id,
                name:        r.name,
                rating:      r.rating,
                description: r.description,
                image: Some(ProtoImage {
                    url:     r.image.url,
                    default: r.image.default,
                }),
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    ReviewStoreHostWorld, ReviewStoreHostWorldPre<host_lib::StoreData>, ReviewStoreServer,
    "review-db",
    "0.0.0.0:8099", "review-store.wasm", "review-host"
);
