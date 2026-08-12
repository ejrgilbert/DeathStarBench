use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use wasmtime::{Config, Engine, InstanceAllocationStrategy, PoolingAllocationConfig, Store};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView};
pub type Cache = std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>;

/// A fixed-size pool of pre-instantiated `(Store<T>, I)` pairs to reuse warm instances on requests
pub struct InstancePool<T: Send + 'static, I: Send + 'static> {
    tx: mpsc::Sender<(Store<T>, I)>,
    rx: Mutex<mpsc::Receiver<(Store<T>, I)>>,
}

impl<T: Send + 'static, I: Send + 'static> InstancePool<T, I> {
    /// Instantiate `size` warm instances up front via `make`.
    pub async fn build<F, Fut>(size: usize, make: F) -> Result<Self>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(Store<T>, I)>>,
    {
        let size = size.max(1);
        let (tx, rx) = mpsc::channel(size);
        for _ in 0..size {
            tx.try_send(make().await?)
                .map_err(|_| anyhow::anyhow!("instance pool prefill overflow"))?;
        }
        Ok(Self { tx, rx: Mutex::new(rx) })
    }

    /// Borrow a warm instance, awaiting until one is free. It is returned to the
    /// pool automatically when the returned guard drops.
    pub async fn checkout(&self) -> Checked<'_, T, I> {
        let item = self.rx.lock().await.recv().await.expect("instance pool closed");
        Checked { tx: &self.tx, item: Some(item) }
    }
}

/// RAII handle to a checked-out `(Store<T>, I)`; returns it to the pool on drop.
pub struct Checked<'a, T: Send + 'static, I: Send + 'static> {
    tx: &'a mpsc::Sender<(Store<T>, I)>,
    item: Option<(Store<T>, I)>,
}

impl<T: Send + 'static, I: Send + 'static> Checked<'_, T, I> {
    /// `(&mut Store, &Instance)` for making the guest call.
    pub fn parts(&mut self) -> (&mut Store<T>, &I) {
        let (s, i) = self.item.as_mut().expect("checked-out instance");
        (s, i)
    }
}

impl<T: Send + 'static, I: Send + 'static> Drop for Checked<'_, T, I> {
    fn drop(&mut self) {
        if let Some(item) = self.item.take() {
            // Capacity was reserved when we checked out, so this never blocks.
            let _ = self.tx.try_send(item);
        }
    }
}

/// Build the shared wasmtime engine used by every host mode (abi/tcp/store/svc).
///
/// All hosts follow the wasi:http model: a *fresh* component instance is built
/// for each incoming request (see `handle_request` / `new_instance`). With the
/// default on-demand allocator that means an `mmap` + memory-init of every
/// linear memory on the hot path of every request.
///
/// For the ABI host this is catastrophic: composition is eager, so
/// instantiating `frontend-all.wasm` instantiates *all* ~19 nested components
/// (~322 core instances, ~18 linear memories, ~54 tables) on every request, in
/// one process. That fixed per-request cost — not transport — is what made the
/// composed ("no IPC") path measure slower than the TCP/IPC path.
///
/// The pooling allocator pre-reserves instance/memory/table/stack slots at
/// startup and reuses them across requests. Freed slots are kept "warm" so the
/// backing pages stay mapped, making re-instantiation ~O(touched pages) instead
/// of O(app size). This moves instantiation off the critical path.
///
/// Limits are sized for the largest artifact in this benchmark
/// (`frontend-all.wasm`) at `CONCURRENCY` in-flight requests. The single-
/// component hosts (tcp/store/svc) reuse the same generous config unchanged.
const MIB: usize = 1 << 20;
const KIB: u64 = 1 << 10;

// ---- Pooling allocator defaults --------------------------------------------
// All per-instance figures are for the largest artifact here, the composed
// `frontend-all.wasm`, as reported by `wasm-tools print`. They exceed wasmtime's
// default per-component caps (20 each), so the pool must be widened explicitly.
// Each is an env-override *default*, not a fixed limit; single-component hosts
// need far less and reuse the same config unchanged.

/// Simultaneous in-flight requests to provision slots for. wrk drives 50
/// connections; this leaves headroom for keep-alive overlap.
const DEFAULT_CONCURRENCY: u32 = 128;
/// Core instances per composed instance (measured ~322).
const DEFAULT_CORES_PER_COMPONENT: u32 = 384;
/// Linear memories per composed instance (measured ~18).
const DEFAULT_MEMS_PER_COMPONENT: u32 = 32;
/// Tables per composed instance (measured ~54).
const DEFAULT_TABLES_PER_COMPONENT: u32 = 64;
/// Cap per linear memory. Services keep small heaps; this bounds the pool's
/// virtual reservation (= total_memories * this).
const DEFAULT_MAX_MEMORY_MIB: u32 = 128;
/// Component VMContext size cap. wasmtime's 1 MiB default is far too small for
/// the composed component.
const MAX_COMPONENT_INSTANCE_SIZE: usize = 32 * MIB;
/// Per-memory-slot guard region. Shrunk from wasmtime's 2 GiB default so the
/// pool's (unbacked) virtual reservation of `max_memory_size + guard` per slot
/// stays sane across all slots.
const MEMORY_GUARD_SIZE: u64 = 64 * KIB;

pub fn make_engine() -> Result<Engine> {
    // Read a `u32` pooling knob from the environment, else use the default.
    fn knob(key: &str, default: u32) -> u32 {
        std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
    }

    let concurrency = knob("POOL_CONCURRENCY", DEFAULT_CONCURRENCY);
    let cores_per   = knob("POOL_CORES_PER_COMPONENT", DEFAULT_CORES_PER_COMPONENT);
    let mems_per    = knob("POOL_MEMS_PER_COMPONENT", DEFAULT_MEMS_PER_COMPONENT);
    let tables_per  = knob("POOL_TABLES_PER_COMPONENT", DEFAULT_TABLES_PER_COMPONENT);
    let max_mem_mib = knob("POOL_MAX_MEMORY_MIB", DEFAULT_MAX_MEMORY_MIB);

    // Totals are `per_component * concurrency`: enough slots for that many
    // in-flight instances of the composed component at once.
    let mut pool = PoolingAllocationConfig::default();
    pool.max_core_instances_per_component(cores_per);
    pool.max_memories_per_component(mems_per);
    pool.max_tables_per_component(tables_per);
    pool.total_component_instances(concurrency);
    pool.total_core_instances(concurrency * cores_per);
    pool.total_memories(concurrency * mems_per);
    pool.total_tables(concurrency * tables_per);
    pool.total_stacks(concurrency); // async: one fiber stack per in-flight instance
    pool.max_component_instance_size(MAX_COMPONENT_INSTANCE_SIZE);
    pool.max_memory_size(max_mem_mib as usize * MIB);
    pool.max_unused_warm_slots(concurrency); // keep freed slots warm for reuse

    let mut config = Config::new();
    config.wasm_component_model(true);
    config.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
    config.memory_guard_size(MEMORY_GUARD_SIZE);
    Ok(Engine::new(&config)?)
}

pub fn make_wasi_ctx() -> WasiCtx {
    WasiCtxBuilder::new()
        .inherit_env()
        .inherit_stdout()
        .inherit_stderr()
        .build()
}

pub fn make_store_wasi_ctx(data_dir: &str) -> Result<WasiCtx> {
    Ok(WasiCtxBuilder::new()
        .inherit_env()
        .inherit_stdout()
        .inherit_stderr()
        .preopened_dir(data_dir, "/data", DirPerms::READ, FilePerms::READ)?
        .build())
}

/// The Rust type stored in the resource table for each `host:storage/collection.connection`
/// resource handle.  One handle per store, opened by name at init time.
pub struct MongoCollection {
    pub inner: Arc<mongodb::Collection<mongodb::bson::Document>>,
}

/// Store data for single-collection ABI and store-mode hosts.
pub struct StoreData {
    pub wasi:  WasiCtx,
    pub table: ResourceTable,
    pub db:    Arc<mongodb::Database>,
    pub cache: Arc<Cache>,
}

impl wasmtime_wasi::WasiView for StoreData {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

pub fn bson_to_json(doc: &mongodb::bson::Document) -> Result<Vec<u8>> {
    let mut doc = doc.clone();
    doc.remove("_id");
    let val = mongodb::bson::from_document::<serde_json::Value>(doc)?;
    Ok(serde_json::to_vec(&val)?)
}

pub fn json_to_bson(bytes: &[u8]) -> Result<mongodb::bson::Document> {
    let val: serde_json::Value = serde_json::from_slice(bytes)?;
    Ok(mongodb::bson::to_document(&val)?)
}

pub async fn mongo_find_all(
    collection: &Arc<mongodb::Collection<mongodb::bson::Document>>,
) -> Result<Vec<Vec<u8>>> {
    use futures::TryStreamExt;
    let mut cursor = collection.find(mongodb::bson::doc! {}).await?;
    let mut out = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        out.push(bson_to_json(&doc)?);
    }
    Ok(out)
}

pub async fn mongo_count(collection: &Arc<mongodb::Collection<mongodb::bson::Document>>) -> Result<u64> {
    Ok(collection.count_documents(mongodb::bson::doc! {}).await?)
}

/// Targeted single-document query
pub async fn mongo_find_one(
    collection: &Arc<mongodb::Collection<mongodb::bson::Document>>,
    filter: &[u8],
) -> Result<Option<Vec<u8>>> {
    let filter = json_to_bson(filter)?;
    match collection.find_one(filter).await? {
        Some(doc) => Ok(Some(bson_to_json(&doc)?)),
        None => Ok(None),
    }
}

/// Targeted multi-document query
pub async fn mongo_find(
    collection: &Arc<mongodb::Collection<mongodb::bson::Document>>,
    filter: &[u8],
) -> Result<Vec<Vec<u8>>> {
    use futures::TryStreamExt;
    let filter = json_to_bson(filter)?;
    let mut cursor = collection.find(filter).await?;
    let mut out = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        out.push(bson_to_json(&doc)?);
    }
    Ok(out)
}

pub async fn mongo_insert_many(
    collection: &Arc<mongodb::Collection<mongodb::bson::Document>>,
    docs: Vec<Vec<u8>>,
) -> Result<()> {
    let bson_docs: Vec<mongodb::bson::Document> =
        docs.iter().map(|b| json_to_bson(b)).collect::<Result<_>>()?;
    collection.insert_many(bson_docs).await?;
    Ok(())
}

pub async fn mongo_insert_one(
    collection: &Arc<mongodb::Collection<mongodb::bson::Document>>,
    doc: Vec<u8>,
) -> Result<()> {
    let bson_doc = json_to_bson(&doc)?;
    collection.insert_one(bson_doc).await?;
    Ok(())
}

/// Host data for svc-mode hosts.
pub struct SvcHostData<C> {
    pub wasi:         WasiCtx,
    pub table:        ResourceTable,
    pub store_client: Arc<Mutex<C>>,
    pub cache:        Arc<Cache>,
}

impl<C: Send> wasmtime_wasi::WasiView for SvcHostData<C> {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

/// Implements `host::storage::collection::HostConnection` (resource lifecycle) and
/// `host::storage::collection::Host` (free functions) on a type that has:
///   - `table(&mut self) -> &mut ResourceTable`  (via WasiView)
///   - `pub db: Arc<mongodb::Database>`
///
/// The host's `bindgen!` must map `"host:storage/collection.connection"`
/// to `host_lib::MongoCollection` via the `with` field.
///
/// Note: wasmtime generates the Host trait methods with bare return types (not Result),
/// matching the WIT interface which has no error type on these functions.
#[macro_export]
macro_rules! impl_collection_host {
    ($T:ty) => {
        impl host::storage::collection::HostConnection for $T {
            async fn open(
                &mut self,
                name: String,
            ) -> ::wasmtime::component::Resource<$crate::MongoCollection> {
                let col = self.db.collection::<::mongodb::bson::Document>(&name);
                let mc = $crate::MongoCollection { inner: ::std::sync::Arc::new(col) };
                <$T as ::wasmtime_wasi::WasiView>::ctx(self).table
                    .push(mc)
                    .expect("resource table push")
            }

            async fn drop(
                &mut self,
                rep: ::wasmtime::component::Resource<$crate::MongoCollection>,
            ) -> ::wasmtime::Result<()> {
                <$T as ::wasmtime_wasi::WasiView>::ctx(self).table.delete(rep)?;
                Ok(())
            }
        }

        impl host::storage::collection::Host for $T {
            async fn count(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
            ) -> u64 {
                let col = <$T as ::wasmtime_wasi::WasiView>::ctx(self).table
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_count(&col).await.unwrap_or(0)
            }

            async fn find_all(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
            ) -> ::std::vec::Vec<::std::vec::Vec<u8>> {
                let col = <$T as ::wasmtime_wasi::WasiView>::ctx(self).table
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_find_all(&col).await.unwrap_or_default()
            }

            async fn find_one(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
                filter: ::std::vec::Vec<u8>,
            ) -> ::core::option::Option<::std::vec::Vec<u8>> {
                let col = <$T as ::wasmtime_wasi::WasiView>::ctx(self).table
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_find_one(&col, &filter).await.ok().flatten()
            }

            async fn find(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
                filter: ::std::vec::Vec<u8>,
            ) -> ::std::vec::Vec<::std::vec::Vec<u8>> {
                let col = <$T as ::wasmtime_wasi::WasiView>::ctx(self).table
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_find(&col, &filter).await.unwrap_or_default()
            }

            async fn insert_one(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
                doc: ::std::vec::Vec<u8>,
            ) {
                let col = <$T as ::wasmtime_wasi::WasiView>::ctx(self).table
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_insert_one(&col, doc).await.unwrap()
            }

            async fn insert_many(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
                docs: ::std::vec::Vec<::std::vec::Vec<u8>>,
            ) {
                let col = <$T as ::wasmtime_wasi::WasiView>::ctx(self).table
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_insert_many(&col, docs).await.unwrap()
            }
        }
    };
}

#[macro_export]
macro_rules! impl_cache_host {
    ($T:ty) => {
        impl host::cache::keyvalue::Host for $T {
            async fn get(&mut self, key: String) -> ::core::option::Option<::std::vec::Vec<u8>> {
                self.cache.read().unwrap().get(&key).cloned()
            }
            async fn set(&mut self, key: String, value: ::std::vec::Vec<u8>) {
                self.cache.write().unwrap().insert(key, value);
            }
        }
    };
}

pub struct SvcHost<Pre, Client> {
    pub engine:       Arc<wasmtime::Engine>,
    pub pre:          Arc<Pre>,
    pub store_client: Arc<Mutex<Client>>,
    pub cache:        Arc<Cache>,
}

impl<Pre: Send + Sync, Client: Send + Sync + 'static> SvcHost<Pre, Client> {
    pub fn new(
        engine:       Arc<wasmtime::Engine>,
        pre:          Arc<Pre>,
        store_client: Arc<Mutex<Client>>,
        cache:        Arc<Cache>,
    ) -> Self {
        Self { engine, pre, store_client, cache }
    }

    pub fn make_store(&self) -> (wasmtime::Store<SvcHostData<Client>>, Arc<Pre>) {
        let data = SvcHostData {
            wasi:         make_wasi_ctx(),
            table:        ResourceTable::new(),
            store_client: self.store_client.clone(),
            cache:        self.cache.clone(),
        };
        (wasmtime::Store::new(&self.engine, data), self.pre.clone())
    }
}

/// Generates `pub type HostData = SvcHostData<$Client>;`
#[macro_export]
macro_rules! svc_host_data {
    ($Client:ty) => {
        pub type HostData = $crate::SvcHostData<$Client>;
    };
}

/// Boilerplate store-service types for store-mode hosts.
/// $Pre must be the wasmtime-generated `<WorldName>Pre<StoreData>` type.
#[macro_export]
macro_rules! define_store_service {
    ($World:ty, $Pre:ty) => {
        struct StoreGrpcService {
            engine: ::std::sync::Arc<::wasmtime::Engine>,
            pre:    ::std::sync::Arc<$Pre>,
            db:     ::std::sync::Arc<::mongodb::Database>,
            cache:  ::std::sync::Arc<$crate::Cache>,
        }

        impl StoreGrpcService {
            async fn new_instance(
                &self,
            ) -> ::anyhow::Result<(::wasmtime::Store<$crate::StoreData>, $World)> {
                let data = $crate::StoreData {
                    wasi:  $crate::make_wasi_ctx(),
                    table: ::wasmtime_wasi::ResourceTable::new(),
                    db:    self.db.clone(),
                    cache: self.cache.clone(),
                };
                let mut store = ::wasmtime::Store::new(&self.engine, data);
                let instance = self.pre.instantiate_async(&mut store).await?;
                Ok((store, instance))
            }
        }
    };
}

/// Generates `pub async fn run()` for SVC-Pre-mode hosts (wasm service + gRPC store, per-request instantiation).
/// Uses `host_lib::SvcHost<$Pre, $Client>` as the concrete host type.
#[macro_export]
macro_rules! run_svc_pre {
    ($World:ty, $Pre:ty, $Client:ty,
     $default_store:literal, $default_addr:literal, $default_wasm:literal, $label:literal) => {
        pub async fn run() -> ::anyhow::Result<()> {
            use ::std::sync::Arc;
            use ::tokio::sync::Mutex;
            use ::wasmtime::component::{Component, Linker};

            let store_addr  = ::std::env::var("STORE_ADDR").unwrap_or_else(|_| $default_store.into());
            let listen_addr: ::std::net::SocketAddr = ::std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| $default_addr.into()).parse()?;
            let wasm_file   = ::std::env::var("WASM_FILE").unwrap_or_else(|_| $default_wasm.into());

            let store_client = <$Client>::connect(store_addr).await?;
            let store_client = Arc::new(Mutex::new(store_client));
            let cache        = Arc::new($crate::Cache::new(::std::collections::HashMap::new()));

            let engine = Arc::new($crate::make_engine()?);
            let mut linker: Linker<$crate::SvcHostData<$Client>> = Linker::new(&engine);
            ::wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker::<_, ::wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;

            let component = Component::from_file(&engine, &wasm_file)?;
            let pre = Arc::new(<$Pre>::new(linker.instantiate_pre(&component)?)?);

            println!("{} listening on {listen_addr}", $label);
            crate::grpc::serve(
                Arc::new($crate::SvcHost::new(engine, pre, store_client, cache)),
                listen_addr,
            ).await
        }
    };
}

/// Entry point for hosts that have both `store` and `svc` modes.
/// Declares `mod grpc; mod store; mod svc;` and dispatches via the `MODE` env var.
#[macro_export]
macro_rules! store_svc_main {
    () => {
        mod grpc;
        mod store;
        mod svc;

        #[tokio::main]
        async fn main() -> ::anyhow::Result<()> {
            let mode = ::std::env::var("MODE").unwrap_or_else(|_| "svc".into());
            match mode.as_str() {
                "store" => store::run().await,
                "svc"   => svc::run().await,
                other   => ::anyhow::bail!("unknown MODE={other}; expected store|svc"),
            }
        }
    };
}

/// Generates `pub async fn run()` for STORE-mode hosts (wasm store + gRPC server).
/// The wasm selects its collection via `connection::open(name)`.
/// $Pre must be the wasmtime-generated `<WorldName>Pre<StoreData>` type.
#[macro_export]
macro_rules! run_store {
    ($World:ty, $Pre:ty, $GrpcServer:ident, $db:literal,
     $default_addr:literal, $default_wasm:literal, $label:literal) => {
        pub async fn run() -> ::anyhow::Result<()> {
            use ::std::sync::Arc;
            use ::wasmtime::component::{Component, Linker};
            use ::tonic::transport::Server;

            let mongo_uri  = ::std::env::var("MONGO_URI")
                .unwrap_or_else(|_| "mongodb://localhost:27017".into());
            let listen_addr = ::std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| $default_addr.into());
            let wasm_file  = ::std::env::var("WASM_FILE")
                .unwrap_or_else(|_| $default_wasm.into());

            let mongo = ::mongodb::Client::with_uri_str(&mongo_uri).await?;
            let db    = Arc::new(mongo.database($db));
            let cache = Arc::new($crate::Cache::new(::std::collections::HashMap::new()));

            let engine     = $crate::make_engine()?;
            let mut linker: Linker<$crate::StoreData> = Linker::new(&engine);
            ::wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker::<_, ::wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;

            let component = Component::from_file(&engine, &wasm_file)?;
            let pre       = <$Pre>::new(linker.instantiate_pre(&component)?)?;

            let svc = StoreGrpcService {
                engine: Arc::new(engine),
                pre:    Arc::new(pre),
                db,
                cache,
            };

            println!("{} [store] listening on {listen_addr}", $label);

            Server::builder()
                .add_service($GrpcServer::new(svc))
                .serve(listen_addr.parse()?)
                .await?;
            Ok(())
        }
    };
}
