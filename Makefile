all: build test-portus test-ipc lint algs

travis: build test-portus libccp-integration



OS := $(shell uname)

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
	cargo +nightly clippy

cargo_bench: build test
	cargo +nightly bench --all

ipc_latency: build
	sudo ./target/debug/ipc_latency -i 10

bench: cargo_bench ipc_latency

algs: generic

generic:
	cd ccp_generic_cong_avoid && cargo +nightly build
	cd ccp_generic_cong_avoid && cargo +nightly clippy

clean:
	cargo clean
	cd ccp_generic_cong_avoid && cargo clean
	cd integration_tests/libccp_integration && make clean
	cd integration_tests/libccp_integration && cargo clean

integration-test:
	python integration_tests/algorithms/compare.py reference-trace

libccp-integration:
	cd integration_tests/libccp_integration && cargo +nightly build
	cd integration_tests/libccp_integration && make
ifeq ($(OS), Linux)
	cd integration_tests/libccp_integration && export LD_LIBRARY_PATH=$(shell pwd)/integration_tests/libccp_integration && ./target/debug/test_ccp ./test_datapath
endif
ifeq ($(OS), Darwin)
	cd integration_tests/libccp_integration && export DYLD_LIBRARY_PATH=$(shell pwd) && ./target/debug/test_ccp ./test_datapath
endif
