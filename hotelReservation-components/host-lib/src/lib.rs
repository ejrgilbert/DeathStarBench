use anyhow::Result;
use std::sync::Arc;
use wasmtime::{Config, Engine, InstanceAllocationStrategy, PoolingAllocationConfig, Store};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView};
pub type Cache = std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>;

/// gRPC contract for the cache service (compiled from `proto/cache.proto` by
/// this crate's `build.rs`). A cache-using svc host holds a `CacheClient` and
/// bridges its component's `cache:keyvalue/keyvalue` import to these RPCs.
pub mod cache_proto {
    tonic::include_proto!("cache");
}
/// Cheaply-cloneable tonic client for the cache service (see `SvcHostData`).
pub type CacheClient = cache_proto::cache_client::CacheClient<tonic::transport::Channel>;

/// Register no-op host stubs for the `wasi:otel/*`
pub fn add_otel_stubs<T>(linker: &mut wasmtime::component::Linker<T>) -> Result<()> {
    use wasmtime::component::Val;

    const OTEL_TRACING: &str = "wasi:otel/tracing@0.2.0-rc.2";
    const OTEL_METRICS: &str = "wasi:otel/metrics@0.2.0-rc.2";
    const OTEL_LOGS: &str = "wasi:otel/logs@0.2.0-rc.2";
    const BUILTIN_CFG: &str = "splicer:builtin-config/get@0.1.0";

    // splicer:builtin-config/get — get(key: string) -> option<string>.
    // None → the builtin falls back to its hardcoded (manifest) default.
    {
        let mut iface = linker.instance(BUILTIN_CFG)?;
        iface.func_new("get", |_store, _ty, _params, results| {
            results[0] = Val::Option(None);
            Ok(())
        })?;
    }

    // wasi:otel/tracing — on-start / on-end (no-op) + outer-span-context.
    {
        let mut iface = linker.instance(OTEL_TRACING)?;
        iface.func_new("on-start", |_store, _ty, _params, _results| Ok(()))?;
        iface.func_new("on-end", |_store, _ty, _params, _results| Ok(()))?;
        // Return an all-empty context so the builtin mints a fresh trace-id
        // rather than inheriting a host parent span.
        iface.func_new("outer-span-context", |_store, _ty, _params, results| {
            results[0] = Val::Record(vec![
                ("trace-id".into(), Val::String(String::new())),
                ("span-id".into(), Val::String(String::new())),
                ("trace-flags".into(), Val::Flags(vec![])),
                ("is-remote".into(), Val::Bool(false)),
                ("trace-state".into(), Val::List(vec![])),
            ]);
            Ok(())
        })?;
    }

    // wasi:otel/logs — on-emit (no-op).
    {
        let mut iface = linker.instance(OTEL_LOGS)?;
        iface.func_new("on-emit", |_store, _ty, _params, _results| Ok(()))?;
    }

    // wasi:otel/metrics — export(resource-metrics) -> result<_, error>.
    {
        let mut iface = linker.instance(OTEL_METRICS)?;
        iface.func_new("export", |_store, _ty, _params, results| {
            results[0] = Val::Result(Ok(None));
            Ok(())
        })?;
    }

    Ok(())
}

/// Async factory that builds one fresh warm `(Store<T>, I)`. Retained by the
/// pool so poisoned instances can be replaced (see `Checked`'s drop).
type MakeFn<T, I> =
    Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(Store<T>, I)>> + Send>> + Send + Sync>;

/// A fixed-size pool of pre-instantiated `(Store<T>, I)` pairs to reuse warm
/// instances on requests.
///
/// Backed by an `async_channel` MPMC channel whose receiver is cloneable, so
/// concurrent `checkout()`s recv in parallel with no shared lock. (The previous
/// `tokio::mpsc` has a single non-clonable consumer, which forced a
/// `Mutex<Receiver>` held across `recv().await` — a hot-path serialization point
/// that pinned one core and capped throughput under load.)
pub struct InstancePool<T: Send + 'static, I: Send + 'static> {
    tx: async_channel::Sender<(Store<T>, I)>,
    rx: async_channel::Receiver<(Store<T>, I)>,
    make: MakeFn<T, I>,
}

impl<T: Send + 'static, I: Send + 'static> InstancePool<T, I> {
    /// Instantiate `size` warm instances up front via `make`. `make` is retained
    /// so instances left non-reentrant by a trap/cancellation can be rebuilt.
    pub async fn build<F, Fut>(size: usize, make: F) -> Result<Self>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(Store<T>, I)>> + Send + 'static,
    {
        let make: MakeFn<T, I> = Arc::new(move || Box::pin(make()));
        let size = size.max(1);
        let (tx, rx) = async_channel::bounded(size);
        for _ in 0..size {
            tx.try_send(make().await?)
                .map_err(|_| anyhow::anyhow!("instance pool prefill overflow"))?;
        }
        Ok(Self { tx, rx, make })
    }

    /// Borrow a warm instance, awaiting until one is free. The instance is only
    /// returned to the pool if the caller calls [`Checked::commit`] after a
    /// clean guest call; otherwise (trap, error, or cancellation) it is dropped
    /// and a fresh instance is rebuilt to keep the pool full. This is required
    /// because a wasm component instance whose call did not return cleanly is
    /// left non-reentrant — recycling it makes every later call trap with
    /// "cannot enter component instance".
    pub async fn checkout(&self) -> Checked<T, I> {
        // No lock: the async_channel receiver is cloneable/MPMC, so concurrent
        // checkouts recv in parallel.
        let item = self.rx.recv().await.expect("instance pool closed");
        Checked {
            tx: self.tx.clone(),
            make: self.make.clone(),
            item: Some(item),
            committed: false,
        }
    }
}

/// RAII handle to a checked-out `(Store<T>, I)`. On drop it returns the instance
/// to the pool only if [`Checked::commit`] was called; otherwise it discards the
/// (possibly poisoned) instance and asynchronously rebuilds a replacement.
pub struct Checked<T: Send + 'static, I: Send + 'static> {
    tx: async_channel::Sender<(Store<T>, I)>,
    make: MakeFn<T, I>,
    item: Option<(Store<T>, I)>,
    committed: bool,
}

impl<T: Send + 'static, I: Send + 'static> Checked<T, I> {
    /// `(&mut Store, &Instance)` for making the guest call.
    pub fn parts(&mut self) -> (&mut Store<T>, &I) {
        let (s, i) = self.item.as_mut().expect("checked-out instance");
        (s, i)
    }

    /// Mark the guest call as completed cleanly, so the warm instance is returned
    /// to the pool for reuse. Call this only after the call returned `Ok` and any
    /// store-borrowing work (e.g. draining the response body) has finished.
    pub fn commit(mut self) {
        self.committed = true;
        // drop returns the instance to the pool.
    }
}

impl<T: Send + 'static, I: Send + 'static> Drop for Checked<T, I> {
    fn drop(&mut self) {
        let Some(item) = self.item.take() else { return };

        if self.committed {
            // Capacity was reserved when we checked out, so this never blocks.
            let _ = self.tx.try_send(item);
            return;
        }

        // The call trapped, errored, or was cancelled mid-flight (e.g. the client
        // disconnected). The component instance may be left non-reentrant, so it
        // must not be recycled. Drop it and rebuild a fresh warm instance to
        // refill the slot we vacated at checkout.
        drop(item);
        let tx = self.tx.clone();
        let make = self.make.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                match make().await {
                    Ok(fresh) => {
                        let _ = tx.try_send(fresh);
                    }
                    Err(e) => eprintln!("[instance-pool] refill after poison failed: {e:?}"),
                }
            });
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
///
/// `store_client` is a plain (cheaply cloneable) tonic client, NOT an
/// `Arc<Mutex<_>>`: a tonic `Client<Channel>` clone shares the underlying
/// connection and multiplexes concurrent requests over HTTP/2. Wrapping it in a
/// mutex would serialize every downstream call to one in-flight request at a
/// time — a hard throughput ceiling. Clone it per call instead (`.clone()`).
pub struct SvcHostData<C> {
    pub wasi:         WasiCtx,
    pub table:        ResourceTable,
    pub store_client: C,
    /// In-process cache map. Used by non-cache-using hosts (which still satisfy
    /// an unused `host:cache/keyvalue` import) via `impl_cache_host!`.
    pub cache:        Arc<Cache>,
    /// gRPC client to the cache service, set when `CACHE_ADDR` is configured.
    /// Cache-using svc hosts (rate/profile/review/reservation) bridge their
    /// component's `cache:keyvalue/keyvalue` import to it via
    /// `impl_cache_svc_grpc!`, so a cache access is a real network hop.
    pub cache_client: Option<CacheClient>,
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

/// Implements the service-facing `cache:keyvalue/keyvalue` import by bridging to
/// the cache service over gRPC (TCP mode). `$T` must have a `cache_client:
/// Option<CacheClient>` field (see `SvcHostData`), set from `CACHE_ADDR`. Used by
/// the cache-using svc hosts (rate/profile/review/reservation) so a cache access
/// is a real network hop, matching the native memcached round-trip.
#[macro_export]
macro_rules! impl_cache_svc_grpc {
    ($T:ty) => {
        impl cache::keyvalue::keyvalue::Host for $T {
            async fn get(&mut self, key: String) -> ::core::option::Option<::std::vec::Vec<u8>> {
                let mut client = self.cache_client.clone()
                    .expect("CACHE_ADDR not set for cache-using svc host");
                let resp = client
                    .get($crate::cache_proto::GetRequest { key })
                    .await
                    .expect("gRPC cache Get failed")
                    .into_inner();
                if resp.found { ::core::option::Option::Some(resp.value) } else { ::core::option::Option::None }
            }
            async fn get_multi(
                &mut self,
                keys: ::std::vec::Vec<String>,
            ) -> ::std::vec::Vec<::core::option::Option<::std::vec::Vec<u8>>> {
                let mut client = self.cache_client.clone()
                    .expect("CACHE_ADDR not set for cache-using svc host");
                let resp = client
                    .get_multi($crate::cache_proto::GetMultiRequest { keys })
                    .await
                    .expect("gRPC cache GetMulti failed")
                    .into_inner();
                resp.entries.into_iter()
                    .map(|e| if e.found { ::core::option::Option::Some(e.value) } else { ::core::option::Option::None })
                    .collect()
            }
            async fn set(&mut self, key: String, value: ::std::vec::Vec<u8>) {
                // Fire-and-forget, matching the native services' `go Set(...)`:
                // the response path never waits on the cache write.
                let mut client = self.cache_client.clone()
                    .expect("CACHE_ADDR not set for cache-using svc host");
                ::tokio::spawn(async move {
                    let _ = client.set($crate::cache_proto::SetRequest { key, value }).await;
                });
            }
        }
    };
}

/// Implements the service-facing `cache:keyvalue/keyvalue` import in-process
/// against `$T`'s `cache: Arc<Cache>` map. Used by the composed ABI host, where
/// cache access must stay in-process (no network hop) — the whole point of the
/// composition fast-path.
#[macro_export]
macro_rules! impl_cache_svc_inproc {
    ($T:ty) => {
        impl cache::keyvalue::keyvalue::Host for $T {
            async fn get(&mut self, key: String) -> ::core::option::Option<::std::vec::Vec<u8>> {
                self.cache.read().unwrap().get(&key).cloned()
            }
            async fn get_multi(
                &mut self,
                keys: ::std::vec::Vec<String>,
            ) -> ::std::vec::Vec<::core::option::Option<::std::vec::Vec<u8>>> {
                let map = self.cache.read().unwrap();
                keys.iter().map(|k| map.get(k).cloned()).collect()
            }
            async fn set(&mut self, key: String, value: ::std::vec::Vec<u8>) {
                self.cache.write().unwrap().insert(key, value);
            }
        }
    };
}

/// A gRPC-fronted svc host that serves every request from a warm pool of reused
/// `(Store<SvcHostData<Client>>, World)` instances instead of instantiating a
/// fresh one per request.
///
/// Reuse is essential for these services: each loads its dataset into a
/// per-instance global on first use and expects it to persist across requests
/// (matching the Go original, which loads once at startup). A fresh instance per
/// request makes them reload the whole dataset from the store on every call.
///
/// Build the pool with [`InstancePool::build`] (the `run_svc_pre!` macro does
/// this) and hand it here; the gRPC handler then does `checkout()` → call →
/// [`Checked::commit`]. This is the same warm-instance-reuse core the frontend
/// host uses, factored out so every svc host shares it.
pub struct PooledSvcHost<Client: Send + 'static, World: Send + 'static> {
    pool: InstancePool<SvcHostData<Client>, World>,
}

impl<Client: Send + 'static, World: Send + 'static> PooledSvcHost<Client, World> {
    pub fn new(pool: InstancePool<SvcHostData<Client>, World>) -> Self {
        Self { pool }
    }

    /// Borrow a warm instance for one request. Call [`Checked::commit`] on the
    /// returned guard after a clean guest call so the instance returns to the
    /// pool; otherwise (error / panic / cancellation) it is discarded and a
    /// fresh one is rebuilt.
    pub async fn checkout(&self) -> Checked<SvcHostData<Client>, World> {
        self.pool.checkout().await
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
        // Serves gRPC requests from a warm pool of reused instances (built in
        // `run_store!`). Store guests cache their Mongo connection and dataset in
        // per-instance globals, so reuse loads the collection once instead of
        // re-running a full `FindAll` on every request.
        struct StoreGrpcService {
            pool: $crate::InstancePool<$crate::StoreData, $World>,
        }

        impl StoreGrpcService {
            /// Borrow a warm instance for one request. Call `.commit()` on the
            /// returned guard after a clean call so it returns to the pool;
            /// otherwise it is discarded and rebuilt.
            async fn checkout(&self) -> $crate::Checked<$crate::StoreData, $World> {
                self.pool.checkout().await
            }
        }
    };
}

/// Generates `pub async fn run()` for SVC-Pre-mode hosts (wasm service + gRPC
/// store). Serves requests from a warm [`PooledSvcHost`] pool of reused instances
/// (size from `POOL_SIZE`, default 64) instead of instantiating per request, so
/// each service loads its dataset once and reuses it across requests.
#[macro_export]
macro_rules! run_svc_pre {
    ($World:ty, $Pre:ty, $Client:ty,
     $default_store:literal, $default_addr:literal, $default_wasm:literal, $label:literal) => {
        pub async fn run() -> ::anyhow::Result<()> {
            use ::std::sync::Arc;
            use ::wasmtime::component::{Component, Linker};

            let store_addr  = ::std::env::var("STORE_ADDR").unwrap_or_else(|_| $default_store.into());
            let listen_addr: ::std::net::SocketAddr = ::std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| $default_addr.into()).parse()?;
            let wasm_file   = ::std::env::var("WASM_FILE").unwrap_or_else(|_| $default_wasm.into());
            let pool_size: usize = ::std::env::var("POOL_SIZE").ok()
                .and_then(|v| v.parse().ok()).unwrap_or(64);

            // Plain cloneable tonic client (multiplexes concurrent requests);
            // each pooled instance gets its own clone (see SvcHostData).
            let store_client = <$Client>::connect(store_addr).await?;
            let cache        = Arc::new($crate::Cache::new(::std::collections::HashMap::new()));

            // Cache-using hosts set CACHE_ADDR so their `cache:keyvalue/keyvalue`
            // import bridges to the cache service over gRPC (a real network hop).
            // Non-cache hosts leave it unset and keep the unused in-process map.
            let cache_client = match ::std::env::var("CACHE_ADDR") {
                Ok(addr) => Some($crate::CacheClient::connect(addr).await?),
                Err(_)   => None,
            };

            let engine = Arc::new($crate::make_engine()?);
            let mut linker: Linker<$crate::SvcHostData<$Client>> = Linker::new(&engine);
            ::wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker::<_, ::wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;
            $crate::add_otel_stubs(&mut linker)?;

            let component = Component::from_file(&engine, &wasm_file)?;
            let pre = Arc::new(<$Pre>::new(linker.instantiate_pre(&component)?)?);

            // Warm pool of reused instances (see PooledSvcHost): instantiate up
            // front and reuse, so the service's per-instance dataset loads once.
            let pool = $crate::InstancePool::build(pool_size, move || {
                let engine       = engine.clone();
                let pre          = pre.clone();
                let store_client = store_client.clone();
                let cache        = cache.clone();
                let cache_client = cache_client.clone();
                async move {
                    let data = $crate::SvcHostData {
                        wasi:         $crate::make_wasi_ctx(),
                        table:        ::wasmtime_wasi::ResourceTable::new(),
                        store_client,
                        cache,
                        cache_client,
                    };
                    let mut store = ::wasmtime::Store::new(&engine, data);
                    let instance = pre.instantiate_async(&mut store).await?;
                    ::anyhow::Ok((store, instance))
                }
            })
            .await?;

            println!("{} listening on {listen_addr} (instance pool size {pool_size})", $label);
            crate::grpc::serve(
                Arc::new($crate::PooledSvcHost::new(pool)),
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

            let pool_size: usize = ::std::env::var("POOL_SIZE").ok()
                .and_then(|v| v.parse().ok()).unwrap_or(64);

            let mongo = ::mongodb::Client::with_uri_str(&mongo_uri).await?;
            let db    = Arc::new(mongo.database($db));
            let cache = Arc::new($crate::Cache::new(::std::collections::HashMap::new()));

            let engine = Arc::new($crate::make_engine()?);
            let mut linker: Linker<$crate::StoreData> = Linker::new(&engine);
            ::wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker::<_, ::wasmtime::component::HasSelf<_>>(&mut linker, |d| d)?;
            $crate::add_otel_stubs(&mut linker)?;

            let component = Component::from_file(&engine, &wasm_file)?;
            let pre = Arc::new(<$Pre>::new(linker.instantiate_pre(&component)?)?);

            // Warm pool of reused instances so each store loads its collection
            // once (its connection + dataset are cached in per-instance globals).
            let pool = $crate::InstancePool::build(pool_size, move || {
                let engine = engine.clone();
                let pre    = pre.clone();
                let db     = db.clone();
                let cache  = cache.clone();
                async move {
                    let data = $crate::StoreData {
                        wasi:  $crate::make_wasi_ctx(),
                        table: ::wasmtime_wasi::ResourceTable::new(),
                        db,
                        cache,
                    };
                    let mut store = ::wasmtime::Store::new(&engine, data);
                    let instance = pre.instantiate_async(&mut store).await?;
                    ::anyhow::Ok((store, instance))
                }
            })
            .await?;

            let svc = StoreGrpcService { pool };

            println!("{} [store] listening on {listen_addr} (instance pool size {pool_size})", $label);

            Server::builder()
                .add_service($GrpcServer::new(svc))
                .serve(listen_addr.parse()?)
                .await?;
            Ok(())
        }
    };
}
