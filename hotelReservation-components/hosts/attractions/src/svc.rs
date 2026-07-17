use anyhow::Result;
use wasmtime::Store;
use crate::grpc::AttractionsComponent;

mod store_proto {
    tonic::include_proto!("attractions_store");
}
use store_proto::{
    attractions_store_client::AttractionsStoreClient,
    InitRequest, LoadRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/attractions/wit",
    world: "attractions-host-world",
    async: true,
});

host_lib::svc_host_data!(AttractionsStoreClient<tonic::transport::Channel>);

#[async_trait::async_trait]
impl hotel::store::attractions_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client.lock().await
            .init(tonic::Request::new(InitRequest {})).await
            .expect("gRPC store Init failed");
    }

    async fn load_hotel_positions(
        &mut self,
    ) -> Vec<hotel::store::attractions_store::HotelPosition> {
        let resp = self.store_client.lock().await
            .load_hotel_positions(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC LoadHotelPositions failed")
            .into_inner();
        resp.hotels.into_iter().map(|h| hotel::store::attractions_store::HotelPosition {
            id: h.id, lat: h.lat, lon: h.lon,
        }).collect()
    }

    async fn load_restaurants(
        &mut self,
    ) -> Vec<hotel::store::attractions_store::Restaurant> {
        let resp = self.store_client.lock().await
            .load_restaurants(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC LoadRestaurants failed")
            .into_inner();
        resp.restaurants.into_iter().map(|r| hotel::store::attractions_store::Restaurant {
            id: r.id, lat: r.lat, lon: r.lon, name: r.name, rating: r.rating, category: r.category,
        }).collect()
    }

    async fn load_museums(
        &mut self,
    ) -> Vec<hotel::store::attractions_store::Museum> {
        let resp = self.store_client.lock().await
            .load_museums(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC LoadMuseums failed")
            .into_inner();
        resp.museums.into_iter().map(|m| hotel::store::attractions_store::Museum {
            id: m.id, lat: m.lat, lon: m.lon, name: m.name, category: m.category,
        }).collect()
    }

    async fn load_cinemas(
        &mut self,
    ) -> Vec<hotel::store::attractions_store::Cinema> {
        let resp = self.store_client.lock().await
            .load_cinemas(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC LoadCinemas failed")
            .into_inner();
        resp.cinemas.into_iter().map(|c| hotel::store::attractions_store::Cinema {
            id: c.id, lat: c.lat, lon: c.lon, name: c.name, category: c.category,
        }).collect()
    }
}

#[async_trait::async_trait]
impl AttractionsComponent for AttractionsHostWorld {
    type Data = HostData;

    async fn nearby_rest(
        &self,
        store: &mut Store<HostData>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_attractions()
            .call_nearby_rest(store, &hotel_id).await?)
    }

    async fn nearby_mus(
        &self,
        store: &mut Store<HostData>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_attractions()
            .call_nearby_mus(store, &hotel_id).await?)
    }

    async fn nearby_cinema(
        &self,
        store: &mut Store<HostData>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_api_attractions()
            .call_nearby_cinema(store, &hotel_id).await?)
    }
}

host_lib::run_svc!(
    AttractionsHostWorld,
    AttractionsStoreClient<tonic::transport::Channel>,
    hotel_api_attractions,
    "http://localhost:8088",
    "0.0.0.0:8087",
    "attractions.wasm",
    "attractions-host"
);
