include .bingo/Variables.mk

DOWNLOAD_DIR = /tmp
BIN_DIR = $(shell pwd)/bin

OS := $(shell uname -s | tr '[:upper:]' '[:lower:]' | sed 's/darwin/osx/')
ARCH := $(shell uname -m | sed 's/arm64/aarch_64/')

PROTOC_VERION = 25.0
PROTOC_PACKAGE = protoc-$(PROTOC_VERION)-$(OS)-$(ARCH)
PROTOC_BIN = $(BIN_DIR)/protoc

.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-24s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

.PHONY: proto
proto: protoc-plugins ## Generates proto buffer code
	for f in services/**/proto/*.proto; do \
		$(PROTOC_BIN) --go_out=. --go_opt=paths=source_relative \
			--go-grpc_out=. --go-grpc_opt=paths=source_relative \
			$$f; \
	done

.PHONY: data
data:
	go-bindata -o data/bindata.go -pkg data data/*.json

.PHONY: run
run: ## Runs the application using docker-compose
	docker-compose build
	docker-compose up --remove-orphans

.PHONY: bin
bin: ## Creates bin directory
	mkdir -p $(BIN_DIR)

.PHONY: clean 
clean: ## Removes all binaries in bin directory
	rm -rf $(BIN_DIR)
	rm -f $(GOBIN)/protoc-gen-go

.PHONY: protoc
protoc: bin $(PROTOC_GEN_GO) ## Downloads protoc compiler
	curl -LSs https://github.com/protocolbuffers/protobuf/releases/download/v$(PROTOC_VERION)/$(PROTOC_PACKAGE).zip -o $(DOWNLOAD_DIR)/$(PROTOC_PACKAGE).zip
	unzip -qqo $(DOWNLOAD_DIR)/$(PROTOC_PACKAGE).zip -d $(DOWNLOAD_DIR)/$(PROTOC_PACKAGE)
	mv -f $(DOWNLOAD_DIR)/$(PROTOC_PACKAGE)/bin/protoc $(BIN_DIR)

.PHONY: protoc-plugins
protoc-plugins: protoc $(PROTOC_GEN_GO) $(PROTOC_GEN_GO_GRPC) ## Copies the protoc plugin
	cp -f $(PROTOC_GEN_GO) $(GOBIN)/protoc-gen-go
	cp -f $(PROTOC_GEN_GO_GRPC) $(GOBIN)/protoc-gen-go-grpc

# ── sidecar (Envoy) instrumentation ───────────────────────────────────────────
SIDECAR_DIR := build/sidecar
BUILTIN     ?= otel-bare-metrics
SIDECAR_MAP := search:8082 profile:8081 recommendation:8085 user:8086 \
               reservation:8087 review:8088 attractions:8089 geo:8083 rate:8084

.PHONY: sidecar-configs
sidecar-configs: ## Generate Envoy sidecar configs + config.sidecar.json (BUILTIN=<name>)
	@test -f sidecar/$(BUILTIN).yaml || { echo "no sidecar config: sidecar/$(BUILTIN).yaml" >&2; exit 1; }
	@mkdir -p $(SIDECAR_DIR)/$(BUILTIN)
	@for pair in $(SIDECAR_MAP); do \
	  svc=$${pair%%:*}; port=$${pair##*:}; \
	  sed -e "s/@SVC@/$$svc/g" -e "s/@PORT@/$$port/g" sidecar/$(BUILTIN).yaml > $(SIDECAR_DIR)/$(BUILTIN)/$$svc.yaml; \
	done
	@# Re-point the 9 hotel-service *Address values at envoy-<svc> (bare host:port,
	@# same shape as the baseline addresses). The key alternation matches only the 9
	@# service addresses — not Mongo/Memc/consul/jaeger.
	@sed -E 's/("(Search|Profile|Recommend|User|Reserve|Review|Attractions|Geo|Rate)Address": ")([a-z]+)(:[0-9]+")/\1envoy-\3\4/' config.json > config.sidecar.json
	@echo "wrote envoy configs to $(SIDECAR_DIR)/$(BUILTIN)/ and config.sidecar.json"