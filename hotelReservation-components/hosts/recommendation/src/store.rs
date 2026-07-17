use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("recommendation_store");
}
use proto::{
    recommendation_store_server::{RecommendationStore, RecommendationStoreServer},
    Hotel as ProtoHotel, InitRequest, InitResponse, LoadHotelsRequest, LoadHotelsResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/recommendation_store/wit",
    world: "recommendation-store-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(RecommendationStoreHostWorld);

#[tonic::async_trait]
impl RecommendationStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_store_recommendation_store()
            .call_init(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_hotels(
        &self,
        _req: Request<LoadHotelsRequest>,
    ) -> Result<Response<LoadHotelsResponse>, Status> {
        let mut store = self.store.lock().await;
        let wit_hotels = self.instance
            .hotel_store_recommendation_store()
            .call_load_hotels(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadHotelsResponse {
            hotels: wit_hotels.into_iter().map(|h| ProtoHotel {
                id: h.id, lat: h.lat, lon: h.lon, rate: h.rate, price: h.price,
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    RecommendationStoreHostWorld, RecommendationStoreServer,
    hotel_store_recommendation_store,
    "recommendation-db", "recommendation",
    "0.0.0.0:8086", "recommendation-store.wasm", "recommendation-host"
);
