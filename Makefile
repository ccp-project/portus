all: build test lint algs

build:
	cargo +nightly build --all

OS := $(shell uname)
test: build
	cargo +nightly test --all
ifeq ($(OS), Linux)
	sudo ./target/debug/nltest
	sudo ./target/debug/kptest
else
endif
	make libccp-integration

lint:
	cargo +nightly clippy

cargo_bench: build test
	cargo +nightly bench --all

ipc_latency: build
	sudo ./target/debug/ipc_latency -i 10

bench: cargo_bench ipc_latency

algs: generic

generic:
	cd ccp_generic_cong_avoid && cargo build
	cd ccp_generic_cong_avoid && cargo +nightly clippy

clean:
	cargo clean
	cd ccp_generic_cong_avoid && cargo clean
	cd ccp_integration_test && cargo clean

integration-test:
	python integration-tests/compare.py reference-trace

libccp-integration:
	cd ccp_integration_test && cargo build
	cd ccp_integration_test/libccp && make clean && make integration-test
	export LD_LIBRARY_PATH=ccp_integration_test/libccp && ccp_integration_test/target/debug/integration_test ccp_integration_test/libccp/integration-test
