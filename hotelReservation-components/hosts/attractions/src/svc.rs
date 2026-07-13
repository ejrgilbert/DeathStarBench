use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
use host_lib::make_wasi_ctx;

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

pub struct HostData {
    wasi:         wasmtime_wasi::WasiCtx,
    table:        ResourceTable,
    store_client: Arc<Mutex<AttractionsStoreClient<tonic::transport::Channel>>>,
}

impl wasmtime_wasi::WasiView for HostData {
    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx { &mut self.wasi }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

#[async_trait::async_trait]
impl hotel::attractions_data::attractions_store::Host for HostData {
    async fn init(&mut self) {
        self.store_client
            .lock().await
            .init(tonic::Request::new(InitRequest {}))
            .await
            .expect("gRPC store Init failed");
    }

    async fn load_hotel_positions(
        &mut self,
    ) -> Vec<hotel::attractions_data::attractions_store::HotelPosition> {
        let resp = self.store_client.lock().await
            .load_hotel_positions(tonic::Request::new(LoadRequest {}))
            .await
            .expect("gRPC LoadHotelPositions failed")
            .into_inner();
        resp.hotels.into_iter().map(|h| hotel::attractions_data::attractions_store::HotelPosition {
            id: h.id, lat: h.lat, lon: h.lon,
        }).collect()
    }

    async fn load_restaurants(
        &mut self,
    ) -> Vec<hotel::attractions_data::attractions_store::Restaurant> {
        let resp = self.store_client.lock().await
            .load_restaurants(tonic::Request::new(LoadRequest {}))
            .await
            .expect("gRPC LoadRestaurants failed")
            .into_inner();
        resp.restaurants.into_iter().map(|r| hotel::attractions_data::attractions_store::Restaurant {
            id: r.id, lat: r.lat, lon: r.lon, name: r.name, rating: r.rating, category: r.category,
        }).collect()
    }

    async fn load_museums(
        &mut self,
    ) -> Vec<hotel::attractions_data::attractions_store::Museum> {
        let resp = self.store_client.lock().await
            .load_museums(tonic::Request::new(LoadRequest {}))
            .await
            .expect("gRPC LoadMuseums failed")
            .into_inner();
        resp.museums.into_iter().map(|m| hotel::attractions_data::attractions_store::Museum {
            id: m.id, lat: m.lat, lon: m.lon, name: m.name, category: m.category,
        }).collect()
    }

    async fn load_cinemas(
        &mut self,
    ) -> Vec<hotel::attractions_data::attractions_store::Cinema> {
        let resp = self.store_client.lock().await
            .load_cinemas(tonic::Request::new(LoadRequest {}))
            .await
            .expect("gRPC LoadCinemas failed")
            .into_inner();
        resp.cinemas.into_iter().map(|c| hotel::attractions_data::attractions_store::Cinema {
            id: c.id, lat: c.lat, lon: c.lon, name: c.name, category: c.category,
        }).collect()
    }
}

#[async_trait::async_trait]
impl AttractionsComponent for AttractionsHostWorld {
    type Data = HostData;

    async fn nearby_rest(
        &self,
        store: &mut wasmtime::Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_attractions_attractions()
            .call_nearby_rest(store, &hotel_id)
            .await?)
    }

    async fn nearby_mus(
        &self,
        store: &mut wasmtime::Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_attractions_attractions()
            .call_nearby_mus(store, &hotel_id)
            .await?)
    }

    async fn nearby_cinema(
        &self,
        store: &mut wasmtime::Store<Self::Data>,
        hotel_id: String,
    ) -> Result<Vec<String>> {
        Ok(self.hotel_attractions_attractions()
            .call_nearby_cinema(store, &hotel_id)
            .await?)
    }
}

pub async fn run() -> Result<()> {
    let store_addr = std::env::var("STORE_ADDR")
        .unwrap_or_else(|_| "http://localhost:8088".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8087".into())
        .parse()?;
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "attractions.wasm".into());

    let store_client = AttractionsStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    AttractionsHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = HostData {
        wasi: make_wasi_ctx(),
        table: ResourceTable::new(),
        store_client,
    };
    let mut store = wasmtime::Store::new(&engine, data);

    let component = wasmtime::component::Component::from_file(&engine, &wasm_file)?;
    let instance =
        AttractionsHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance.hotel_attractions_attractions()
        .call_init(&mut store)
        .await?;

    println!("attractions-host [svc] listening on {listen_addr}");

    crate::grpc::serve(
        Arc::new(Mutex::new(store)),
        Arc::new(instance),
        listen_addr,
    ).await
}
