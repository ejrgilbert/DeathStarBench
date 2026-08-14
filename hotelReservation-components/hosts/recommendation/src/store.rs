use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("recommendation_store");
}
use proto::{
    recommendation_store_server::{RecommendationStore, RecommendationStoreServer},
    Hotel as ProtoHotel, LoadHotelsRequest, LoadHotelsResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/recommendation_store/wit",
    world: "recommendation-store-host-world",
    imports: { default: async },
    exports: { default: async },
    with: {
        "host:storage/collection.connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(RecommendationStoreHostWorld, RecommendationStoreHostWorldPre<host_lib::StoreData>);

#[tonic::async_trait]
impl RecommendationStore for StoreGrpcService {
    async fn load_hotels(
        &self,
        _req: Request<LoadHotelsRequest>,
    ) -> Result<Response<LoadHotelsResponse>, Status> {
        let mut checked = self.checkout().await;
        let (store, instance) = checked.parts();
        let wit_hotels = instance
            .hotel_store_recommendation_store()
            .call_load_hotels(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(LoadHotelsResponse {
            hotels: wit_hotels.into_iter().map(|h| ProtoHotel {
                id: h.id, lat: h.lat, lon: h.lon, rate: h.rate, price: h.price,
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    RecommendationStoreHostWorld, RecommendationStoreHostWorldPre<host_lib::StoreData>, RecommendationStoreServer,
    "recommendation-db",
    "0.0.0.0:8086", "recommendation-store.wasm", "recommendation-host"
);
