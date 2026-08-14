use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("geo_store");
}
use proto::{
    geo_store_server::{GeoStore, GeoStoreServer},
    Point as ProtoPoint,
    LoadRequest, LoadGeoResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/geo_store/wit",
    world: "geo-store-host-world",
    imports: { default: async },
    exports: { default: async },
    with: {
        "host:storage/collection.connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(GeoStoreHostWorld, GeoStoreHostWorldPre<host_lib::StoreData>);

#[tonic::async_trait]
impl GeoStore for StoreGrpcService {
    async fn load_geo(
        &self,
        _req: Request<LoadRequest>,
    ) -> Result<Response<LoadGeoResponse>, Status> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let items = instance
            .hotel_store_geo_store()
            .call_load_geo(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(LoadGeoResponse {
            geo: items.into_iter().map(|p| ProtoPoint { id: p.id, lat: p.lat, lon: p.lon }).collect(),
        }))
    }
}

host_lib::run_store!(
    GeoStoreHostWorld, GeoStoreHostWorldPre<host_lib::StoreData>, GeoStoreServer,
    "geo-db",
    "0.0.0.0:8090", "geo-store.wasm", "geo-host"
);
