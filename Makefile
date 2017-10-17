all: build test 

build:
	cargo build

test: build
	cargo +nightly test --features bench
	sudo ./target/debug/nltest

cargo_bench: build test
	cargo +nightly bench --features bench

ipc_latency: build
	sudo ./target/debug/ipc_latency

bench: cargo_bench ipc_latency

clean:
	rm -rf ./target
