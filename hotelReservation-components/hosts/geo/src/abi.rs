use anyhow::Result;
use wasmtime::Store;
use host_lib::StoreData;
use crate::grpc::GeoComponent;

wasmtime::component::bindgen!({
    path: "../../components/geo/wit",
    world: "geo-composed-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);

#[async_trait::async_trait]
impl GeoComponent for GeoComposedHostWorld {
    type Data = StoreData;

    async fn nearby(
        &self,
        store: &mut Store<Self::Data>,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_geo_geo().call_nearby(store, lat, lon).await?)
    }
}

host_lib::run_abi!(
    GeoComposedHostWorld, hotel_geo_geo,
    "geo-db", "geo",
    "0.0.0.0:8089", "geo-composed.wasm", "geo-host"
);
