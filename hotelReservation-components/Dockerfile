# Build stage: compiles the entire Rust workspace.
FROM rust:1.92.0-slim AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY host-lib/ host-lib/
COPY hosts/ hosts/
COPY proto/ proto/
COPY components/recommendation_store/wit/ components/recommendation_store/wit/
COPY components/recommendation/wit/ components/recommendation/wit/
COPY components/attractions_store/wit/ components/attractions_store/wit/
COPY components/attractions/wit/ components/attractions/wit/

RUN cargo build --release --locked


# recommendation image
FROM debian:bookworm-slim AS recommendation
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/recommendation-host /usr/local/bin/
COPY recommendation-store.wasm recommendation.wasm recommendation-composed.wasm /app/
WORKDIR /app
CMD ["recommendation-host"]

# attractions image
FROM debian:bookworm-slim AS attractions
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/attractions-host /usr/local/bin/
COPY attractions-store.wasm attractions.wasm attractions-composed.wasm /app/
WORKDIR /app
CMD ["attractions-host"]
