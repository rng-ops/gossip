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
    docker build -t terrain-gossip/gossipd -f docker/gossipd.Dockerfile .
    docker build -t terrain-gossip/routerd -f docker/routerd.Dockerfile .
    docker build -t terrain-gossip/prober -f docker/prober.Dockerfile .
    docker build -t terrain-gossip/infernode -f docker/infernode.Dockerfile .

# Run local test network
testnet-up:
    @echo "Starting local test network..."
    @echo "TODO: Implement docker-compose based testnet"

# Stop local test network
testnet-down:
    @echo "Stopping local test network..."
    @echo "TODO: Implement docker-compose based testnet"
