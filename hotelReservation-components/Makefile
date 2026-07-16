BUILD_DIR    := build
TINYGO_FLAGS := -target=wasip2 -scheduler=none
SERVICES     := recommendation attractions geo user rate profile review reservation # append here to add a service
CACHE_SERVICES := rate profile review reservation

.PHONY: all all-composed wit-deps generate bench bench-tcp bench-abi \
        tcp-up tcp-down abi-up abi-down teardown clean

$(BUILD_DIR)/frontend.wasm: components/frontend | $(BUILD_DIR)
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/frontend/wit \
	  -wit-world frontend-world \
	  -o $@ ./components/frontend/

$(BUILD_DIR)/search.wasm: components/search | $(BUILD_DIR)
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/search/wit \
	  -wit-world search-world \
	  -o $@ ./components/search/

$(BUILD_DIR)/cache.wasm: components/cache | $(BUILD_DIR)
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/cache/wit \
	  -wit-world cache-world \
	  -o $@ ./components/cache/

$(BUILD_DIR):
	mkdir -p $@

# ── wit-deps ──────────────────────────────────────────────────────────────────
wit-deps:
	$(foreach s,$(SERVICES), \
	  cd components/$(s)_store && wkg wit fetch && cd ../.. ; \
	  cd components/$(s)       && wkg wit fetch && cd ../.. ;)
	cd components/search   && wkg wit fetch && cd ../..
	cd components/frontend && wkg wit fetch && cd ../..
	cd components/cache    && wkg wit fetch && cd ../..

# ── generate ──────────────────────────────────────────────────────────────────
generate:
	$(foreach s,$(SERVICES), \
	  wit-bindgen-go generate --world $(s)-store-world --out ./components/$(s)_store ./components/$(s)_store/wit ; \
	  wit-bindgen-go generate --world $(s)-world       --out ./components/$(s)       ./components/$(s)/wit ;)
	wit-bindgen-go generate --world search-world        --out ./components/search   ./components/search/wit
	wit-bindgen-go generate --world frontend-host-world --out ./components/frontend ./components/frontend/wit
	wit-bindgen-go generate --world cache-world    --out ./components/cache    ./components/cache/wit

# ── wasm build targets ────────────────────────────────────────────────────────
all: $(BUILD_DIR) $(foreach s,$(SERVICES),$(BUILD_DIR)/$(s)-store.wasm $(BUILD_DIR)/$(s).wasm) $(BUILD_DIR)/search.wasm $(BUILD_DIR)/frontend.wasm $(BUILD_DIR)/cache.wasm
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

# Compose services with a cache
define cache-composed-rule
$(BUILD_DIR)/$(1)-composed.wasm: $(BUILD_DIR)/$(1).wasm $(BUILD_DIR)/$(1)-store.wasm $(BUILD_DIR)/cache.wasm
	wac plug --plug $(BUILD_DIR)/$(1)-store.wasm $(BUILD_DIR)/$(1).wasm -o $(BUILD_DIR)/$(1)-with-store.wasm
	wac plug --plug $(BUILD_DIR)/cache.wasm $(BUILD_DIR)/$(1)-with-store.wasm -o $$@
endef
$(foreach s,$(CACHE_SERVICES),$(eval $(call cache-composed-rule,$(s))))

# Compose non-cache services
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
BENCH_N    = 10000
BENCH_C    = 10
BENCH_HOST = localhost
BENCH_PORT = 8100
BENCH_ADDR = $(BENCH_HOST):$(BENCH_PORT)

bench:
	@echo "=== MakeReservation ==="
	ghz --insecure --proto proto/reservation.proto \
	  --call reservation.Reservation/MakeReservation \
      -d '{"customerName":"Elizabeth","hotelId":"1","inDate":"2015-04-09","outDate":"2015-04-10","roomNumber":200}' \
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
