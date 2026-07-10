TINYGO_FLAGS = -target=wasip2 -scheduler=none

.PHONY: all clean

all: recommendation-store.wasm recommendation.wasm

recommendation-store.wasm:
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/recommendation_store/wit \
	  -wit-world recommendation-store-world \
	  -o $@ ./components/recommendation_store/

recommendation.wasm:
	tinygo build $(TINYGO_FLAGS) \
	  -wit-package ./components/recommendation/wit \
	  -wit-world recommendation-world \
	  -o $@ ./components/recommendation/

# Config 3: compose store into recommendation (eliminates inter-component TCP hop)
recommendation-composed.wasm: recommendation.wasm recommendation-store.wasm
	wac plug --plug recommendation-store.wasm recommendation.wasm -o $@

tcp-up: all
	docker compose -f docker-compose.tcp.yml up --build

tcp-down:
	docker compose -f docker-compose.tcp.yml down -v

clean:
	rm -f recommendation-store.wasm recommendation.wasm recommendation-composed.wasm
