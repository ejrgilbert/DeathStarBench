BUILD_DIR    := build
TINYGO_FLAGS := -target=wasip2 -scheduler=none
SERVICES     := recommendation attractions geo user  # append here to add a service

BENCH_N    = 1000
BENCH_C    = 10
BENCH_HOST = localhost
BENCH_PORT = 8085
BENCH_ADDR = $(BENCH_HOST):$(BENCH_PORT)

.PHONY: all all-composed wit-deps generate bench bench-tcp bench-abi \
        tcp-up tcp-down abi-up abi-down teardown clean

$(BUILD_DIR):
	mkdir -p $@

# ── wit-deps ──────────────────────────────────────────────────────────────────
wit-deps:
	$(foreach s,$(SERVICES), \
	  cd components/$(s)_store && wkg wit fetch && cd ../.. ; \
	  cd components/$(s)       && wkg wit fetch && cd ../.. ;)

# ── generate ──────────────────────────────────────────────────────────────────
generate:
	$(foreach s,$(SERVICES), \
	  wit-bindgen-go generate --world $(s)-store-world --out ./components/$(s)_store ./components/$(s)_store/wit ; \
	  wit-bindgen-go generate --world $(s)-world       --out ./components/$(s)       ./components/$(s)/wit ;)

# ── wasm build targets ────────────────────────────────────────────────────────
all: $(BUILD_DIR) $(foreach s,$(SERVICES),$(BUILD_DIR)/$(s)-store.wasm $(BUILD_DIR)/$(s).wasm)
all-composed: $(BUILD_DIR) $(foreach s,$(SERVICES),$(BUILD_DIR)/$(s)-composed.wasm)

$(BUILD_DIR)/%-store.wasm: components/%_store | $(BUILD_DIR)
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/$*_store/wit \
	  -wit-world $*-store-world \
	  -o $@ ./components/$*_store/

$(BUILD_DIR)/%.wasm: components/% | $(BUILD_DIR)
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/$*/wit \
	  -wit-world $*-world \
	  -o $@ ./components/$*/

$(BUILD_DIR)/%-composed.wasm: $(BUILD_DIR)/%.wasm $(BUILD_DIR)/%-store.wasm
	wac plug --plug $(BUILD_DIR)/$*-store.wasm $(BUILD_DIR)/$*.wasm -o $@

# ── docker compose ────────────────────────────────────────────────────────────
tcp-up: all
	docker compose -f docker-compose.tcp.yml up --build -d --remove-orphans

tcp-down:
	docker compose -f docker-compose.tcp.yml down -v --remove-orphans

abi-up: all-composed
	docker compose -f docker-compose.abi.yml up --build -d --remove-orphans

abi-down:
	docker compose -f docker-compose.abi.yml down -v --remove-orphans

# ── benchmarks ────────────────────────────────────────────────────────────────
bench:
	@echo "=== distance ==="
	ghz --insecure --proto proto/recommendation.proto \
	  --call recommendation.Recommendation.GetRecommendations \
	  -d '{"require":"dis","lat":37.7749,"lon":-122.4194}' \
	  -n $(BENCH_N) -c $(BENCH_C) $(BENCH_ADDR)
	@echo "=== rate ==="
	ghz --insecure --proto proto/recommendation.proto \
	  --call recommendation.Recommendation.GetRecommendations \
	  -d '{"require":"rate","lat":37.7749,"lon":-122.4194}' \
	  -n $(BENCH_N) -c $(BENCH_C) $(BENCH_ADDR)
	@echo "=== price ==="
	ghz --insecure --proto proto/recommendation.proto \
	  --call recommendation.Recommendation.GetRecommendations \
	  -d '{"require":"price","lat":37.7749,"lon":-122.4194}' \
	  -n $(BENCH_N) -c $(BENCH_C) $(BENCH_ADDR)

teardown:
	-docker compose -f docker-compose.tcp.yml down -v --remove-orphans
	-docker compose -f docker-compose.abi.yml down -v --remove-orphans

bench-tcp: teardown tcp-up
	@until nc -z $(BENCH_HOST) $(BENCH_PORT); do sleep 1; done
	$(MAKE) bench tcp-down

bench-abi: teardown abi-up
	@until nc -z $(BENCH_HOST) $(BENCH_PORT); do sleep 1; done
	$(MAKE) bench abi-down

clean:
	rm -rf $(BUILD_DIR)
