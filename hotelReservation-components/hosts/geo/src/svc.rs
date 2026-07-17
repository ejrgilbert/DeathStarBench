use anyhow::Result;
use wasmtime::Store;
use crate::grpc::GeoComponent;

mod store_proto {
    tonic::include_proto!("geo_store");
}
use store_proto::{geo_store_client::GeoStoreClient, InitRequest, LoadRequest};

wasmtime::component::bindgen!({
    path: "../../components/geo/wit",
    world: "geo-host-world",
    async: true,
});

host_lib::svc_host_data!(GeoStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
impl hotel::store::geo_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client.lock().await
            .init(tonic::Request::new(InitRequest {})).await
            .expect("gRPC geo-store Init failed");
    }

    async fn load_geo(&mut self) -> Vec<hotel::store::geo_store::Point> {
        let resp = self.store_client.lock().await
            .load_geo(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC geo-store LoadGeo failed")
            .into_inner();
        resp.geo.into_iter().map(|p| hotel::store::geo_store::Point {
            id: p.id, lat: p.lat, lon: p.lon,
        }).collect()
    }
}

#[async_trait::async_trait]
impl GeoComponent for GeoHostWorld {
    type Data = HostData;

    async fn nearby(
        &self,
        store: &mut Store<HostData>,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_geo().call_nearby(store, lat, lon).await?)
    }
}

host_lib::run_svc!(
    GeoHostWorld,
    GeoStoreClient<tonic::transport::Channel>,
    hotel_api_geo,
    "http://localhost:8090",
    "0.0.0.0:8089",
    "geo.wasm",
    "geo-host"
);
