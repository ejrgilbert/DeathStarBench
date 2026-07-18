use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::ResourceTable;
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
    async: true,
});

host_lib::svc_host_data!(AttractionsStoreClient<tonic::transport::Channel>);
host_lib::impl_cache_host!(HostData);

#[async_trait::async_trait]
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
}

struct AttractionsSvcHost {
    engine:       Arc<wasmtime::Engine>,
    pre:          Arc<AttractionsHostWorldPre<HostData>>,
    store_client: Arc<Mutex<AttractionsStoreClient<tonic::transport::Channel>>>,
    cache:        Arc<host_lib::Cache>,
}

#[async_trait::async_trait]
impl AttractionsComponent for AttractionsSvcHost {
    async fn nearby_rest(&self, hotel_id: String) -> Result<Vec<String>> {
        let (mut store, instance) = self.make_instance().await?;
        Ok(instance.hotel_api_attractions().call_nearby_rest(&mut store, &hotel_id).await?)
    }
    async fn nearby_mus(&self, hotel_id: String) -> Result<Vec<String>> {
        let (mut store, instance) = self.make_instance().await?;
        Ok(instance.hotel_api_attractions().call_nearby_mus(&mut store, &hotel_id).await?)
    }
    async fn nearby_cinema(&self, hotel_id: String) -> Result<Vec<String>> {
        let (mut store, instance) = self.make_instance().await?;
        Ok(instance.hotel_api_attractions().call_nearby_cinema(&mut store, &hotel_id).await?)
    }
}

impl AttractionsSvcHost {
    async fn make_instance(&self) -> Result<(Store<HostData>, AttractionsHostWorld)> {
        let data = HostData {
            wasi:         host_lib::make_wasi_ctx(),
            table:        ResourceTable::new(),
            store_client: self.store_client.clone(),
            cache:        self.cache.clone(),
        };
        let mut store = Store::new(&self.engine, data);
        let instance = self.pre.instantiate_async(&mut store).await?;
        Ok((store, instance))
    }
}

pub async fn run() -> Result<()> {
    let store_addr  = std::env::var("STORE_ADDR").unwrap_or("http://localhost:8088".into());
    let listen_addr: std::net::SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or("0.0.0.0:8087".into()).parse()?;
    let wasm_file = std::env::var("WASM_FILE").unwrap_or("attractions.wasm".into());

    let store_client = AttractionsStoreClient::connect(store_addr).await?;
    let store_client = Arc::new(Mutex::new(store_client));
    let cache        = Arc::new(host_lib::Cache::new(Default::default()));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<HostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    AttractionsHostWorld::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(AttractionsHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    println!("attractions-host [svc] listening on {listen_addr}");
    crate::grpc::serve(Arc::new(AttractionsSvcHost { engine, pre, store_client, cache }), listen_addr).await
}
