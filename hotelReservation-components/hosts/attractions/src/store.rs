use anyhow::Result;
use tonic::{Request, Response, Status};
use host_lib::StoreData;

mod proto {
    tonic::include_proto!("attractions_store");
}
use proto::{
    attractions_store_server::{AttractionsStore, AttractionsStoreServer},
    HotelPosition as ProtoHotel, Restaurant as ProtoRestaurant,
    Museum as ProtoMuseum, Cinema as ProtoCinema,
    InitRequest, InitResponse, LoadRequest,
    LoadHotelPositionsResponse, LoadRestaurantsResponse,
    LoadMuseumsResponse, LoadCinemasResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/attractions_store/wit",
    world: "attractions-store-host-world",
    async: true,
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
    },
});

host_lib::impl_collection_host!(StoreData);
host_lib::define_store_service!(AttractionsStoreHostWorld);

#[tonic::async_trait]
impl AttractionsStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_store_attractions_store()
            .call_init(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_hotel_positions(
        &self,
        _req: Request<LoadRequest>,
    ) -> Result<Response<LoadHotelPositionsResponse>, Status> {
        let mut store = self.store.lock().await;
        let items = self.instance
            .hotel_store_attractions_store()
            .call_load_hotel_positions(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadHotelPositionsResponse {
            hotels: items.into_iter().map(|h| ProtoHotel { id: h.id, lat: h.lat, lon: h.lon }).collect(),
        }))
    }

    async fn load_restaurants(
        &self,
        _req: Request<LoadRequest>,
    ) -> Result<Response<LoadRestaurantsResponse>, Status> {
        let mut store = self.store.lock().await;
        let items = self.instance
            .hotel_store_attractions_store()
            .call_load_restaurants(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadRestaurantsResponse {
            restaurants: items.into_iter().map(|r| ProtoRestaurant {
                id: r.id, lat: r.lat, lon: r.lon, name: r.name, rating: r.rating, category: r.category,
            }).collect(),
        }))
    }

    async fn load_museums(
        &self,
        _req: Request<LoadRequest>,
    ) -> Result<Response<LoadMuseumsResponse>, Status> {
        let mut store = self.store.lock().await;
        let items = self.instance
            .hotel_store_attractions_store()
            .call_load_museums(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadMuseumsResponse {
            museums: items.into_iter().map(|m| ProtoMuseum {
                id: m.id, lat: m.lat, lon: m.lon, name: m.name, category: m.category,
            }).collect(),
        }))
    }

    async fn load_cinemas(
        &self,
        _req: Request<LoadRequest>,
    ) -> Result<Response<LoadCinemasResponse>, Status> {
        let mut store = self.store.lock().await;
        let items = self.instance
            .hotel_store_attractions_store()
            .call_load_cinemas(&mut *store).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadCinemasResponse {
            cinemas: items.into_iter().map(|c| ProtoCinema {
                id: c.id, lat: c.lat, lon: c.lon, name: c.name, category: c.category,
            }).collect(),
        }))
    }
}

host_lib::run_store!(
    AttractionsStoreHostWorld, AttractionsStoreServer,
    hotel_store_attractions_store,
    "attractions-db",
    "0.0.0.0:8088", "attractions-store.wasm", "attractions-host"
);
