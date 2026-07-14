use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::Linker;
use crate::grpc::{ReviewComponent, ReviewComm, Image};

mod store_proto {
    tonic::include_proto!("review_store");
}
use store_proto::{review_store_client::ReviewStoreClient, InitRequest, LoadReviewsRequest};

wasmtime::component::bindgen!({
    path: "../../components/review/wit",
    world: "review-host-world",
    async: true,
});

pub struct HostData {
    pub wasi:         wasmtime_wasi::WasiCtx,
    pub table:        wasmtime_wasi::ResourceTable,
    pub store_client: Arc<Mutex<ReviewStoreClient<tonic::transport::Channel>>>,
    pub cache:        Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::review_data::review_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client.lock().await
            .init(tonic::Request::new(InitRequest {})).await
            .expect("gRPC review-store Init failed");
    }

    async fn load_reviews(&mut self) -> Vec<hotel::review_data::review_store::Review> {
        let resp = self.store_client.lock().await
            .load_reviews(tonic::Request::new(LoadReviewsRequest {})).await
            .expect("gRPC review-store LoadReviews failed")
            .into_inner();
        resp.reviews.into_iter().map(|r| {
            let img = r.image.unwrap_or_default();
            hotel::review_data::review_store::Review {
                review_id:   r.review_id,
                hotel_id:    r.hotel_id,
                name:        r.name,
                rating:      r.rating,
                description: r.description,
                image: hotel::review_data::review_store::Image {
                    url:     img.url,
                    default: img.default,
                },
            }
        }).collect()
    }
}

#[async_trait::async_trait]
impl host::cache::keyvalue::Host for HostData {
    async fn get(&mut self, key: String) -> Option<Vec<u8>> {
        self.cache.lock().await.get(&key).cloned()
    }

    async fn set(&mut self, key: String, value: Vec<u8>) {
        self.cache.lock().await.insert(key, value);
    }
}

#[async_trait::async_trait]
impl ReviewComponent for ReviewHostWorld {
    type Data = HostData;

    async fn get_reviews(
        &self,
        store: &mut Store<HostData>,
        hotel_id: String,
    ) -> Result<Vec<ReviewComm>> {
        let wit_reviews = self.hotel_review_review()
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

pub async fn run() -> anyhow::Result<()> {
    use wasmtime::component::Component;

    let store_addr = std::env::var("STORE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8099".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8098".into())
        .parse()?;
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "review.wasm".into());

    let store_client = ReviewStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    ReviewHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = HostData {
        wasi:         host_lib::make_wasi_ctx(),
        table:        wasmtime_wasi::ResourceTable::new(),
        store_client,
        cache,
    };
    let mut store = wasmtime::Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance = ReviewHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_review_review().call_init(&mut store).await?;

    println!("review-host [svc] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
