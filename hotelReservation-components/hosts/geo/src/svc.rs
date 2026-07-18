use anyhow::Result;
use crate::grpc::GeoComponent;

mod store_proto {
    tonic::include_proto!("geo_store");
}
use store_proto::{geo_store_client::GeoStoreClient, LoadRequest};

wasmtime::component::bindgen!({
    path: "../../components/geo/wit",
    world: "geo-host-world",
    async: true,
});

host_lib::svc_host_data!(GeoStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
impl hotel::store::geo_store::Host for HostData {
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

type GeoSvcHost = host_lib::SvcHost<GeoHostWorldPre<HostData>, GeoStoreClient<tonic::transport::Channel>>;

#[async_trait::async_trait]
impl GeoComponent for GeoSvcHost {
    async fn nearby(&self, lat: f64, lon: f64) -> Result<Vec<String>> {
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_geo().call_nearby(&mut store, lat, lon).await?)
    }
}

host_lib::run_svc_pre!(
    GeoHostWorld, GeoHostWorldPre<HostData>, GeoStoreClient<tonic::transport::Channel>,
    "http://localhost:8090", "0.0.0.0:8089", "geo.wasm", "geo-host [svc]"
);
