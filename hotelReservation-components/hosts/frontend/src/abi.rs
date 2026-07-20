use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use wasmtime::component::{Component, Linker};
use wasmtime::Store;
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

wasmtime::component::bindgen!({
    world: "frontend-all-host-world",
    path: "../../components/frontend/wit",
    with: {
        "host:storage/collection/connection": host_lib::MongoCollection,
        "wasi:http/types@0.2.0":              wasmtime_wasi_http::bindings::http::types,
        "wasi:io/poll@0.2.0":                 wasmtime_wasi::bindings::io::poll,
        "wasi:io/error@0.2.0":                wasmtime_wasi::bindings::io::error,
        "wasi:io/streams@0.2.0":              wasmtime_wasi::bindings::io::streams,
        "wasi:clocks/monotonic-clock@0.2.0":  wasmtime_wasi::bindings::clocks::monotonic_clock,
    },
    async: true,
});

// Per-collection DB routing: each entry maps a MongoDB collection name to the
// database that owns it.  Built from per-service MONGO_URI_* env vars in run().
type DbMap = Arc<HashMap<String, Arc<mongodb::Database>>>;

struct AbiHostData {
    wasi:  WasiCtx,
    table: ResourceTable,
    http:  WasiHttpCtx,
    dbs:   DbMap,
    cache: Arc<host_lib::Cache>,
}

impl wasmtime_wasi::WasiView for AbiHostData {
    fn ctx(&mut self)   -> &mut WasiCtx      { &mut self.wasi  }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

impl WasiHttpView for AbiHostData {
    fn ctx(&mut self)   -> &mut WasiHttpCtx  { &mut self.http  }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

// Implement collection host directly so we can route by collection name.
#[async_trait::async_trait]
impl host::storage::collection::HostConnection for AbiHostData {
    async fn open(
        &mut self,
        name: String,
    ) -> wasmtime::component::Resource<host_lib::MongoCollection> {
        let db = self.dbs.get(&name)
            .unwrap_or_else(|| panic!("no MongoDB configured for collection '{name}'"));
        let col = db.collection::<mongodb::bson::Document>(&name);
        let mc = host_lib::MongoCollection { inner: Arc::new(col) };
        self.table().push(mc).expect("resource table push")
    }

    async fn drop(
        &mut self,
        rep: wasmtime::component::Resource<host_lib::MongoCollection>,
    ) -> anyhow::Result<()> {
        self.table().delete(rep)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl host::storage::collection::Host for AbiHostData {
    async fn count(
        &mut self,
        c: wasmtime::component::Resource<host_lib::MongoCollection>,
    ) -> u64 {
        let col = self.table().get(&c).unwrap().inner.clone();
        host_lib::mongo_count(&col).await.unwrap_or(0)
    }

    async fn find_all(
        &mut self,
        c: wasmtime::component::Resource<host_lib::MongoCollection>,
    ) -> Vec<Vec<u8>> {
        let col = self.table().get(&c).unwrap().inner.clone();
        host_lib::mongo_find_all(&col).await.unwrap_or_default()
    }

    async fn find_one(
        &mut self,
        _c: wasmtime::component::Resource<host_lib::MongoCollection>,
        _filter: Vec<u8>,
    ) -> Option<Vec<u8>> {
        unimplemented!()
    }

    async fn find(
        &mut self,
        _c: wasmtime::component::Resource<host_lib::MongoCollection>,
        _filter: Vec<u8>,
    ) -> Vec<Vec<u8>> {
        unimplemented!()
    }

    async fn insert_one(
        &mut self,
        c: wasmtime::component::Resource<host_lib::MongoCollection>,
        doc: Vec<u8>,
    ) {
        let col = self.table().get(&c).unwrap().inner.clone();
        host_lib::mongo_insert_one(&col, doc).await.unwrap()
    }

    async fn insert_many(
        &mut self,
        c: wasmtime::component::Resource<host_lib::MongoCollection>,
        docs: Vec<Vec<u8>>,
    ) {
        let col = self.table().get(&c).unwrap().inner.clone();
        host_lib::mongo_insert_many(&col, docs).await.unwrap()
    }
}

host_lib::impl_cache_host!(AbiHostData);

struct ServerState {
    engine: wasmtime::Engine,
    pre:    FrontendAllHostWorldPre<AbiHostData>,
    dbs:    DbMap,
    cache:  Arc<host_lib::Cache>,
}

async fn handle_request(
    state: Arc<ServerState>,
    req:   hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>> {
    let data = AbiHostData {
        wasi:  WasiCtxBuilder::new().inherit_stdout().inherit_stderr().build(),
        table: ResourceTable::new(),
        http:  WasiHttpCtx::new(),
        dbs:   state.dbs.clone(),
        cache: state.cache.clone(),
    };
    let mut store = Store::new(&state.engine, data);

    let instance = state.pre.instantiate_async(&mut store).await?;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let scheme   = wasmtime_wasi_http::bindings::http::types::Scheme::Http;
    let incoming = store.data_mut().new_incoming_request(scheme, req)?;
    let outparam = store.data_mut().new_response_outparam(sender)?;

    instance
        .wasi_http_incoming_handler()
        .call_handle(&mut store, incoming, outparam)
        .await?;

    match receiver.await? {
        Ok(resp) => Ok(resp),
        Err(e)   => Err(anyhow::anyhow!("wasi:http error code: {e:?}")),
    }
}

pub async fn run() -> Result<()> {
    fn env_or(k: &str, d: &str) -> String {
        std::env::var(k).unwrap_or_else(|_| d.to_string())
    }

    let wasm_file   = env_or("WASM_FILE",   "frontend-all.wasm");
    let listen_addr: SocketAddr = env_or("LISTEN_ADDR", "0.0.0.0:8080").parse()?;

    // Build per-collection DB routing.
    // Each MONGO_URI_<SERVICE> env var points to the MongoDB for that service.
    // Falls back to MONGO_URI / MONGO_DB if per-service URIs are not set
    // (backward-compatible with the old single-MongoDB setup).
    let fallback_uri = env_or("MONGO_URI", "mongodb://localhost:27017");
    let fallback_db  = env_or("MONGO_DB",  "hotel");

    // (collection_name, uri_env_var, db_name)
    let specs: &[(&str, &str, &str)] = &[
        ("attractions", "MONGO_URI_ATTRACTIONS",    "attractions-db"),
        ("geo",         "MONGO_URI_GEO",            "geo-db"),
        ("profiles",    "MONGO_URI_PROFILE",        "profile-db"),
        ("rates",       "MONGO_URI_RATE",            "rate-db"),
        ("recs",        "MONGO_URI_RECOMMENDATION", "recommendation-db"),
        ("number",      "MONGO_URI_RESERVATION",    "reservation-db"),
        ("reservation", "MONGO_URI_RESERVATION",    "reservation-db"),
        ("reviews",     "MONGO_URI_REVIEW",         "review-db"),
        ("users",       "MONGO_URI_USER",           "user-db"),
    ];

    // Deduplicate MongoDB clients by URI (connection pool reuse).
    let mut uri_to_client: HashMap<String, Arc<mongodb::Client>> = HashMap::new();
    let mut dbs: HashMap<String, Arc<mongodb::Database>> = HashMap::new();

    for (col, uri_env, db_name) in specs {
        let uri = std::env::var(uri_env).unwrap_or_else(|_| fallback_uri.clone());
        let db_name = if std::env::var(uri_env).is_ok() {
            db_name.to_string()
        } else {
            fallback_db.clone()
        };
        let client = if let Some(c) = uri_to_client.get(&uri) {
            c.clone()
        } else {
            let c = Arc::new(mongodb::Client::with_uri_str(&uri).await?);
            uri_to_client.insert(uri.clone(), c.clone());
            c
        };
        dbs.insert(col.to_string(), Arc::new(client.database(&db_name)));
    }

    let dbs   = Arc::new(dbs);
    let cache = Arc::new(host_lib::Cache::new(std::collections::HashMap::new()));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<AbiHostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;
    host::storage::collection::add_to_linker(&mut linker, |d| d)?;
    host::cache::keyvalue::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = FrontendAllHostWorldPre::new(linker.instantiate_pre(&component)?)?;

    let state = Arc::new(ServerState { engine, pre, dbs, cache });

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    println!("frontend-all-host [abi] listening on {listen_addr}");

    loop {
        let (tcp, _) = listener.accept().await?;
        let io = TokioIo::new(tcp);
        let state = state.clone();

        tokio::task::spawn(async move {
            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |req| {
                        let state = state.clone();
                        async move {
                            handle_request(state, req).await.inspect_err(|e| {
                                eprintln!("request error: {e:#}");
                            })
                        }
                    }),
                )
                .await
            {
                eprintln!("connection error: {e}");
            }
        });
    }
}
