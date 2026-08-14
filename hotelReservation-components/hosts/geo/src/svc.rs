use anyhow::Result;
use crate::grpc::GeoComponent;

mod store_proto {
    tonic::include_proto!("geo_store");
}
use store_proto::{geo_store_client::GeoStoreClient, LoadRequest};

wasmtime::component::bindgen!({
    path: "../../components/geo/wit",
    world: "geo-host-world",
    imports: { default: async },
    exports: { default: async },
});

host_lib::svc_host_data!(GeoStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

impl hotel::store::geo_store::Host for HostData {
    async fn load_geo(&mut self) -> Vec<hotel::store::geo_store::Point> {
        let resp = self.store_client.clone()
            .load_geo(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC geo-store LoadGeo failed")
            .into_inner();
        resp.geo.into_iter().map(|p| hotel::store::geo_store::Point {
            id: p.id, lat: p.lat, lon: p.lon,
        }).collect()
    }
}

type GeoSvcHost = host_lib::PooledSvcHost<GeoStoreClient<tonic::transport::Channel>, GeoHostWorld>;

#[async_trait::async_trait]
impl GeoComponent for GeoSvcHost {
    async fn nearby(&self, lat: f64, lon: f64) -> Result<Vec<String>> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let out = instance.hotel_api_geo().call_nearby(&mut *store, lat, lon).await?;
        checked.commit();
        Ok(out)
    }
}

host_lib::run_svc_pre!(
    GeoHostWorld, GeoHostWorldPre<HostData>, GeoStoreClient<tonic::transport::Channel>,
    "http://localhost:8090", "0.0.0.0:8089", "geo.wasm", "geo-host [svc]"
);
