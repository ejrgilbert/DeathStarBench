use anyhow::Result;
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

struct AbiHostData {
    wasi:  WasiCtx,
    table: ResourceTable,
    http:  WasiHttpCtx,
    db:    Arc<mongodb::Database>,
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

host_lib::impl_collection_host!(AbiHostData);
host_lib::impl_cache_host!(AbiHostData);

struct ServerState {
    engine: wasmtime::Engine,
    pre:    FrontendAllHostWorldPre<AbiHostData>,
    db:     Arc<mongodb::Database>,
    cache:  Arc<host_lib::Cache>,
}

async fn handle_request(
    state: Arc<ServerState>,
    req:   hyper::Request<hyper::body::Incoming>,
) -> Result<hyper::Response<wasmtime_wasi_http::body::HyperOutgoingBody>> {
    let data = AbiHostData {
        wasi:  WasiCtxBuilder::new().inherit_stderr().build(),
        table: ResourceTable::new(),
        http:  WasiHttpCtx::new(),
        db:    state.db.clone(),
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

    let mongo_uri   = env_or("MONGO_URI",   "mongodb://localhost:27017");
    let mongo_db    = env_or("MONGO_DB",    "hotel");
    let wasm_file   = env_or("WASM_FILE",   "frontend-all.wasm");
    let listen_addr: SocketAddr = env_or("LISTEN_ADDR", "0.0.0.0:8080").parse()?;

    let mongo = mongodb::Client::with_uri_str(&mongo_uri).await?;
    let db    = Arc::new(mongo.database(&mongo_db));
    let cache = Arc::new(host_lib::Cache::new(std::collections::HashMap::new()));

    let engine = host_lib::make_engine()?;
    let mut linker: Linker<AbiHostData> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    wasmtime_wasi_http::add_only_http_to_linker_async(&mut linker)?;
    host::storage::collection::add_to_linker(&mut linker, |d| d)?;
    host::cache::keyvalue::add_to_linker(&mut linker, |d| d)?;

    let component = Component::from_file(&engine, &wasm_file)?;
    let pre = FrontendAllHostWorldPre::new(linker.instantiate_pre(&component)?)?;

    let state = Arc::new(ServerState { engine, pre, db, cache });

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
                        async move { handle_request(state, req).await }
                    }),
                )
                .await
            {
                eprintln!("connection error: {e}");
            }
        });
    }
}
