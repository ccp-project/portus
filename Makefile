all: build test 

build:
	cargo +nightly build --all

OS := $(shell uname)
test: build
	cargo +nightly test --features bench --all
ifeq ($(OS), Linux)
	sudo ./target/debug/nltest
else
endif

cargo_bench: build test
	cargo +nightly bench --features bench --all

ipc_latency: build
	sudo ./target/debug/ipc_latency

bench: cargo_bench ipc_latency

clean:
	cargo clean
