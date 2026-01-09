# TerrainGossip Multi-Stage Dockerfile
# Builds all daemon binaries in a single image

# =============================================================================
# Stage 1: Build
# =============================================================================
FROM rust:latest AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Update Rust to latest stable
RUN rustup update stable && rustup default stable

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates/terrain-gossip-core/Cargo.toml crates/terrain-gossip-core/
COPY crates/terrain-gossip-net/Cargo.toml crates/terrain-gossip-net/
COPY crates/gossipd/Cargo.toml crates/gossipd/
COPY crates/routerd/Cargo.toml crates/routerd/
COPY crates/prober/Cargo.toml crates/prober/
COPY crates/infernode/Cargo.toml crates/infernode/

# Create dummy source files for dependency caching
RUN mkdir -p crates/terrain-gossip-core/src && echo "pub fn dummy() {}" > crates/terrain-gossip-core/src/lib.rs \
    && mkdir -p crates/terrain-gossip-net/src && echo "pub fn dummy() {}" > crates/terrain-gossip-net/src/lib.rs \
    && mkdir -p crates/gossipd/src && echo "fn main() {}" > crates/gossipd/src/main.rs && echo "pub fn dummy() {}" > crates/gossipd/src/lib.rs \
    && mkdir -p crates/routerd/src && echo "fn main() {}" > crates/routerd/src/main.rs && echo "pub fn dummy() {}" > crates/routerd/src/lib.rs \
    && mkdir -p crates/prober/src && echo "fn main() {}" > crates/prober/src/main.rs && echo "pub fn dummy() {}" > crates/prober/src/lib.rs \
    && mkdir -p crates/infernode/src && echo "fn main() {}" > crates/infernode/src/main.rs && echo "pub fn dummy() {}" > crates/infernode/src/lib.rs

# Build dependencies (cached layer)
RUN cargo build --release --workspace 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/
COPY test_vectors.json ./

# Touch source files to invalidate cache and rebuild
RUN touch crates/*/src/*.rs

# Build release binaries
RUN cargo build --release --workspace

# =============================================================================
# Stage 2: Runtime base
# =============================================================================
FROM debian:bookworm-slim AS runtime-base

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -u 1000 -m gossip

# =============================================================================
# Stage 3: gossipd
# =============================================================================
FROM runtime-base AS gossipd

COPY --from=builder /build/target/release/gossipd /usr/local/bin/

USER gossip
WORKDIR /home/gossip

# Default data directory
RUN mkdir -p /home/gossip/data

EXPOSE 9100

ENTRYPOINT ["gossipd"]
CMD ["--help"]

# =============================================================================
# Stage 4: routerd
# =============================================================================
FROM runtime-base AS routerd

COPY --from=builder /build/target/release/routerd /usr/local/bin/

USER gossip
WORKDIR /home/gossip

EXPOSE 9200

ENTRYPOINT ["routerd"]
CMD ["--help"]

# =============================================================================
# Stage 5: prober
# =============================================================================
FROM runtime-base AS prober

COPY --from=builder /build/target/release/prober /usr/local/bin/

USER gossip
WORKDIR /home/gossip

RUN mkdir -p /home/gossip/data

EXPOSE 9300

ENTRYPOINT ["prober"]
CMD ["--help"]

# =============================================================================
# Stage 6: infernode
# =============================================================================
FROM runtime-base AS infernode

COPY --from=builder /build/target/release/infernode /usr/local/bin/

USER gossip
WORKDIR /home/gossip

EXPOSE 9400

ENTRYPOINT ["infernode"]
CMD ["--help"]

# =============================================================================
# Stage 7: All-in-one (for development/testing)
# =============================================================================
FROM runtime-base AS all

COPY --from=builder /build/target/release/gossipd /usr/local/bin/
COPY --from=builder /build/target/release/routerd /usr/local/bin/
COPY --from=builder /build/target/release/prober /usr/local/bin/
COPY --from=builder /build/target/release/infernode /usr/local/bin/

USER gossip
WORKDIR /home/gossip

RUN mkdir -p /home/gossip/data

EXPOSE 9100 9200 9300 9400

# Default to shell for manual execution
CMD ["/bin/bash"]
