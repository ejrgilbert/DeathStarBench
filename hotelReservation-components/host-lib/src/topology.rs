//! Topology capture for the equivalence harness (NOT the perf sweeps).
//!
//! When `EQUIV_TOPOLOGY=1`, a tower layer counts every gRPC request a host
//! serves, keyed by the request path (the full method, e.g. `/geo.Geo/Nearby`),
//! and a background task flushes the running totals to `/telemetry/rpc-<host>.json`
//! every 100ms. The harness reads these files before/after each replayed request
//! group (via `docker compose exec cat`); the delta is that group's RPC multiset.
//!
//! The cache host additionally counts get/set operations (see `record_cache`)
//! and flushes them to `/telemetry/cache-<host>.json`, the memcached-`stats`
//! analog for the original. All gated so it never affects the perf-measurement
//! paths.
//!
//! Mirrors the original stack's `inlangmw/topology.go`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::task::{Context, Poll};

static COUNTS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

fn counts() -> &'static Mutex<HashMap<String, u64>> {
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Cache-operation counts for the memcached-equivalent cache host, keyed by op
/// (`get`/`set`). `get` counts per key (a GetMulti of N keys adds N) to match
/// memcached's per-key `cmd_get` accounting so the two stacks reconcile. The
/// cache host is deliberately NOT given the RPC `TopologyLayer` (its gRPC
/// surface has no counterpart in the memcached-backed original), so cache
/// access is captured here and kept out of the service->service RPC multiset.
static CACHE_COUNTS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

fn cache_counts() -> &'static Mutex<HashMap<String, u64>> {
    CACHE_COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records `n` cache operations of kind `op` (`get`/`set`). Call from the cache
/// host's gRPC handlers. No-op unless topology capture is enabled.
pub fn record_cache(op: &str, n: u64) {
    if !enabled() || n == 0 {
        return;
    }
    let mut m = cache_counts().lock().unwrap();
    *m.entry(op.to_string()).or_insert(0) += n;
}

/// Whether topology capture is enabled for this run (cached).
pub fn enabled() -> bool {
    static EN: OnceLock<bool> = OnceLock::new();
    *EN.get_or_init(|| std::env::var("EQUIV_TOPOLOGY").as_deref() == Ok("1"))
}

fn record(path: &str) {
    if !enabled() {
        return;
    }
    let mut m = counts().lock().unwrap();
    *m.entry(path.to_string()).or_insert(0) += 1;
}

/// Spawns the periodic flusher (once, only when enabled). Call from a tokio ctx.
pub fn start_flusher() {
    if !enabled() {
        return;
    }
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(async move {
        let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "host".into());
        let dir = std::path::Path::new("/telemetry");
        let _ = std::fs::create_dir_all(dir);
        loop {
            flush_map(dir, &counts().lock().unwrap(), &format!("rpc-{host}"));
            flush_map(dir, &cache_counts().lock().unwrap(), &format!("cache-{host}"));
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
}

/// Atomically writes a counter map to `<dir>/<stem>.json`.
fn flush_map(dir: &std::path::Path, m: &HashMap<String, u64>, stem: &str) {
    let path = dir.join(format!("{stem}.json"));
    let tmp = dir.join(format!("{stem}.json.tmp"));
    if let Ok(j) = serde_json::to_vec(m) {
        if std::fs::write(&tmp, &j).is_ok() {
            let _ = std::fs::rename(&tmp, &path); // atomic replace
        }
    }
}

/// A tower layer that counts each request by its path (gRPC full method).
#[derive(Clone, Default)]
pub struct TopologyLayer;

impl<S> tower::Layer<S> for TopologyLayer {
    type Service = TopologyService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        TopologyService { inner }
    }
}

#[derive(Clone)]
pub struct TopologyService<S> {
    inner: S,
}

impl<S, ReqBody> tower::Service<http::Request<ReqBody>> for TopologyService<S>
where
    S: tower::Service<http::Request<ReqBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        record(req.uri().path());
        self.inner.call(req)
    }
}
