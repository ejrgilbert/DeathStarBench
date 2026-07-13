use std::sync::Arc;
use anyhow::Result;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use host_lib::{make_engine, make_store_wasi_ctx, StoreData};

mod proto {
    tonic::include_proto!("geo_store");
}
use proto::{
    geo_store_server::{GeoStore, GeoStoreServer},
    Point as ProtoPoint,
    InitRequest, InitResponse, LoadRequest, LoadGeoResponse,
};

wasmtime::component::bindgen!({
    path: "../../components/geo_store/wit",
    world: "geo-store-host-world",
    async: true,
});

host_lib::impl_collection_host!(StoreData);

type SharedStore    = Arc<Mutex<Store<StoreData>>>;
type SharedInstance = Arc<GeoStoreHostWorld>;

struct StoreGrpcService {
    store:    SharedStore,
    instance: SharedInstance,
}

#[tonic::async_trait]
impl GeoStore for StoreGrpcService {
    async fn init(&self, _req: Request<InitRequest>) -> Result<Response<InitResponse>, Status> {
        let mut store = self.store.lock().await;
        self.instance
            .hotel_geo_data_geo_store()
            .call_init(&mut *store)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(InitResponse {}))
    }

    async fn load_geo(
        &self,
        _req: Request<LoadRequest>,
    ) -> Result<Response<LoadGeoResponse>, Status> {
        let mut store = self.store.lock().await;
        let items = self.instance
            .hotel_geo_data_geo_store()
            .call_load_geo(&mut *store)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(LoadGeoResponse {
            geo: items.into_iter().map(|p| ProtoPoint { id: p.id, lat: p.lat, lon: p.lon }).collect(),
        }))
    }
}

pub async fn run() -> Result<()> {
    let mongo_uri = std::env::var("MONGO_URI")
        .unwrap_or_else(|_| "mongodb://localhost:27017".into());
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8090".into());
    let data_dir = std::env::var("DATA_DIR")
        .unwrap_or_else(|_| "/data".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "geo-store.wasm".into());

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let collection = Arc::new(
        mongo.database("geo-db").collection("geo"),
    );

    let engine = make_engine()?;
    let mut linker: Linker<StoreData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    GeoStoreHostWorld::add_to_linker(&mut linker, |d| d)?;

    let data = StoreData {
        wasi: make_store_wasi_ctx(&data_dir)?,
        table: wasmtime_wasi::ResourceTable::new(),
        collection,
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance =
        GeoStoreHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    instance
        .hotel_geo_data_geo_store()
        .call_init(&mut store)
        .await?;

    let store    = Arc::new(Mutex::new(store));
    let instance = Arc::new(instance);

    println!("geo-host [store] listening on {listen_addr}");

    Server::builder()
        .add_service(GeoStoreServer::new(StoreGrpcService { store, instance }))
        .serve(listen_addr.parse()?)
        .await?;

    Ok(())
}
