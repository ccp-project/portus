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

lint:
	cargo +nightly clippy

cargo_bench: build test
	cargo +nightly bench --all

ipc_latency: build
	sudo ./target/debug/ipc_latency -i 10

bench: cargo_bench ipc_latency

algs: generic bbr

generic:
	cd ccp_generic_cong_avoid && cargo build
	cd ccp_generic_cong_avoid && cargo +nightly clippy

bbr:
	cd ccp_bbr && cargo build
	cd ccp_bbr && cargo +nightly clippy

clean:
	cargo clean
	cd ccp_reno && cargo clean
	cd ccp_cubic && cargo clean
	cd ccp_bbr && cargo clean
