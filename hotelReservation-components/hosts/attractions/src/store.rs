use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use host_lib::{make_engine, make_store_wasi_ctx, StoreData};

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
});

host_lib::impl_collection_host!(StoreData);

type SharedStore    = Arc<Mutex<Store<StoreData>>>;
type SharedInstance = Arc<AttractionsStoreHostWorld>;

struct StoreGrpcService {
    store:    SharedStore,
    instance: SharedInstance,
}

#[tonic::async_trait]
impl AttractionsStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_attractions_data_attractions_store()
            .call_init(&mut *store)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_hotel_positions(
        &self,
        _req: Request<LoadRequest>,
    ) -> Result<Response<LoadHotelPositionsResponse>, Status> {
        let mut store = self.store.lock().await;
        let items = self.instance
            .hotel_attractions_data_attractions_store()
            .call_load_hotel_positions(&mut *store)
            .await
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
            .hotel_attractions_data_attractions_store()
            .call_load_restaurants(&mut *store)
            .await
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
            .hotel_attractions_data_attractions_store()
            .call_load_museums(&mut *store)
            .await
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
            .hotel_attractions_data_attractions_store()
            .call_load_cinemas(&mut *store)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadCinemasResponse {
            cinemas: items.into_iter().map(|c| ProtoCinema {
                id: c.id, lat: c.lat, lon: c.lon, name: c.name, category: c.category,
            }).collect(),
        }))
    }
}

pub async fn run() -> Result<()> {
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8088".into());
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "attractions-store.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(
        mongo.database("attractions-db").collection("attractions"),
    );

    let engine = make_engine()?;
    let mut linker: Linker<StoreData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    AttractionsStoreHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = StoreData {
        wasi: make_store_wasi_ctx(&data_dir)?,
        table: wasmtime_wasi::ResourceTable::new(),
        collection,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance =
        AttractionsStoreHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance
        .hotel_attractions_data_attractions_store()
        .call_init(&mut store)
        .await?;

    let store    = Arc::new(Mutex::new(store));
    let instance = Arc::new(instance);

    println!("attractions-host [store] listening on {listen_addr}");

    Server::builder()
        .add_service(AttractionsStoreServer::new(StoreGrpcService { store, instance }))
        .serve(listen_addr.parse()?)
        .await?;

    Ok(())
}
