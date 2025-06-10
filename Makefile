.PHONY: fmt-check fmt-fix build test clean clippy

# Development
build: fmt-fix
	cargo build

# Formatting
fmt-check:
	cargo +nightly fmt --all -- --check

fmt-fix:
	cargo +nightly fmt --all

test: build
	cargo test

clippy:
	cargo clippy -- -D warnings

# CI workflow
ci: fmt-check clippy test build

clean:
	cargo clean
