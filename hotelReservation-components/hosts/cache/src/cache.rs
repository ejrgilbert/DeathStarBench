use std::sync::Arc;

use anyhow::Result;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView};

use host_lib::{Cache, InstancePool};

mod proto {
    tonic::include_proto!("cache");
}
use proto::cache_server::{Cache as CacheSvc, CacheServer};
use proto::{CacheEntry, GetMultiRequest, GetMultiResponse, GetRequest, GetResponse, SetRequest, SetResponse};

// Host world: run the cache component (exports `cache:keyvalue/keyvalue`) and
// satisfy its `host:cache/keyvalue` import in-process with the shared map below.
wasmtime::component::bindgen!({
    path: "../../components/cache/wit",
    world: "cache-host-world",
    imports: { default: async },
    exports: { default: async },
});

/// Per-host data: the actual cache map lives here (shared across the warm pool
/// via `Arc`), so a `set` served by one pooled instance is visible to a `get`
/// served by another. The cache component forwards to this via `host:cache`.
struct CacheHostData {
    wasi:  WasiCtx,
    table: ResourceTable,
    cache: Arc<Cache>,
}

impl wasmtime_wasi::WasiView for CacheHostData {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

// Implements `host::cache::keyvalue::Host` against `self.cache` (the map).
host_lib::impl_cache_host!(CacheHostData);

struct CacheGrpcService {
    pool: InstancePool<CacheHostData, CacheHostWorld>,
}

#[tonic::async_trait]
impl CacheSvc for CacheGrpcService {
    async fn get(&self, req: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
        let key = req.into_inner().key;
        let mut checked = self.pool.checkout().await;
        let (store, instance) = checked.parts();
        let val = instance
            .cache_keyvalue_keyvalue()
            .call_get(&mut *store, &key)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(match val {
            Some(v) => GetResponse { found: true, value: v },
            None    => GetResponse { found: false, value: Vec::new() },
        }))
    }

    async fn get_multi(
        &self,
        req: Request<GetMultiRequest>,
    ) -> Result<Response<GetMultiResponse>, Status> {
        let keys = req.into_inner().keys;
        let mut checked = self.pool.checkout().await;
        let (store, instance) = checked.parts();
        let vals = instance
            .cache_keyvalue_keyvalue()
            .call_get_multi(&mut *store, &keys)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        let entries = vals
            .into_iter()
            .map(|v| match v {
                Some(value) => CacheEntry { found: true, value },
                None        => CacheEntry { found: false, value: Vec::new() },
            })
            .collect();
        Ok(Response::new(GetMultiResponse { entries }))
    }

    async fn set(&self, req: Request<SetRequest>) -> Result<Response<SetResponse>, Status> {
        let SetRequest { key, value } = req.into_inner();
        let mut checked = self.pool.checkout().await;
        let (store, instance) = checked.parts();
        instance
            .cache_keyvalue_keyvalue()
            .call_set(&mut *store, &key, &value)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        checked.commit();
        Ok(Response::new(SetResponse {}))
    }
}

pub async fn run() -> Result<()> {
    fn env_or(k: &str, d: &str) -> String {
        std::env::var(k).unwrap_or_else(|_| d.to_string())
    }

    let listen_addr = env_or("LISTEN_ADDR", "0.0.0.0:8110");
    let wasm_file   = env_or("WASM_FILE",   "cache.wasm");
    let pool_size: usize = env_or("POOL_SIZE", "64").parse().unwrap_or(64);

    // The map is shared across every pooled instance.
    let cache = Arc::new(Cache::new(std::collections::HashMap::new()));

    let engine = Arc::new(host_lib::make_engine()?);
    let mut linker: Linker<CacheHostData> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    CacheHostWorld::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = Arc::new(CacheHostWorldPre::new(linker.instantiate_pre(&component)?)?);

    let pool = InstancePool::build(pool_size, move || {
        let engine = engine.clone();
        let pre    = pre.clone();
        let cache  = cache.clone();
        async move {
            let data = CacheHostData {
                wasi:  host_lib::make_wasi_ctx(),
                table: ResourceTable::new(),
                cache,
            };
            let mut store = Store::new(&engine, data);
            let instance = pre.instantiate_async(&mut store).await?;
            anyhow::Ok((store, instance))
        }
    })
    .await?;

    let svc = CacheGrpcService { pool };

    println!("cache-host listening on {listen_addr} (instance pool size {pool_size})");

    Server::builder()
        .add_service(CacheServer::new(svc))
        .serve(listen_addr.parse()?)
        .await?;
    Ok(())
}
