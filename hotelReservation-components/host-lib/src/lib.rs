use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::{Config, Engine};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder};
pub type Cache = std::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>;

pub fn make_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.async_support(true);
    config.wasm_component_model(true);
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

/// The Rust type stored in the resource table for each `host:storage/collection/connection`
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
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
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
    fn ctx(&mut self)   -> &mut WasiCtx       { &mut self.wasi  }
    fn table(&mut self) -> &mut ResourceTable  { &mut self.table }
}

/// Implements `host::storage::collection::HostConnection` (resource lifecycle) and
/// `host::storage::collection::Host` (free functions) on a type that has:
///   - `table(&mut self) -> &mut ResourceTable`  (via WasiView)
///   - `pub db: Arc<mongodb::Database>`
///
/// The host's `bindgen!` must map `"host:storage/collection/connection"`
/// to `host_lib::MongoCollection` via the `with` field.
///
/// Note: wasmtime generates the Host trait methods with bare return types (not Result),
/// matching the WIT interface which has no error type on these functions.
#[macro_export]
macro_rules! impl_collection_host {
    ($T:ty) => {
        #[::async_trait::async_trait]
        impl host::storage::collection::HostConnection for $T {
            async fn open(
                &mut self,
                name: String,
            ) -> ::wasmtime::component::Resource<$crate::MongoCollection> {
                let col = self.db.collection::<::mongodb::bson::Document>(&name);
                let mc = $crate::MongoCollection { inner: ::std::sync::Arc::new(col) };
                <$T as ::wasmtime_wasi::WasiView>::table(self)
                    .push(mc)
                    .expect("resource table push")
            }

            async fn drop(
                &mut self,
                rep: ::wasmtime::component::Resource<$crate::MongoCollection>,
            ) -> ::anyhow::Result<()> {
                <$T as ::wasmtime_wasi::WasiView>::table(self).delete(rep)?;
                Ok(())
            }
        }

        #[::async_trait::async_trait]
        impl host::storage::collection::Host for $T {
            async fn count(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
            ) -> u64 {
                let col = <$T as ::wasmtime_wasi::WasiView>::table(self)
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_count(&col).await.unwrap_or(0)
            }

            async fn find_all(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
            ) -> ::std::vec::Vec<::std::vec::Vec<u8>> {
                let col = <$T as ::wasmtime_wasi::WasiView>::table(self)
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_find_all(&col).await.unwrap_or_default()
            }

            async fn find_one(
                &mut self,
                _c: ::wasmtime::component::Resource<$crate::MongoCollection>,
                _filter: ::std::vec::Vec<u8>,
            ) -> ::core::option::Option<::std::vec::Vec<u8>> {
                unimplemented!()
            }

            async fn find(
                &mut self,
                _c: ::wasmtime::component::Resource<$crate::MongoCollection>,
                _filter: ::std::vec::Vec<u8>,
            ) -> ::std::vec::Vec<::std::vec::Vec<u8>> {
                unimplemented!()
            }

            async fn insert_one(
                &mut self,
                _c: ::wasmtime::component::Resource<$crate::MongoCollection>,
                _doc: ::std::vec::Vec<u8>,
            ) {
                unimplemented!()
            }

            async fn insert_many(
                &mut self,
                c: ::wasmtime::component::Resource<$crate::MongoCollection>,
                docs: ::std::vec::Vec<::std::vec::Vec<u8>>,
            ) {
                let col = <$T as ::wasmtime_wasi::WasiView>::table(self)
                    .get(&c).unwrap().inner.clone();
                $crate::mongo_insert_many(&col, docs).await.unwrap()
            }
        }
    };
}

#[macro_export]
macro_rules! impl_cache_host {
    ($T:ty) => {
        #[::async_trait::async_trait]
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

/// Generates `pub type HostData = SvcHostData<$Client>;`
#[macro_export]
macro_rules! svc_host_data {
    ($Client:ty) => {
        pub type HostData = $crate::SvcHostData<$Client>;
    };
}

/// Boilerplate store-service types for store-mode hosts.
#[macro_export]
macro_rules! define_store_service {
    ($World:ty) => {
        type SharedStore    = ::std::sync::Arc<::tokio::sync::Mutex<::wasmtime::Store<$crate::StoreData>>>;
        type SharedInstance = ::std::sync::Arc<$World>;

        struct StoreGrpcService {
            store:    SharedStore,
            instance: SharedInstance,
        }
    };
}

/// Generates `pub async fn run()` for ABI-mode hosts (composed wasm + direct MongoDB).
/// The wasm selects its collection via `connection::open(name)`.
#[macro_export]
macro_rules! run_abi {
    ($World:ty, $accessor:ident, $db:literal,
     $default_addr:literal, $default_wasm:literal, $label:literal) => {
        pub async fn run() -> ::anyhow::Result<()> {
            use ::std::sync::Arc;
            use ::tokio::sync::Mutex;
            use ::wasmtime::component::{Component, Linker};
            use ::wasmtime::Store;

            let mongo_uri   = ::std::env::var("MONGO_URI")
                .unwrap_or_else(|_| "mongodb://localhost:27017".into());
            let listen_addr: ::std::net::SocketAddr = ::std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| $default_addr.into())
                .parse()?;
            let data_dir    = ::std::env::var("DATA_DIR")
                .unwrap_or_else(|_| "/data".into());
            let wasm_file   = ::std::env::var("WASM_FILE")
                .unwrap_or_else(|_| $default_wasm.into());

            let mongo = ::mongodb::Client::with_uri_str(&mongo_uri).await?;
            let db    = Arc::new(mongo.database($db));

            let engine     = $crate::make_engine()?;
            let mut linker: Linker<$crate::StoreData> = Linker::new(&engine);
            ::wasmtime_wasi::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker(&mut linker, |d| d)?;

            let data = $crate::StoreData {
                wasi:  $crate::make_store_wasi_ctx(&data_dir)?,
                table: ::wasmtime_wasi::ResourceTable::new(),
                db,
                cache: ::std::sync::Arc::new($crate::Cache::new(::std::collections::HashMap::new())),
            };
            let mut store = Store::new(&engine, data);

            let component = Component::from_file(&engine, &wasm_file)?;
            let instance  = <$World>::instantiate_async(&mut store, &component, &linker).await?;

            println!("{} [abi] listening on {listen_addr}", $label);

            crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
        }
    };
}

/// Generates `pub async fn run()` for SVC-mode hosts (wasm service + gRPC store).
#[macro_export]
macro_rules! run_svc {
    ($World:ty, $Client:ty, $accessor:ident,
     $default_store:literal, $default_addr:literal, $default_wasm:literal, $label:literal) => {
        pub async fn run() -> ::anyhow::Result<()> {
            use ::std::sync::Arc;
            use ::tokio::sync::Mutex;
            use ::wasmtime::component::Linker;

            let store_addr  = ::std::env::var("STORE_ADDR")
                .unwrap_or_else(|_| $default_store.into());
            let listen_addr: ::std::net::SocketAddr = ::std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| $default_addr.into())
                .parse()?;
            let wasm_file   = ::std::env::var("WASM_FILE")
                .unwrap_or_else(|_| $default_wasm.into());

            let store_client = <$Client>::connect(store_addr).await?;
            let store_client = Arc::new(Mutex::new(store_client));

            let engine     = $crate::make_engine()?;
            let mut linker: Linker<$crate::SvcHostData<$Client>> = Linker::new(&engine);
            ::wasmtime_wasi::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker(&mut linker, |d| d)?;

            let data = $crate::SvcHostData {
                wasi:         $crate::make_wasi_ctx(),
                table:        ::wasmtime_wasi::ResourceTable::new(),
                store_client,
                cache:        ::std::sync::Arc::new($crate::Cache::new(::std::collections::HashMap::new())),
            };
            let mut store = ::wasmtime::Store::new(&engine, data);

            let component = ::wasmtime::component::Component::from_file(&engine, &wasm_file)?;
            let instance  = <$World>::instantiate_async(&mut store, &component, &linker).await?;

            println!("{} [svc] listening on {listen_addr}", $label);

            crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
        }
    };
}

/// Generates `pub async fn run()` for STORE-mode hosts (wasm store + gRPC server).
/// The wasm selects its collection via `connection::open(name)`.
#[macro_export]
macro_rules! run_store {
    ($World:ty, $GrpcServer:ident, $accessor:ident, $db:literal,
     $default_addr:literal, $default_wasm:literal, $label:literal) => {
        pub async fn run() -> ::anyhow::Result<()> {
            use ::std::sync::Arc;
            use ::tokio::sync::Mutex;
            use ::wasmtime::component::{Component, Linker};
            use ::wasmtime::Store;
            use ::tonic::transport::Server;

            let mongo_uri  = ::std::env::var("MONGO_URI")
                .unwrap_or_else(|_| "mongodb://localhost:27017".into());
            let listen_addr = ::std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| $default_addr.into());
            let data_dir   = ::std::env::var("DATA_DIR")
                .unwrap_or_else(|_| "/data".into());
            let wasm_file  = ::std::env::var("WASM_FILE")
                .unwrap_or_else(|_| $default_wasm.into());

            let mongo = ::mongodb::Client::with_uri_str(&mongo_uri).await?;
            let db    = Arc::new(mongo.database($db));

            let engine     = $crate::make_engine()?;
            let mut linker: Linker<$crate::StoreData> = Linker::new(&engine);
            ::wasmtime_wasi::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker(&mut linker, |d| d)?;

            let data = $crate::StoreData {
                wasi:  $crate::make_store_wasi_ctx(&data_dir)?,
                table: ::wasmtime_wasi::ResourceTable::new(),
                db,
                cache: ::std::sync::Arc::new($crate::Cache::new(::std::collections::HashMap::new())),
            };
            let mut store = Store::new(&engine, data);

            let component = Component::from_file(&engine, &wasm_file)?;
            let instance  = <$World>::instantiate_async(&mut store, &component, &linker).await?;
            instance.$accessor().call_init(&mut store).await?;

            let store    = Arc::new(Mutex::new(store));
            let instance = Arc::new(instance);

            println!("{} [store] listening on {listen_addr}", $label);

            Server::builder()
                .add_service($GrpcServer::new(StoreGrpcService { store, instance }))
                .serve(listen_addr.parse()?)
                .await?;
            Ok(())
        }
    };
}
