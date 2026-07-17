use anyhow::Result;
use wasmtime::Store;
use host_lib::StoreData;
use crate::grpc::GeoComponent;

wasmtime::component::bindgen!({
    path: "../../components/geo/wit",
    world: "geo-composed-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::impl_cache_host!(StoreData);

#[async_trait::async_trait]
impl GeoComponent for GeoComposedHostWorld {
    type Data = StoreData;

    async fn nearby(
        &self,
        store: &mut Store<Self::Data>,
        lat: f64,
        lon: f64,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_geo().call_nearby(store, lat, lon).await?)
    }
}

host_lib::run_abi!(
    GeoComposedHostWorld, hotel_api_geo,
    "geo-db",
    "0.0.0.0:8089", "geo-composed.wasm", "geo-host"
);
