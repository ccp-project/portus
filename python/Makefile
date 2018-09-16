.PHONY: test-build
test-build: src/lib.rs
	mkdir -p build
	PYTHONPATH='build/' python setup.py develop --install-dir build/
	rm -rf build/
