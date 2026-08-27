IMAGE := repro-gc-bug

# Path to the tinygo binary to test with. Defaults to the from-source build in
# this repo (../build/tinygo), which compiles the runtime from ../src — so it
# picks up the GC fix. Override with `make local TINYGO=tinygo` to test the
# installed release instead.
TINYGO      ?= /Users/evgilber/git/tinygo/build/tinygo
REPRO_WASM  := repro.local.wasm
HOST        := host/target/release/repro-host

.PHONY: all run run-small local local-small docker-local clean

all: run

# Expected: check FAIL, check-fixed PASS. Exit code 1 = repro confirmed.
run:
	docker build -t $(IMAGE) . && docker run --rm $(IMAGE) || true

# n=1: no GC pressure, both exports pass (verifies correctness).
run-small:
	docker build -t $(IMAGE) . && docker run --rm $(IMAGE) 1 || true

# Build the guest with $(TINYGO) and run it against the local host. With a fixed
# runtime, check PASSes; with a buggy one it FAILs. Needs wasm-opt (binaryen) on
# PATH and the WASI wit deps under wit/deps (see the wit/deps target).
local: wit/deps $(HOST)
	$(TINYGO) build -target=wasip2 -scheduler=none \
	  -wit-package ./wit -wit-world repro-world -o $(REPRO_WASM) .
	./$(HOST) $(REPRO_WASM) 10000

# n=1 sanity run of the locally-built guest.
local-small: wit/deps $(HOST)
	$(TINYGO) build -target=wasip2 -scheduler=none \
	  -wit-package ./wit -wit-world repro-world -o $(REPRO_WASM) .
	./$(HOST) $(REPRO_WASM) 1

# Build the guest with $(TINYGO) on the host, then run it in a container that
# only builds the Rust host. Uses the portable .wasm because the host tinygo
# binary can't run in the (Linux) container.
docker-local: wit/deps
	$(TINYGO) build -target=wasip2 -scheduler=none \
	  -wit-package ./wit -wit-world repro-world -o repro.local.wasm .
	docker build -f Dockerfile.local -t $(IMAGE)-local .
	docker run --rm $(IMAGE)-local || true

# Fetch the WASI wit dependencies referenced by wit/bug.wit.
wit/deps:
	wkg wit fetch --wit-dir wit

$(HOST):
	cargo build --release --manifest-path host/Cargo.toml

clean:
	rm -f $(REPRO_WASM)
