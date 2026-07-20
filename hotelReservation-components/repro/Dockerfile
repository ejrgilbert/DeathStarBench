# syntax=docker/dockerfile:1
# ── builder ──────────────────────────────────────────────────────────────────
FROM golang:1.25-bookworm AS builder

# TinyGo (tarball includes LLVM — no separate LLVM install needed)
ARG TINYGO_VERSION=0.41.1
ARG TARGETARCH
RUN set -eux; \
    curl -fsSL "https://github.com/tinygo-org/tinygo/releases/download/v${TINYGO_VERSION}/tinygo${TINYGO_VERSION}.linux-${TARGETARCH}.tar.gz" \
      | tar -C /usr/local -xzf -; \
    ln -s /usr/local/tinygo/bin/tinygo /usr/local/bin/tinygo

# wit-bindgen-go
RUN go install go.bytecodealliance.org/cmd/wit-bindgen-go@v0.7.0

# Rust, wasm-tools (called by TinyGo's -wit-package to embed the component)
ARG RUST_VERSION=1.97.0
ARG WASM_TOOLS_VERSION=1.247.0
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION}
ENV PATH="/root/.cargo/bin:${PATH}"
RUN cargo install wasm-tools --version ${WASM_TOOLS_VERSION} --locked

WORKDIR /build

# ── cache Rust deps with a stub binary ───────────────────────────────────────
COPY host/Cargo.toml host/Cargo.lock host/
RUN mkdir host/src && echo 'fn main(){}' > host/src/main.rs \
 && cargo build --release --manifest-path host/Cargo.toml \
 && rm -rf host/target/release/.fingerprint/repro-host-*

# ── cache Go module deps ──────────────────────────────────────────────────────
COPY go.mod go.sum ./
RUN go mod download

# ── generate bindings, build ─────────────────────────────────────────────────
COPY . .
RUN wit-bindgen-go generate --world repro-world --out . ./wit
RUN tinygo build -target=wasip2 -scheduler=none \
      -wit-package ./wit -wit-world repro-world \
      -o repro.wasm .
RUN touch host/src/main.rs \
 && cargo build --release --manifest-path host/Cargo.toml

# ── runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim
COPY --from=builder /build/repro.wasm      /repro/repro.wasm
COPY --from=builder /build/host/target/release/repro-host /repro/repro-host
ENTRYPOINT ["/repro/repro-host", "/repro/repro.wasm"]
