use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("review_store");
}
use proto::{
    review_store_server::{ReviewStore, ReviewStoreServer},
    Review as ProtoReview, Image as ProtoImage,
    InitRequest, InitResponse, LoadReviewsRequest, LoadReviewsResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/review_store/wit",
    world: "review-store-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(ReviewStoreHostWorld);

#[tonic::async_trait]
impl ReviewStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_store_review_store()
            .call_init(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_reviews(
        &self,
        _req: Request<LoadReviewsRequest>,
    ) -> Result<Response<LoadReviewsResponse>, Status> {
        let mut store = self.store.lock().await;
        let wit_reviews = self.instance
            .hotel_store_review_store()
            .call_load_reviews(&mut *store).await
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
    ReviewStoreHostWorld, ReviewStoreServer, hotel_store_review_store,
    "review-db", "reviews",
    "0.0.0.0:8099", "review-store.wasm", "review-host"
);
