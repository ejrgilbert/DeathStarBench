use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use wasmtime::component::{Component, Linker};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView};

mod proto {
    tonic::include_proto!("cache");
}
use proto::{
    cache_server::{Cache, CacheServer},
    GetRequest, GetResponse, SetRequest, SetResponse,
};

wasmtime::component::bindgen!({
    path: "wit",
    world: "cache-host-world",
    async: true,
});

struct CacheData {
    wasi:  WasiCtx,
    table: ResourceTable,
}

impl WasiView for CacheData {
    fn ctx(&mut self)   -> &mut WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

type SharedStore    = Arc<Mutex<Store<CacheData>>>;
type SharedInstance = Arc<CacheHostWorld>;

struct CacheService {
    store:    SharedStore,
    instance: SharedInstance,
}

#[tonic::async_trait]
impl Cache for CacheService {
    async fn get(&self, req: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
        let key = req.into_inner().key;
        let mut store = self.store.lock().await;
        let result = self.instance
            .cache_keyvalue_keyvalue()
            .call_get(&mut *store, &key).await
            .map_err(|e| Status::internal(e.to_string()))?;
        match result {
            Some(bytes) => Ok(Response::new(GetResponse { value: bytes, found: true })),
            None        => Ok(Response::new(GetResponse { value: vec![], found: false })),
        }
    }

    async fn set(&self, req: Request<SetRequest>) -> Result<Response<SetResponse>, Status> {
        let r = req.into_inner();
        let mut store = self.store.lock().await;
        self.instance
            .cache_keyvalue_keyvalue()
            .call_set(&mut *store, &r.key, &r.value).await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(SetResponse {}))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listen_addr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8102".into());
    let wasm_file = std::env::var("WASM_FILE")
        .unwrap_or_else(|_| "cache.wasm".into());

    let mut config = wasmtime::Config::new();
    config.async_support(true);
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;

    let mut linker: Linker<CacheData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;

    let data = CacheData {
        wasi:  WasiCtxBuilder::new().inherit_stderr().build(),
        table: ResourceTable::new(),
    };
    let mut store = Store::new(&engine, data);

    let component = Component::from_file(&engine, &wasm_file)?;
    let instance  = CacheHostWorld::instantiate_async(&mut store, &component, &linker).await?;

    let store    = Arc::new(Mutex::new(store));
    let instance = Arc::new(instance);

    println!("cache-host listening on {listen_addr}");

    Server::builder()
        .add_service(CacheServer::new(CacheService { store, instance }))
        .serve(listen_addr.parse()?)
        .await?;
    Ok(())
}
