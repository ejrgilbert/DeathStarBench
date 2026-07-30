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

## Two things to know

- **One wasm instance per request** (wasi:http model): each request builds a
  fresh `Store` + instance and tears it down after.
- **Pooling allocator** (`host_lib::make_engine`): per-request instantiation
  cost scales with what you instantiate, and the ABI host instantiates the whole
  ~18 MB app graph every request. The pooling allocator pre-reserves and reuses
  warm instance/memory/table slots, making re-instantiation ~O(pages touched)
  instead of O(app size) — without it the no-IPC path measures *slower* than TCP.
  Pool size is env-tunable (`POOL_*`); defaults are sized for `frontend-all.wasm`.
