.PHONY: all build build-release test lint fmt fmt-check check ci doc clean setup

# Default target
all: build test

# Build all workspace crates
build:
	cargo build --workspace

# Build in release mode
build-release:
	cargo build --workspace --release

# Run all tests
test:
	cargo test --workspace

# Run tests with output
test-verbose:
	cargo test --workspace -- --nocapture

# Run clippy lints
lint:
	cargo clippy --workspace --all-targets -- -D warnings

# Format code
fmt:
	cargo fmt --all

# Check formatting
fmt-check:
	cargo fmt --all -- --check

# Check all targets
check:
	cargo check --workspace --all-targets

# Run all CI checks
ci: fmt-check lint test
	@echo "All CI checks passed!"

# Generate documentation
doc:
	cargo doc --workspace --no-deps

# Clean build artifacts
clean:
	cargo clean

# Install dev dependencies
setup:
	rustup component add rustfmt clippy

# Build TypeScript packages
ts-build:
	cd ts/proto && npm install && npm run build

# Test TypeScript packages
ts-test:
	cd ts/proto && npm test

# Run all tests including TypeScript
test-all: test ts-test

# Daemons
run-gossipd:
	cargo run -p gossipd

run-routerd:
	cargo run -p routerd

run-prober:
	cargo run -p prober

run-infernode:
	cargo run -p infernode
