FROM rust:1.94-slim AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY host-lib/  host-lib/
COPY hosts/     hosts/
COPY proto/     proto/
COPY components/ components/

RUN cargo build --release


FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
ARG SERVICE
COPY --from=builder /build/target/release/${SERVICE}-host /usr/local/bin/${SERVICE}-host
COPY build/ /app/
WORKDIR /app
ENV SERVICE=${SERVICE}
ENTRYPOINT ["sh", "-c", "exec ${SERVICE}-host"]
