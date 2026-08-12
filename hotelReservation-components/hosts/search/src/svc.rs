use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
use crate::grpc::SearchComponent;

mod geo_proto {
    tonic::include_proto!("geo");
}
use geo_proto::geo_client::GeoClient;

mod rate_proto {
    tonic::include_proto!("rate");
}
use rate_proto::rate_client::RateClient;

wasmtime::component::bindgen!({
    path: "../../components/search/wit",
    world: "search-host-world",
    imports: { default: async },
    exports: { default: async },
});

pub struct HostData {
    pub wasi:        wasmtime_wasi::WasiCtx,
    pub table:       wasmtime_wasi::ResourceTable,
    pub geo_client:  Arc<Mutex<GeoClient<tonic::transport::Channel>>>,
    pub rate_client: Arc<Mutex<RateClient<tonic::transport::Channel>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl hotel::api::geo::Host for HostData {
    async fn nearby(&mut self, lat: f64, lon: f64) -> Vec<String> {
        self.geo_client.lock().await
            .nearby(tonic::Request::new(geo_proto::Request { lat, lon }))
            .await
            .expect("gRPC geo Nearby failed")
            .into_inner()
            .ids
    }
}

impl hotel::api::rate::Host for HostData {
    async fn get_rates(
        &mut self,
        hotel_ids: Vec<String>,
        in_date: String,
        out_date: String,
    ) -> Vec<hotel::api::rate::RatePlan> {
        let resp = self.rate_client.lock().await
            .get_rates(tonic::Request::new(rate_proto::Request {
                hotel_ids,
                in_date,
                out_date,
            }))
            .await
            .expect("gRPC rate GetRates failed")
            .into_inner();

        resp.rate_plans.into_iter().map(|rp| {
            let rt = rp.room_type.unwrap_or_default();
            hotel::api::rate::RatePlan {
                hotel_id: rp.hotel_id,
                code:     rp.code,
                in_date:  rp.in_date,
                out_date: rp.out_date,
                room_type: hotel::api::rate::RoomType {
                    bookable_rate:        rt.bookable_rate,
                    code:                 rt.code,
                    room_description:     rt.room_description,
                    total_rate:           rt.total_rate,
                    total_rate_inclusive: rt.total_rate_inclusive,
                },
            }
        }).collect()
    }
}

struct SearchSvcHost {
    engine:      Arc<wasmtime::Engine>,
    pre:         Arc<SearchHostWorldPre<HostData>>,
    geo_client:  Arc<Mutex<GeoClient<tonic::transport::Channel>>>,
    rate_client: Arc<Mutex<RateClient<tonic::transport::Channel>>>,
}

#[async_trait::async_trait]
impl SearchComponent for SearchSvcHost {
    async fn nearby(&self, lat: f64, lon: f64, in_date: String, out_date: String) -> Result<Vec<String>> {
        let data = HostData {
            wasi:        host_lib::make_wasi_ctx(),
            table:       ResourceTable::new(),
            geo_client:  self.geo_client.clone(),
            rate_client: self.rate_client.clone(),
        };
        let mut store = Store::new(&self.engine, data);
        let instance = self.pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_search()
            .call_nearby(&mut store, lat, lon, &in_date, &out_date).await?)
    }
}

pub async fn run() -> anyhow::Result<()> {
    let geo_addr  = std::env::var("GEO_ADDR").unwrap_or("http://localhost:8089".into());
    let rate_addr = std::env::var("RATE_ADDR").unwrap_or("http://localhost:8093".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or("0.0.0.0:8097".into()).parse()?;
    let wasm_file = std::env::var("WASM_FILE").unwrap_or("search.wasm".into());

    let geo_client  = GeoClient::connect(geo_addr).await?;
    let rate_client = RateClient::connect(rate_addr).await?;
    let geo_client  = Arc::new(Mutex::new(geo_client));
    let rate_client = Arc::new(Mutex::new(rate_client));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    SearchHostWorld::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(SearchHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    println!("search-host listening on {listen_addr}");
    crate::grpc::serve(Arc::new(SearchSvcHost { engine, pre, geo_client, rate_client }), listen_addr).await
}
