all: build test 

build:
	cargo build

test: build
	cargo +nightly test --features bench

bench: build test
	cargo +nightly bench --features bench

clean:
	rm -rf ./target
