all: build test 

build:
	cargo build

test: build
	cargo +nightly test --features bench
	sudo ./target/debug/nltest

bench: build test
	cargo +nightly bench --features bench

clean:
	rm -rf ./target
