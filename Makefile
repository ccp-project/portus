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
ifeq ($(OS), Linux)
	cd integration_tests/libccp_integration && make clean && make && export LD_LIBRARY_PATH=$(shell pwd) && ./target/debug/test_ccp ./test_datapath
endif
ifeq ($(OS), Darwin)
	cd integration_tests/libccp_integration && make clean && make && export DYLD_LIBRARY_PATH=$(shell pwd) && ./target/debug/test_ccp ./test_datapath
endif
