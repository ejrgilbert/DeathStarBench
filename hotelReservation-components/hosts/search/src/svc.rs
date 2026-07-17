use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::Store;
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
    async: true,
});

pub struct HostData {
    pub wasi:        wasmtime_wasi::WasiCtx,
    pub table:       wasmtime_wasi::ResourceTable,
    pub geo_client:  Arc<Mutex<GeoClient<tonic::transport::Channel>>>,
    pub rate_client: Arc<Mutex<RateClient<tonic::transport::Channel>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self)   -> &mut wasmtime_wasi::WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
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

#[async_trait::async_trait]
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

#[async_trait::async_trait]
impl SearchComponent for SearchHostWorld {
    type Data = HostData;

    async fn nearby(
        &self,
        store: &mut Store<HostData>,
        lat: f64,
        lon: f64,
        in_date: String,
        out_date: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_search()
            .call_nearby(store, lat, lon, &in_date, &out_date).await?)
    }
}

pub async fn run() -> anyhow::Result<()> {
    use wasmtime::component::{Component, Linker};

    let geo_addr  = std::env::var("GEO_ADDR")
        .unwrap_or_else(|_| "http://localhost:8089".into());
    let rate_addr = std::env::var("RATE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8093".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8097".into())
        .parse()?;
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "search.wasm".into());

    let geo_client  = GeoClient::connect(geo_addr).await?;
    let rate_client = RateClient::connect(rate_addr).await?;

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    SearchHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = HostData {
        wasi:        host_lib::make_wasi_ctx(),
        table:       wasmtime_wasi::ResourceTable::new(),
        geo_client:  Arc::new(Mutex::new(geo_client)),
        rate_client: Arc::new(Mutex::new(rate_client)),
    };
    let mut store = wasmtime::Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance  = SearchHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    println!("search-host listening on {listen_addr}");

    crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
}
