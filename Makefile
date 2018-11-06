all: build test-portus test-ipc libccp-integration lint

travis: build test-portus libccp-integration

OS := $(shell uname)
CLIPPY := $(shell rustup component list --toolchain nightly | grep "clippy" | grep -c "installed")

build:
	cargo +nightly build --all

test-portus: build
	cargo +nightly test --all

test-ipc: build
ifeq ($(OS), Linux)
	sudo ./target/debug/nltest
	sudo ./target/debug/kptest
endif

lint:
ifeq ($(CLIPPY), 1)
	cargo +nightly clippy
else
	$(warning clippy not installed, skipping...)
endif

cargo_bench: build test
	cargo +nightly bench --all

ipc_latency: build
	sudo ./target/debug/ipc_latency -i 10

bench: cargo_bench ipc_latency

clean:
	cargo clean
	$(MAKE) -C integration_tests/libccp_integration/libccp clean
	cd integration_tests/libccp_integration && cargo clean
	$(MAKE) -C src/ipc/test-char-dev/ccp-kernel clean

integration-test:
	python integration_tests/algorithms/compare.py reference-trace

integration_tests/libccp_integration/libccp/ccp.h:
	$(error Did you forget to git submodule update --init --recursive ?)

libccp-integration: integration_tests/libccp_integration/libccp/ccp.h
ifeq ($(OS), Linux)
	cd integration_tests/libccp_integration && export LD_LIBRARY_PATH=./libccp && cargo +nightly test -- --test-threads=1
endif
ifeq ($(OS), Darwin)
	cd integration_tests/libccp_integration && export DYLD_LIBRARY_PATH=./libccp && cargo +nightly test -- --test-threads=1
endif

.PHONY: bindings python

bindings: python

python:
	$(MAKE) -C python/
