use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
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

pub struct HostData {
    pub wasi:         wasmtime_wasi::WasiCtx,
    pub table:        wasmtime_wasi::ResourceTable,
    pub store_client: Arc<Mutex<ReviewStoreClient<tonic::transport::Channel>>>,
    pub cache:        Arc<host_lib::Cache>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

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

struct ReviewSvcHost {
    engine:       Arc<wasmtime::Engine>,
    pre:          Arc<ReviewHostWorldPre<HostData>>,
    store_client: Arc<Mutex<ReviewStoreClient<tonic::transport::Channel>>>,
    cache:        Arc<host_lib::Cache>,
}

#[async_trait::async_trait]
impl ReviewComponent for ReviewSvcHost {
    async fn get_reviews(&self, hotel_id: String) -> Result<Vec<ReviewComm>> {
        let data = HostData {
            wasi:         host_lib::make_wasi_ctx(),
            table:        ResourceTable::new(),
            store_client: self.store_client.clone(),
            cache:        self.cache.clone(),
        };
        let mut store = Store::new(&self.engine, data);
        let instance = self.pre.instantiate_async(&mut store).await?;
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

pub async fn run() -> anyhow::Result<()> {
    let store_addr = std::env::var("STORE_ADDR").unwrap_or("http://localhost:8099".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or("0.0.0.0:8098".into()).parse()?;
    let wasm_file = std::env::var("WASM_FILE").unwrap_or("review.wasm".into());

    let store_client = ReviewStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache        = Arc::new(host_lib::Cache::new(Default::default()));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReviewHostWorld::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(ReviewHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    println!("review-host [svc] listening on {listen_addr}");
    crate::grpc::serve(Arc::new(ReviewSvcHost { engine, pre, store_client, cache }), listen_addr).await
}
