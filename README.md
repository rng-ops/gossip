# TerrainGossip

> **A decentralized protocol for private, censorship-resistant LLM inference with continuous behavioral benchmarking**

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![TypeScript](https://img.shields.io/badge/typescript-5.3%2B-blue.svg)](https://www.typescriptlang.org/)

---

## 🌍 What is TerrainGossip?

TerrainGossip is a **gossip-based mesh protocol** for distributed LLM inference that provides:

- **Privacy**: Onion-style multi-hop routing ensures no single node learns both origin and content
- **Decentralization**: No central coordinator, leaderboard, or canonical global score
- **Continuous Benchmarking**: Probers evaluate providers through signed attestations
- **Forkable Governance**: "Worlds" are defined by phrase seeds + rule hashes, making governance forkable by construction
- **Provider Blindness**: Providers cannot learn the handles or scores observers assign to them

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Client    │────▶│   Router    │────▶│  Provider   │
└─────────────┘     └──────┬──────┘     └─────────────┘
                          │
                    ┌─────▼─────┐
                    │  Gossipd  │◀────── Delta Sync
                    │ (Events)  │
                    └─────┬─────┘
                          │
        ┌────────────────┬┴───────────────┐
        ▼                ▼                ▼
   ┌─────────┐     ┌──────────┐     ┌──────────┐
   │ Prober  │     │ Prober   │     │ Prober   │
   └─────────┘     └──────────┘     └──────────┘
```

---

## ✨ Key Features

### 🔐 Privacy-First Design

| Feature | Description |
|---------|-------------|
| **Onion Routing** | Multi-hop encrypted circuits (default 3 hops) using X25519 + ChaCha20-Poly1305 |
| **Provider Blindness** | Providers cannot compute the `TargetRef` used in control-plane aggregation |
| **Observer-Local Handles** | Each observer assigns private handles; no global identity linkage |
| **Two-Plane Separation** | Inference plane (providers) is isolated from control plane (benchmarking) |

### 📊 Decentralized Benchmarking

| Feature | Description |
|---------|-------------|
| **ProbeReceipts** | Signed proof that a benchmark probe occurred |
| **BehaviorAttestations** | Metrics reports with freshness anchors |
| **Dispute Resolution** | Automatic conflict detection when attestations diverge |
| **Metamorphic Tests** | Probes are randomized per-epoch to prevent detection |

### 🌐 Gossip-Based Routing

| Feature | Description |
|---------|-------------|
| **No Canonical Score** | Nodes compute local beliefs from signed events |
| **Delta Sync** | Efficient incremental event replication |
| **Vector Memory** | Embedding index for semantic retrieval and anomaly detection |
| **Statistical Convergence** | Beliefs converge without global consensus |

### 🔧 Forkable Governance

| Feature | Description |
|---------|-------------|
| **World Identity** | `WorldId = BLAKE3("world" \|\| phrase \|\| rule_bundle_hash)` |
| **Rule Bundles** | Versioned configs for epochs, thresholds, metrics weights |
| **Natural Forking** | Change rules → new WorldId → new world |

---

## 🏗️ Architecture

### Node Roles

| Role | Description |
|------|-------------|
| **Provider** | Serves LLM inference, publishes Capability Manifest |
| **Router** | Selects providers using local beliefs, builds onion circuits |
| **Prober** | Benchmarks providers, emits signed receipts/attestations |
| **Gossipd** | Stores append-only event log, serves delta sync |
| **Verifier** | *(Optional)* Validates training shards, emits verdicts |
| **Trainer** | *(Optional)* Trains LoRA adapters from verified shards |

### Protocol Spine

```
Event Log → Delta Sync → Vector Memory → Belief Field → Routing
```

### Two Planes

- **Inference Plane**: Onion-routed client requests/responses (providers participate)
- **Control Plane**: Benchmarking, disputes, descriptor discovery (membership-gated)

---

## 📁 Repository Structure

```
gossip/
├── docs/
│   └── rfc-0001.md          # Full protocol specification
├── crates/
│   └── terrain-gossip-core/  # Rust core library
│       └── src/
│           ├── types.rs      # All protocol types
│           ├── canonical.rs  # Postcard encoding + normalization
│           ├── crypto.rs     # BLAKE3 hash derivations
│           ├── error.rs      # Error types
│           └── test_vectors.rs
├── packages/
│   └── proto/               # TypeScript implementation
│       └── src/
│           ├── types.ts     # Type definitions
│           ├── canonical.ts # Varint encoding
│           ├── crypto.ts    # Hash derivations
│           └── test_vectors.test.ts
├── proto/
│   └── terrain_gossip.proto # Wire protocol schema
├── test_vectors.json        # Cross-language test vectors
└── Cargo.toml               # Workspace config
```

---

## ✅ Implementation Status

### Core Protocol (RFC-0001)

| Component | Status | Description |
|-----------|--------|-------------|
| **Types & Identifiers** | ✅ Complete | WorldId, FAH, Handle, TargetRef, DescriptorId, etc. |
| **Canonical Encoding** | ✅ Complete | Postcard-compatible, cross-validated Rust ↔ TypeScript |
| **Hash Derivations** | ✅ Complete | BLAKE3 with domain separation, keyed hashing |
| **Test Vectors** | ✅ Complete | 6 vectors, both languages pass |
| **Protobuf Schema** | ✅ Complete | Full wire schema extracted from RFC |

### Daemons

| Daemon | Status | Description |
|--------|--------|-------------|
| **gossipd** | ⏳ Not Started | Event log, delta sync, vector memory |
| **routerd** | ⏳ Not Started | Terrain map, FAH routing, belief fields |
| **prober** | ⏳ Not Started | Probe scheduling, receipt/attestation generation |
| **infernode** | ⏳ Not Started | Onion routing, inference relay |

### Optional Plugins

| Plugin | Status | Description |
|--------|--------|-------------|
| Training Data Shards | 📋 Specified | Curator → Verifier → Trainer pipeline |
| LoRA Publishing | 📋 Specified | Adapter training and distribution |

---

## 🔬 Novel Mechanisms

### 1. Provider Blindness with Aggregation

The protocol solves a hard problem: how to aggregate reputation across observers without giving providers a stable identity they can track.

**Solution**: Two-tier identity system:
- **Handle** (local): `BLAKE3("handle" || observer_secret || fingerprint)` — never leaves the observer
- **TargetRef** (control-plane): `BLAKE3_KEYED(control_plane_key, "targetref" || WorldId || DescriptorId)` — shared among control-plane members, but providers lack the key

### 2. Forkable Worlds

Unlike blockchains that require consensus on governance changes:
```
WorldId = BLAKE3("world" || phrase_norm || rule_bundle_hash)
```
Change the rules → get a new WorldId → automatic fork. No migration needed.

### 3. No Canonical Score

Traditional reputation systems publish a "trust score." TerrainGossip doesn't:
- Nodes compute **local belief fields** from signed events
- Beliefs **converge statistically** as nodes see overlapping evidence
- No global leaderboard to game

### 4. Probe Anti-Detection

Providers can't fingerprint benchmark prompts because:
- Probes are **metamorphic** (randomized per epoch)
- Some probes are **indistinguishable from normal traffic**
- ChallengeIds are **commitments**, not literal prompt IDs

### 5. Rotating Replica IDs

Delta sync version vectors don't leak stable identity:
```
replica_id = BLAKE3("replica" || transport_pubkey || world_id || epoch_id)
```

---

## 🚀 Getting Started

### Prerequisites

- **Rust**: 1.75+ with `cargo`
- **Node.js**: 20+ with `npm`

### Build

```bash
# Clone repository
git clone https://github.com/rng-ops/gossip.git
cd gossip

# Build Rust
cargo build

# Build TypeScript
cd packages/proto
npm install
npm run build
```

### Test

```bash
# Run Rust tests (11 tests)
cargo test

# Run TypeScript tests (6 tests)
cd packages/proto
npm test
```

### Generate Test Vectors

```bash
cargo test test_generate_vectors -- --nocapture
```

---

## 📖 Protocol Specification

The full protocol specification is in [docs/rfc-0001.md](docs/rfc-0001.md).

### Key Sections

| Section | Description |
|---------|-------------|
| §3 | Formal identifiers and cryptographic objects |
| §4 | Protocol spine: event log → beliefs → routing |
| §5 | Privacy and mixing: circuit construction |
| §6 | Benchmarking: probes, receipts, attestations |
| §7 | Gossip and "no canonical score" |
| §8 | Game theory and incentives |
| §9 | Governance: rule bundles and world forking |
| §10 | Optional plugin: training data shards |
| §11 | Protobuf wire schema |

---

## 🗺️ Roadmap

### Phase 1: Core Protocol ✅
- [x] RFC-0001 specification
- [x] Core types (Rust + TypeScript)
- [x] Canonical encoding with test vectors
- [x] Cryptographic primitives (BLAKE3, Ed25519, X25519)
- [x] Protobuf schema

### Phase 2: Control Plane (Next)
- [ ] **gossipd**: Event log storage, delta sync protocol
- [ ] **routerd**: Terrain topology, FAH routing tables
- [ ] Belief field computation from attestations
- [ ] Dispute detection and handling

### Phase 3: Inference Plane
- [ ] **infernode**: Onion circuit construction
- [ ] Multi-hop relay with AEAD encryption
- [ ] Provider descriptor discovery
- [ ] Circuit management (create/extend/destroy)

### Phase 4: Benchmarking
- [ ] **prober**: Probe scheduling and execution
- [ ] Metamorphic challenge generation
- [ ] Receipt and attestation signing
- [ ] Freshness anchor integration

### Phase 5: Production Hardening
- [ ] Persistent storage backends
- [ ] Network transport (QUIC, libp2p)
- [ ] Metrics and observability
- [ ] Deployment guides (Docker, k8s)

### Phase 6: Optional Plugins
- [ ] Training data shard pipeline
- [ ] Verifier committee
- [ ] LoRA training and publishing
- [ ] Vector memory semantic search

---

## 🔒 Security Considerations

### Threat Model

TerrainGossip provides:
- ✅ Relay unlinkability (no single node learns origin + content)
- ✅ Provider blindness (providers can't compute their TargetRef)
- ✅ Sybil resistance (probe tickets, diversity requirements)
- ✅ Robust aggregation (median/trimmed mean reduces liar impact)

TerrainGossip does NOT provide:
- ❌ Protection against a global passive adversary with timing correlation
- ❌ Covert channels or protocol mimicry
- ❌ Perfect anonymity (practical tradeoffs for performance)

### Cryptographic Primitives

| Primitive | Usage |
|-----------|-------|
| BLAKE3 | All hash derivations with domain separation |
| Ed25519 | Signatures (transport keys, attestations) |
| X25519 | Ephemeral key agreement for circuits |
| ChaCha20-Poly1305 | AEAD for circuit cells |
| HKDF | Key derivation for circuit hops |

---

## 🤝 Contributing

Contributions are welcome! Please read the RFC carefully before proposing changes.

### Development

```bash
# Format code
cargo fmt
cd packages/proto && npm run lint

# Run all tests
cargo test
cd packages/proto && npm test
```

### Areas of Interest

- Implementing daemon skeletons (gossipd, routerd, prober, infernode)
- Network transport layer (libp2p integration)
- Vector memory backends (embedding storage)
- Simulator for testing routing convergence

---

## 📄 License

Dual licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.

---

## 📚 References

- [RFC-0001: TerrainGossip LLM Mesh Protocol](docs/rfc-0001.md)
- [BLAKE3 Specification](https://github.com/BLAKE3-team/BLAKE3-specs)
- [Postcard Format](https://github.com/jamesmunns/postcard)
- [Tor Specification](https://spec.torproject.org/)

---

<p align="center">
  <em>Privacy-preserving LLM inference for everyone</em>
</p>
