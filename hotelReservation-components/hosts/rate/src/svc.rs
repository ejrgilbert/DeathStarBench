use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
use wasmtime::component::Linker;
use crate::grpc::{RateComponent, RatePlan, RoomType};

mod store_proto {
    tonic::include_proto!("rate_store");
}
use store_proto::{rate_store_client::RateStoreClient, InitRequest, LoadRatesRequest};

mod cache_proto {
    tonic::include_proto!("cache");
}
use cache_proto::{cache_client::CacheClient, GetRequest, SetRequest};

wasmtime::component::bindgen!({
    path: "../../components/rate/wit",
    world: "rate-host-world",
    async: true,
});

pub struct HostData {
    pub wasi:         wasmtime_wasi::WasiCtx,
    pub table:        wasmtime_wasi::ResourceTable,
    pub store_client: Arc<Mutex<RateStoreClient<tonic::transport::Channel>>>,
    pub cache_client: Arc<Mutex<CacheClient<tonic::transport::Channel>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::store::rate_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client.lock().await
            .init(tonic::Request::new(InitRequest {})).await
            .expect("gRPC rate-store Init failed");
    }

    async fn load_rates(&mut self) -> Vec<hotel::store::rate_store::RatePlan> {
        let resp = self.store_client.lock().await
            .load_rates(tonic::Request::new(LoadRatesRequest {})).await
            .expect("gRPC rate-store LoadRates failed")
            .into_inner();
        resp.rates.into_iter().map(|r| {
            let rt = r.room_type.unwrap_or_default();
            hotel::store::rate_store::RatePlan {
                hotel_id: r.hotel_id,
                code: r.code,
                in_date: r.in_date,
                out_date: r.out_date,
                room_type: hotel::store::rate_store::RoomType {
                    bookable_rate: rt.bookable_rate,
                    code: rt.code,
                    room_description: rt.room_description,
                    total_rate: rt.total_rate,
                    total_rate_inclusive: rt.total_rate_inclusive,
                },
            }
        }).collect()
    }
}

#[async_trait::async_trait]
impl cache::keyvalue::keyvalue::Host for HostData {
    async fn get(&mut self, key: String) -> Option<Vec<u8>> {
        let resp = self.cache_client.lock().await
            .get(tonic::Request::new(GetRequest { key })).await
            .expect("gRPC cache Get failed")
            .into_inner();
        if resp.found { Some(resp.value) } else { None }
    }

    async fn set(&mut self, key: String, value: Vec<u8>) {
        self.cache_client.lock().await
            .set(tonic::Request::new(SetRequest { key, value })).await
            .expect("gRPC cache Set failed");
    }
}

#[async_trait::async_trait]
impl RateComponent for RateHostWorld {
    type Data = HostData;

    async fn get_rates(
        &self,
        store: &mut Store<HostData>,
        hotel_ids: Vec<String>,
        in_date: String,
        out_date: String,
    ) -> Result<Vec<RatePlan>> {
        let wit_plans = self.hotel_api_rate()
            .call_get_rates(store, &hotel_ids, &in_date, &out_date).await?;
        Ok(wit_plans.into_iter().map(|p| RatePlan {
            hotel_id: p.hotel_id,
            code: p.code,
            in_date: p.in_date,
            out_date: p.out_date,
            room_type: Some(RoomType {
                bookable_rate: p.room_type.bookable_rate,
                code: p.room_type.code,
                room_description: p.room_type.room_description,
                total_rate: p.room_type.total_rate,
                total_rate_inclusive: p.room_type.total_rate_inclusive,
            }),
        }).collect())
    }
}

pub async fn run() -> anyhow::Result<()> {
    use wasmtime::component::Component;

    let store_addr = std::env::var("STORE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8094".into());
    let cache_addr = std::env::var("CACHE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8102".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8093".into())
        .parse()?;
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "rate.wasm".into());

    let store_client = RateStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache_client = CacheClient::connect(cache_addr).await?;
    let cache_client = Arc::new(Mutex::new(cache_client));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    RateHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = HostData {
        wasi:         host_lib::make_wasi_ctx(),
        table:        wasmtime_wasi::ResourceTable::new(),
        store_client,
        cache_client,
    };
    let mut store = wasmtime::Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance = RateHostWorld::instantiate_async(&mut store, &component, &linker).await?;
    instance.hotel_api_rate().call_init(&mut store).await?;

    println!("rate-host [svc] listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
