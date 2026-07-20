IMAGE := repro-gc-bug

.PHONY: all run run-small

all: run

# Expected: check FAIL, check-fixed PASS. Exit code 1 = repro confirmed.
run:
	docker build -t $(IMAGE) . && docker run --rm $(IMAGE) || true

# n=1: no GC pressure, both exports pass (verifies correctness).
run-small:
	docker build -t $(IMAGE) . && docker run --rm $(IMAGE) 1 || true
