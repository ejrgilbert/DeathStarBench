# TODO: Clean this up to loop on SERVICES

BUILD_DIR := build

$(BUILD_DIR):
	mkdir -p $@

TINYGO_FLAGS = -target=wasip2 -scheduler=none
SERVICES     = recommendation attractions  # add new service names here

BENCH_N    = 1000        # total requests
BENCH_C    = 10          # concurrent workers
BENCH_HOST = localhost
BENCH_PORT = 8085
BENCH_ADDR = $(BENCH_HOST):$(BENCH_PORT)

.PHONY: all all-composed wit-deps generate bench bench-tcp bench-abi \
        tcp-up tcp-down abi-up abi-down teardown clean

wit-deps:
	cd components/recommendation_store && wkg wit fetch
	cd components/recommendation && wkg wit fetch
	cd components/attractions_store && wkg wit fetch
	cd components/attractions && wkg wit fetch

generate:
	wit-bindgen-go generate --world recommendation-store-world --out ./components/recommendation_store ./components/recommendation_store/wit
	wit-bindgen-go generate --world recommendation-world       --out ./components/recommendation       ./components/recommendation/wit
	wit-bindgen-go generate --world attractions-store-world    --out ./components/attractions_store    ./components/attractions_store/wit
	wit-bindgen-go generate --world attractions-world          --out ./components/attractions          ./components/attractions/wit

all: $(foreach s,$(SERVICES),$(BUILD_DIR)/$(s)-store.wasm $(BUILD_DIR)/$(s).wasm)
all-composed: $(foreach s,$(SERVICES),$(BUILD_DIR)/$(s)-composed.wasm)

# Explicit per-service tinygo builds
$(BUILD_DIR)/recommendation-store.wasm:
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/recommendation_store/wit \
	  -wit-world recommendation-store-world \
	  -o $@ ./components/recommendation_store/

$(BUILD_DIR)/recommendation.wasm:
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/recommendation/wit \
	  -wit-world recommendation-world \
	  -o $@ ./components/recommendation/

$(BUILD_DIR)/attractions-store.wasm:
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/attractions_store/wit \
	  -wit-world attractions-store-world \
	  -o $@ ./components/attractions_store/

$(BUILD_DIR)/attractions.wasm:
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/attractions/wit \
	  -wit-world attractions-world \
	  -o $@ ./components/attractions/

# Generic composition rule — works for any service pair
$(BUILD_DIR)/%-composed.wasm: %.wasm %-store.wasm
	wac plug --plug $*-store.wasm $*.wasm -o $@

tcp-up: all
	docker compose -f docker-compose.tcp.yml up --build -d --remove-orphans

tcp-down:
	docker compose -f docker-compose.tcp.yml down -v --remove-orphans

abi-up: all-composed
	docker compose -f docker-compose.abi.yml up --build -d --remove-orphans

abi-down:
	docker compose -f docker-compose.abi.yml down -v --remove-orphans

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
	rm -f $(foreach s,$(SERVICES),$(s)-store.wasm $(s).wasm $(s)-composed.wasm)
