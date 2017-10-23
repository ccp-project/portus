all: build test 

build:
	cargo build --all

test: build
	cargo +nightly test --features bench --all
	sudo ./target/debug/nltest

cargo_bench: build test
	cargo +nightly bench --features bench --all

ipc_latency: build
	sudo ./target/debug/ipc_latency

bench: cargo_bench ipc_latency

clean:
	rm -rf ./target
