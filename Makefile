.PHONY: fmt-check fmt-fix build test clean clippy coverage coverage-html coverage-open coverage-ci install-coverage-tools

# Development
build: fmt-fix test
	cargo build

# Formatting
fmt-check:
	cargo +nightly fmt --all -- --check

fmt-fix:
	cargo +nightly fmt --all

test:
	cargo test

clippy:
	cargo clippy -- -D warnings

# CI workflow
ci: fmt-check clippy test build

clean:
	cargo clean

# Coverage tools
install-coverage-tools:
	@echo "Checking for cargo-llvm-cov..."
	@cargo llvm-cov --version > /dev/null 2>&1 || \
		(echo "Installing cargo-llvm-cov..." && cargo install cargo-llvm-cov)

# Coverage targets
coverage: install-coverage-tools
	cargo llvm-cov --summary-only

coverage-html: install-coverage-tools
	cargo llvm-cov --html
	@echo "Coverage report generated in target/llvm-cov/html/index.html"
	@echo "Run 'make coverage-open' to view in browser"

coverage-open: coverage-html
	cargo llvm-cov --open

coverage-ci: install-coverage-tools
	cargo llvm-cov --lcov --output-path lcov.info
