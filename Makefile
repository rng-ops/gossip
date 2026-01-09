.PHONY: all build build-release test lint fmt fmt-check check ci doc clean setup
.PHONY: docker-build docker-push testnet-up testnet-down testnet-clean testnet-logs
.PHONY: docker-test docker-test-ts docker-lint docker-integration

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

# ============================================================================
# Docker targets
# ============================================================================

# Build all Docker images
docker-build:
	docker compose build

# Build specific Docker image (usage: make docker-build-image TARGET=gossipd)
docker-build-image:
	docker build --target $(TARGET) -t terraingossip/$(TARGET):latest .

# Push Docker images to registry
docker-push:
	docker compose push

# Start local test network
testnet-up:
	docker compose up -d

# Start full network with multiple probers
testnet-up-full:
	docker compose --profile scaled up -d

# Stop local test network
testnet-down:
	docker compose down

# Stop and clean up volumes
testnet-clean:
	docker compose down -v

# View logs (usage: make testnet-logs ARGS="-f gossipd")
testnet-logs:
	docker compose logs $(ARGS)

# Follow all logs
testnet-follow:
	docker compose logs -f

# Run tests in Docker
docker-test:
	docker compose -f docker-compose.test.yml run --rm test-rust

# Run TypeScript tests in Docker
docker-test-ts:
	docker compose -f docker-compose.test.yml run --rm test-typescript

# Run linter in Docker
docker-lint:
	docker compose -f docker-compose.test.yml run --rm lint-rust

# Run integration tests
docker-integration:
	docker compose -f docker-compose.test.yml up --abort-on-container-exit integration-test

# Shell into a running container (usage: make docker-exec SERVICE=gossipd)
docker-exec:
	docker compose exec $(SERVICE) /bin/bash

# Show running services
docker-ps:
	docker compose ps
