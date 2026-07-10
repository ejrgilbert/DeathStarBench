# Build stage: compiles the entire Rust workspace.
# Both service images pull from this shared layer.
FROM rust:1.92.0-slim AS builder
WORKDIR /build

# protoc is required by tonic-build (host-lib/build.rs)
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY host-lib/ host-lib/
COPY hosts/ hosts/
COPY proto/ proto/
# WIT files are read at compile time by wasmtime's bindgen! macro
COPY components/recommendation_store/wit/ components/recommendation_store/wit/
COPY components/recommendation/wit/ components/recommendation/wit/

RUN cargo build --release --locked


# recommendation-store: runs the recommendation-store Wasm component,
# owns the MongoDB collection, exposes gRPC on port 8086.
FROM debian:bookworm-slim AS recommendation-store
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/recommendation-store-host /usr/local/bin/
COPY recommendation-store.wasm /app/recommendation-store.wasm
ENV WASM_FILE=/app/recommendation-store.wasm
WORKDIR /app
CMD ["recommendation-store-host"]


# recommendation-tcp: runs the recommendation Wasm component,
# bridges the store import to the recommendation-store gRPC service,
# exposes the same outward-facing gRPC interface as the original binary (port 8085).
FROM debian:bookworm-slim AS recommendation-tcp
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/recommendation-tcp-host /usr/local/bin/
COPY recommendation.wasm /app/recommendation.wasm
ENV WASM_FILE=/app/recommendation.wasm
WORKDIR /app
CMD ["recommendation-tcp-host"]
