all: build test-portus test-ipc lint

travis: build test-portus

OS := $(shell uname)
CLIPPY := $(shell rustup component list | grep "clippy" | grep -c "installed")

build:
	cargo build --all

test-portus: build
	cargo test --all

test-ipc: build
ifeq ($(OS), Linux)
	sudo ./target/debug/ipc_latency -i 10 --impl unix
	sudo ./target/debug/ipc_latency -i 10 --impl nl
	sudo ./target/debug/ipc_latency -i 10 --impl kp
endif

lint:
ifeq ($(CLIPPY), 1)
	cargo clippy
else
	$(warning clippy not installed, skipping...)
endif

ipc_latency: build
	sudo ./target/debug/ipc_latency -i 10

bench: cargo_bench ipc_latency

clean:
	cargo clean
	$(MAKE) -C src/ipc/test-char-dev/ccp-kernel clean

integration-test:
	python integration_tests/algorithms/compare.py reference-trace

.PHONY: bindings python

bindings: python

python:
	$(MAKE) -C python/
