# Justfile for TerrainGossip LLM Mesh Protocol
# Install just: cargo install just

set shell := ["bash", "-cu"]

# Default recipe: show help
default:
    @just --list

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

# Run all CI checks (format, lint, test)
ci: fmt-check lint test
    @echo "All CI checks passed!"

# Generate documentation
doc:
    cargo doc --workspace --no-deps --open

# Generate documentation without opening
doc-build:
    cargo doc --workspace --no-deps

# Clean build artifacts
clean:
    cargo clean

# Install dev dependencies
setup:
    rustup component add rustfmt clippy
    cargo install cargo-watch

# Watch for changes and run tests
watch:
    cargo watch -x "test --workspace"

# Watch for changes and check
watch-check:
    cargo watch -x "check --workspace"

# Run gossipd daemon
run-gossipd *ARGS:
    cargo run -p gossipd -- {{ARGS}}

# Run routerd daemon
run-routerd *ARGS:
    cargo run -p routerd -- {{ARGS}}

# Run prober daemon
run-prober *ARGS:
    cargo run -p prober -- {{ARGS}}

# Run infernode daemon
run-infernode *ARGS:
    cargo run -p infernode -- {{ARGS}}

# Build TypeScript packages
ts-build:
    cd ts/proto && npm install && npm run build

# Test TypeScript packages
ts-test:
    cd ts/proto && npm test

# Run cross-language test vectors
test-vectors:
    cargo test -p terrain-gossip-core test_vector
    cd ts/proto && npm test

# Generate test vectors
gen-vectors:
    cargo run -p terrain-gossip-core --example generate_vectors

# Security audit
audit:
    cargo audit

# Check for outdated dependencies
outdated:
    cargo outdated

# Update dependencies
update:
    cargo update

# Create a new release
release VERSION:
    @echo "Creating release {{VERSION}}"
    git tag -a v{{VERSION}} -m "Release {{VERSION}}"
    git push origin v{{VERSION}}

# Build Docker images
docker-build:
    docker compose build

# Build specific Docker image
docker-build-image TARGET:
    docker build --target {{TARGET}} -t terraingossip/{{TARGET}}:latest .

# Push Docker images to registry
docker-push:
    docker compose push

# Run local test network
testnet-up:
    @echo "Starting local test network..."
    docker compose up -d

# Run full network with multiple probers
testnet-up-full:
    @echo "Starting full test network with multiple probers..."
    docker compose --profile scaled up -d

# Stop local test network
testnet-down:
    @echo "Stopping local test network..."
    docker compose down

# Stop and clean up volumes
testnet-clean:
    @echo "Stopping and cleaning local test network..."
    docker compose down -v

# View logs
testnet-logs *ARGS:
    docker compose logs {{ARGS}}

# Follow logs
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

# Shell into a running container
docker-exec SERVICE:
    docker compose exec {{SERVICE}} /bin/bash

# Show running services
docker-ps:
    docker compose ps
