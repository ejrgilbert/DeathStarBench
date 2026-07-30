BUILD_DIR    := build
TINYGO_FLAGS := -target=wasip2 -scheduler=none
SERVICES     := recommendation attractions geo user rate profile review reservation # append here to add a service

.PHONY: all all-composed frontend-all bootstrap wit-deps generate bench bench-tcp bench-abi \
        hosts-tcp hosts-abi tcp-up tcp-down abi-up abi-down teardown clean

.DEFAULT_GOAL := bootstrap

bootstrap:
	$(MAKE) wit-deps
	$(MAKE) generate
	$(MAKE) all
	$(MAKE) frontend-all

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

$(BUILD_DIR):
	mkdir -p $@

# ── wit-deps ──────────────────────────────────────────────────────────────────
wit-deps:
	$(foreach s,$(SERVICES), \
	  cd components/$(s)_store && wkg wit fetch && cd ../.. ; \
	  cd components/$(s)       && wkg wit fetch && cd ../.. ;)
	cd components/search   && wkg wit fetch && cd ../..
	cd components/frontend && wkg wit fetch && cd ../..

# ── generate ──────────────────────────────────────────────────────────────────
generate:
	$(foreach s,$(SERVICES), \
	  wit-bindgen-go generate --world $(s)-store-world --out ./components/$(s)_store ./components/$(s)_store/wit ; \
	  wit-bindgen-go generate --world $(s)-world       --out ./components/$(s)       ./components/$(s)/wit ;)
	wit-bindgen-go generate --world search-world        --out ./components/search   ./components/search/wit
	wit-bindgen-go generate --world frontend-host-world --out ./components/frontend ./components/frontend/wit

# ── wasm build targets ────────────────────────────────────────────────────────
all: $(BUILD_DIR) $(foreach s,$(SERVICES),$(BUILD_DIR)/$(s)-store.wasm $(BUILD_DIR)/$(s).wasm) $(BUILD_DIR)/search.wasm $(BUILD_DIR)/frontend.wasm
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

# Compose all services with their stores (no in-wasm cache needed)
$(BUILD_DIR)/%-composed.wasm: $(BUILD_DIR)/%.wasm $(BUILD_DIR)/%-store.wasm
	wac plug --plug $(BUILD_DIR)/$*-store.wasm $(BUILD_DIR)/$*.wasm -o $@

# Compose search with geo-composed + rate-composed
$(BUILD_DIR)/search-all.wasm: $(BUILD_DIR)/search.wasm $(BUILD_DIR)/geo-composed.wasm $(BUILD_DIR)/rate-composed.wasm
	wac plug \
	  --plug $(BUILD_DIR)/geo-composed.wasm \
	  --plug $(BUILD_DIR)/rate-composed.wasm \
	  $(BUILD_DIR)/search.wasm -o $@

# Compose frontend with all service-composed wasms
$(BUILD_DIR)/frontend-all.wasm: $(BUILD_DIR)/frontend.wasm \
    $(BUILD_DIR)/recommendation-composed.wasm \
    $(BUILD_DIR)/geo-composed.wasm \
    $(BUILD_DIR)/attractions-composed.wasm \
    $(BUILD_DIR)/user-composed.wasm \
    $(BUILD_DIR)/rate-composed.wasm \
    $(BUILD_DIR)/profile-composed.wasm \
    $(BUILD_DIR)/review-composed.wasm \
    $(BUILD_DIR)/reservation-composed.wasm \
    $(BUILD_DIR)/search-all.wasm
	wac plug \
	  --plug $(BUILD_DIR)/recommendation-composed.wasm \
	  --plug $(BUILD_DIR)/geo-composed.wasm \
	  --plug $(BUILD_DIR)/attractions-composed.wasm \
	  --plug $(BUILD_DIR)/user-composed.wasm \
	  --plug $(BUILD_DIR)/rate-composed.wasm \
	  --plug $(BUILD_DIR)/profile-composed.wasm \
	  --plug $(BUILD_DIR)/review-composed.wasm \
	  --plug $(BUILD_DIR)/reservation-composed.wasm \
	  --plug $(BUILD_DIR)/search-all.wasm \
	  $(BUILD_DIR)/frontend.wasm -o $@

frontend-all: $(BUILD_DIR)/frontend-all.wasm

# ── host docker images ────────────────────────────────────────────────────────
# Rebuild the host images only when a source baked into them changes. The stamp
# files let make's timestamp logic gate the (slow) `docker compose build`, so
# run.sh can invoke `make hosts-tcp` / `hosts-abi` before every bring-up and pay
# nothing when nothing changed.
HOST_IMAGE_SRCS := $(shell find hosts host-lib proto components -type f 2>/dev/null) \
                   Cargo.toml Cargo.lock Dockerfile $(wildcard $(BUILD_DIR)/*.wasm)

hosts-tcp: $(BUILD_DIR)/.hosts-tcp.stamp
hosts-abi: $(BUILD_DIR)/.hosts-abi.stamp

$(BUILD_DIR)/.hosts-tcp.stamp: $(HOST_IMAGE_SRCS) | $(BUILD_DIR)
	docker compose -f docker-compose.tcp.yml build
	@touch $@

$(BUILD_DIR)/.hosts-abi.stamp: $(HOST_IMAGE_SRCS) | $(BUILD_DIR)
	docker compose -f docker-compose.abi.yml build
	@touch $@

# ── docker compose ────────────────────────────────────────────────────────────
tcp-up: all
	docker compose -f docker-compose.tcp.yml up --build -d --remove-orphans

tcp-down:
	docker compose -f docker-compose.tcp.yml down -v --remove-orphans

abi-up: all-composed $(BUILD_DIR)/frontend-all.wasm
	docker compose -f docker-compose.abi.yml up --build -d --remove-orphans

abi-down:
	docker compose -f docker-compose.abi.yml down -v --remove-orphans

# ── benchmarks ────────────────────────────────────────────────────────────────
BENCH_N    = 10000
BENCH_C    = 10
BENCH_HOST = localhost
BENCH_PORT = 8080
BENCH_URL  = 'http://$(BENCH_HOST):$(BENCH_PORT)/hotels?lat=37.7867&lon=-122.4112&inDate=2015-04-09&outDate=2015-04-10'

bench:
	@echo "=== MakeReservation ==="
	ab -l -n $(BENCH_N) -c $(BENCH_C) $(BENCH_URL)

teardown:
	-docker compose -f docker-compose.tcp.yml down -v --remove-orphans
	-docker compose -f docker-compose.abi.yml down -v --remove-orphans

bench-tcp: teardown tcp-up
	@until nc -z $(BENCH_HOST) $(BENCH_PORT); do sleep 1; done
	@sleep 5
	$(MAKE) bench tcp-down

bench-abi: teardown abi-up
	@until nc -z $(BENCH_HOST) $(BENCH_PORT); do sleep 1; done
	@sleep 5
	$(MAKE) bench abi-down

clean:
	rm -rf $(BUILD_DIR)
