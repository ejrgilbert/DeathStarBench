use anyhow::Result;
use crate::grpc::{ReviewComponent, ReviewComm, Image};

mod store_proto {
    tonic::include_proto!("review_store");
}
use store_proto::{review_store_client::ReviewStoreClient, LoadReviewsRequest};

wasmtime::component::bindgen!({
    path: "../../components/review/wit",
    world: "review-host-world",
    async: true,
});

host_lib::svc_host_data!(ReviewStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
impl hotel::store::review_store::Host for HostData {
    async fn load_reviews(&mut self) -> Vec<hotel::store::review_store::Review> {
        let resp = self.store_client.lock().await
            .load_reviews(tonic::Request::new(LoadReviewsRequest {})).await
            .expect("gRPC review-store LoadReviews failed").into_inner();
        resp.reviews.into_iter().map(|r| {
            let img = r.image.unwrap_or_default();
            hotel::store::review_store::Review {
                review_id:   r.review_id,
                hotel_id:    r.hotel_id,
                name:        r.name,
                rating:      r.rating,
                description: r.description,
                image: hotel::store::review_store::Image {
                    url:     img.url,
                    default: img.default,
                },
            }
        }).collect()
    }
}

type ReviewSvcHost = host_lib::SvcHost<ReviewHostWorldPre<HostData>, ReviewStoreClient<tonic::transport::Channel>>;

#[async_trait::async_trait]
impl ReviewComponent for ReviewSvcHost {
    async fn get_reviews(&self, hotel_id: String) -> Result<Vec<ReviewComm>> {
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
        let wit_reviews = instance.hotel_api_review()
            .call_get_reviews(&mut store, &hotel_id).await?;
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

host_lib::run_svc_pre!(
    ReviewHostWorld, ReviewHostWorldPre<HostData>, ReviewStoreClient<tonic::transport::Channel>,
    "http://localhost:8099", "0.0.0.0:8098", "review.wasm", "review-host [svc]"
);
