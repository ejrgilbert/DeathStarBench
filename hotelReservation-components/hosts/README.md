# Hosts

Small Rust binaries that embed **wasmtime** and run the componentized
hotelReservation services. The host is the native shell: it supplies the imports
the wasm needs (MongoDB storage, a cache, an HTTP/gRPC front door) and drives the
components in `../components`. Used to compare three deployment topologies at
equal service logic, so transport is the only variable.

## Topologies

- **ABI** (`frontend`, `MODE=abi`) — runs the fully composed `frontend-all.wasm`;
  cross-service calls are direct component-model (ABI) calls, in-process, no IPC.
- **TCP** (`frontend` `MODE=tcp` + one host per service) — runs one service
  component each; cross-service calls go over gRPC/TCP between host processes.

## Layout

- `frontend/` — edge host. `src/tcp.rs` (gRPC to backends) and `src/abi.rs`
  (composed, talks to MongoDB directly).
- one dir per backend service (`geo/`, `profile/`, …) — built from
  `host_lib::store_svc_main!()`, each with a `store` mode (wasm + MongoDB) and a
  `svc` mode (wasm + gRPC client to a store).
- `../host-lib/` — shared engine setup, WASI/Mongo helpers, and the macros that
  keep every host thin.

## Runtime configuration

Three settings make the no-IPC (ABI) path fast and stable — without them it runs
*slower* than TCP or crashes under load:

- **Warm instance pool** (`host_lib::InstancePool`, `POOL_SIZE` default 64) —
  reuse instances instead of re-instantiating the ~18 MB app graph per request.
  Body is drained before return so a concurrent request can't re-enter a
  streaming store and trap.
- **Pooling allocator** (`host_lib::make_engine`) — warm instances come from
  fixed, recycled memory/table arenas; makes reuse cheap and bounds host memory.
- **Per-request GC** (`internal/gcutil` → `gcutil.Tick()`) — each TinyGo guest
  calls `runtime.GC()` at handler end. TinyGo scans the stack conservatively, so
  this is the only safe cadence to reclaim cross-boundary buffers; batching frees
  a not-yet-rooted buffer mid-call (see the `gcutil.go` doc comment).
