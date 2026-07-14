use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::{Config, Engine};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder};

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

/// Store data shared by all single-collection store hosts.
/// Each host's bindgen!-generated `host::storage::collection::Host` trait
/// is implemented on this type in the host binary itself (local trait, foreign type — allowed).
pub struct StoreData {
    pub wasi: WasiCtx,
    pub table: ResourceTable,
    pub collection: Arc<mongodb::Collection<bson::Document>>,
}

impl wasmtime_wasi::WasiView for StoreData {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

pub fn bson_to_json(doc: &bson::Document) -> Result<Vec<u8>> {
    let mut doc = doc.clone();
    doc.remove("_id");
    let val = bson::from_document::<serde_json::Value>(doc)?;
    Ok(serde_json::to_vec(&val)?)
}

pub fn json_to_bson(bytes: &[u8]) -> Result<bson::Document> {
    let val: serde_json::Value = serde_json::from_slice(bytes)?;
    Ok(bson::to_document(&val)?)
}

pub async fn mongo_find_all(
    collection: &Arc<mongodb::Collection<bson::Document>>,
) -> Result<Vec<Vec<u8>>> {
    use futures::TryStreamExt;
    let mut cursor = collection.find(bson::doc! {}).await?;
    let mut out = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        out.push(bson_to_json(&doc)?);
    }
    Ok(out)
}

pub async fn mongo_count(collection: &Arc<mongodb::Collection<bson::Document>>) -> Result<u64> {
    Ok(collection.count_documents(bson::doc! {}).await?)
}

pub async fn mongo_insert_many(
    collection: &Arc<mongodb::Collection<bson::Document>>,
    docs: Vec<Vec<u8>>,
) -> Result<()> {
    let bson_docs: Vec<bson::Document> =
        docs.iter().map(|b| json_to_bson(b)).collect::<Result<_>>()?;
    collection.insert_many(bson_docs).await?;
    Ok(())
}

/// Host data for svc-mode hosts.  The store client generic keeps the struct identical
/// across services; only the type parameter differs.
pub struct SvcHostData<C> {
    pub wasi:         WasiCtx,
    pub table:        ResourceTable,
    pub store_client: Arc<Mutex<C>>,
}

impl<C: Send> wasmtime_wasi::WasiView for SvcHostData<C> {
    fn ctx(&mut self)   -> &mut WasiCtx       { &mut self.wasi  }
    fn table(&mut self) -> &mut ResourceTable  { &mut self.table }
}

/// Implements `host::storage::collection::Host` on a type that has a `collection` field
/// of type `Arc<mongodb::Collection<bson::Document>>`.
///
/// The trait is generated locally by each host's `bindgen!` call, so the impl must
/// live in each host crate — this macro just eliminates the copy-paste of the body.
///
/// Usage: `host_lib::impl_collection_host!(YourStoreDataType);`
#[macro_export]
macro_rules! impl_collection_host {
    ($T:ty) => {
        #[::async_trait::async_trait]
        impl host::storage::collection::Host for $T {
            async fn count(&mut self) -> u64 {
                $crate::mongo_count(&self.collection).await.unwrap_or(0)
            }
            async fn find_all(&mut self) -> ::std::vec::Vec<::std::vec::Vec<u8>> {
                $crate::mongo_find_all(&self.collection).await.unwrap_or_default()
            }
            async fn find_one(&mut self, _: ::std::vec::Vec<u8>) -> ::core::option::Option<::std::vec::Vec<u8>> {
                unimplemented!()
            }
            async fn find(&mut self, _: ::std::vec::Vec<u8>) -> ::std::vec::Vec<::std::vec::Vec<u8>> {
                unimplemented!()
            }
            async fn insert_one(&mut self, _: ::std::vec::Vec<u8>) {
                unimplemented!()
            }
            async fn insert_many(&mut self, docs: ::std::vec::Vec<::std::vec::Vec<u8>>) {
                $crate::mongo_insert_many(&self.collection, docs).await.unwrap()
            }
        }
    };
}

/// Generates `pub type HostData = SvcHostData<$Client>;` at module scope so that
/// the store-trait impls written below can reference the short name `HostData`.
#[macro_export]
macro_rules! svc_host_data {
    ($Client:ty) => {
        pub type HostData = $crate::SvcHostData<$Client>;
    };
}

/// Generates the boilerplate store-service types used in every store-mode host:
/// `SharedStore`, `SharedInstance`, and `StoreGrpcService`.
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
///
/// Arguments: WorldType, init_accessor, "db-name", "collection", "default-addr",
///            "default-wasm-file", "service-label"
#[macro_export]
macro_rules! run_abi {
    ($World:ty, $accessor:ident, $db:literal, $coll:literal,
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

            let mongo      = ::mongodb::Client::with_uri_str(&mongo_uri).await?;
            let collection = Arc::new(mongo.database($db).collection($coll));

            let engine     = $crate::make_engine()?;
            let mut linker: Linker<$crate::StoreData> = Linker::new(&engine);
            ::wasmtime_wasi::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker(&mut linker, |d| d)?;

            let data = $crate::StoreData {
                wasi:       $crate::make_store_wasi_ctx(&data_dir)?,
                table:      ::wasmtime_wasi::ResourceTable::new(),
                collection,
            };
            let mut store = Store::new(&engine, data);

            let component = Component::from_file(&engine, &wasm_file)?;
            let instance  = <$World>::instantiate_async(&mut store, &component, &linker).await?;
            instance.$accessor().call_init(&mut store).await?;

            println!("{} [abi] listening on {listen_addr}", $label);

            crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
        }
    };
}

/// Generates `pub async fn run()` for SVC-mode hosts (wasm service + gRPC store).
///
/// Arguments: WorldType, StoreClientType, init_accessor, "default-store-addr",
///            "default-listen-addr", "default-wasm-file", "service-label"
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
            };
            let mut store = ::wasmtime::Store::new(&engine, data);

            let component = ::wasmtime::component::Component::from_file(&engine, &wasm_file)?;
            let instance  = <$World>::instantiate_async(&mut store, &component, &linker).await?;
            instance.$accessor().call_init(&mut store).await?;

            println!("{} [svc] listening on {listen_addr}", $label);

            crate::grpc::serve(Arc::new(Mutex::new(store)), Arc::new(instance), listen_addr).await
        }
    };
}

/// Generates `pub async fn run()` for STORE-mode hosts (wasm store + gRPC server).
/// Requires `define_store_service!` and the gRPC trait impl to appear before this call.
///
/// Arguments: WorldType, GrpcServerIdent, init_accessor, "db-name", "collection",
///            "default-addr", "default-wasm-file", "service-label"
#[macro_export]
macro_rules! run_store {
    ($World:ty, $GrpcServer:ident, $accessor:ident, $db:literal, $coll:literal,
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

            let mongo      = ::mongodb::Client::with_uri_str(&mongo_uri).await?;
            let collection = Arc::new(mongo.database($db).collection($coll));

            let engine     = $crate::make_engine()?;
            let mut linker: Linker<$crate::StoreData> = Linker::new(&engine);
            ::wasmtime_wasi::add_to_linker_async(&mut linker)?;
            <$World>::add_to_linker(&mut linker, |d| d)?;

            let data = $crate::StoreData {
                wasi:       $crate::make_store_wasi_ctx(&data_dir)?,
                table:      ::wasmtime_wasi::ResourceTable::new(),
                collection,
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
