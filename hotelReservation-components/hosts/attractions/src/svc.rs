use anyhow::Result;
use crate::grpc::AttractionsComponent;

mod store_proto {
    tonic::include_proto!("attractions_store");
}
use store_proto::{
    attractions_store_client::AttractionsStoreClient,
    LoadRequest,
};

wasmtime::component::bindgen!({
    path: "../../components/attractions/wit",
    world: "attractions-host-world",
    imports: { default: async },
    exports: { default: async },
});

host_lib::svc_host_data!(AttractionsStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

impl hotel::store::attractions_store::Host for HostData {
    async fn load_hotel_positions(&mut self) -> Vec<hotel::store::attractions_store::HotelPosition> {
        let resp = self.store_client.lock().await
            .load_hotel_positions(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC LoadHotelPositions failed").into_inner();
        resp.hotels.into_iter().map(|h| hotel::store::attractions_store::HotelPosition {
            id: h.id, lat: h.lat, lon: h.lon,
        }).collect()
    }

    async fn load_restaurants(&mut self) -> Vec<hotel::store::attractions_store::Restaurant> {
        let resp = self.store_client.lock().await
            .load_restaurants(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC LoadRestaurants failed").into_inner();
        resp.restaurants.into_iter().map(|r| hotel::store::attractions_store::Restaurant {
            id: r.id, lat: r.lat, lon: r.lon, name: r.name, rating: r.rating, category: r.category,
        }).collect()
    }

    async fn load_museums(&mut self) -> Vec<hotel::store::attractions_store::Museum> {
        let resp = self.store_client.lock().await
            .load_museums(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC LoadMuseums failed").into_inner();
        resp.museums.into_iter().map(|m| hotel::store::attractions_store::Museum {
            id: m.id, lat: m.lat, lon: m.lon, name: m.name, category: m.category,
        }).collect()
    }

    async fn load_cinemas(&mut self) -> Vec<hotel::store::attractions_store::Cinema> {
        let resp = self.store_client.lock().await
            .load_cinemas(tonic::Request::new(LoadRequest {})).await
            .expect("gRPC LoadCinemas failed").into_inner();
        resp.cinemas.into_iter().map(|c| hotel::store::attractions_store::Cinema {
            id: c.id, lat: c.lat, lon: c.lon, name: c.name, category: c.category,
        }).collect()
    }

    // Targeted parity method: the store service is a plain gRPC load-all, so fall
    // back to filtering the loaded hotel positions by id.
    async fn get_hotel_position(&mut self, hotel_id: String) -> Option<hotel::store::attractions_store::HotelPosition> {
        self.load_hotel_positions().await.into_iter().find(|h| h.id == hotel_id)
    }
}

type AttractionsSvcHost = host_lib::SvcHost<AttractionsHostWorldPre<HostData>, AttractionsStoreClient<tonic::transport::Channel>>;

#[async_trait::async_trait]
impl AttractionsComponent for AttractionsSvcHost {
    async fn nearby_rest(&self, hotel_id: String) -> Result<Vec<String>> {
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_attractions().call_nearby_rest(&mut store, &hotel_id).await?)
    }
    async fn nearby_mus(&self, hotel_id: String) -> Result<Vec<String>> {
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_attractions().call_nearby_mus(&mut store, &hotel_id).await?)
    }
    async fn nearby_cinema(&self, hotel_id: String) -> Result<Vec<String>> {
        let (mut store, pre) = self.make_store();
        let instance = pre.instantiate_async(&mut store).await?;
        Ok(instance.hotel_api_attractions().call_nearby_cinema(&mut store, &hotel_id).await?)
    }
}

host_lib::run_svc_pre!(
    AttractionsHostWorld, AttractionsHostWorldPre<HostData>, AttractionsStoreClient<tonic::transport::Channel>,
    "http://localhost:8088", "0.0.0.0:8087", "attractions.wasm", "attractions-host [svc]"
);
