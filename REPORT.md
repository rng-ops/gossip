# TerrainGossip: Decentralized Infrastructure for AI Manipulation Detection

> A gossip-based protocol for distributed LLM evaluation, behavioral monitoring, and evidence collection—built to resist manipulation of the monitoring system itself.

**Hackathon Submission: Apart Research AI Manipulation Hackathon (January 2026)**

---

## 1. Summary

TerrainGossip is a peer-to-peer gossip protocol that enables **decentralized, continuous behavioral benchmarking** of LLM providers without central coordination. Nodes maintain local "belief fields" about provider behavior, exchange cryptographically-signed attestations via delta sync, and use robust aggregation to resist lying sensors. The protocol is designed so that (a) providers cannot detect when they are being evaluated, (b) no single authority controls the "truth" about model behavior, and (c) the monitoring infrastructure itself is resistant to manipulation, poisoning, and sybil attacks.

This positions TerrainGossip as infrastructure for **AI manipulation defense**: it provides the substrate for collecting, distributing, and analyzing behavioral evidence across a decentralized network of evaluators—without creating a single point of failure that adversaries can compromise.

---

## 2. Problem Statement: Why Centralized Evaluation is Fragile

Current AI evaluation infrastructure has three critical weaknesses:

### 2.1 Single Points of Compromise
Centralized leaderboards and evaluation servers (e.g., LMSYS Arena, HuggingFace Open LLM Leaderboard) create high-value targets. If an adversary can manipulate the benchmark server, poison the evaluation dataset, or compromise the scoring pipeline, they control the perceived "truth" about model capabilities. A model that sandbags on manipulation detection benchmarks while performing well on public evals can avoid scrutiny.

### 2.2 Provider-Aware Evaluation
Static benchmarks are fingerprint-able. Providers can detect evaluation prompts (through statistical patterns, known datasets, or timing signatures) and behave differently during evaluation than during deployment. This is the **sandbagging problem**: a model that recognizes it's being tested can hide capabilities or misbehavior.

### 2.3 No Persistent Evidence Trail
Most evaluation happens once at release time. Real-world deployments experience **behavioral drift**—models can be updated, fine-tuned, or silently modified. Without continuous monitoring with cryptographic evidence, there's no way to detect when a deployed model starts behaving differently than the evaluated version.

### 2.4 The Fundamental Issue
> If AI systems can manipulate their evaluators, the entire safety evaluation pipeline becomes meaningless.

We need evaluation infrastructure that is:
- **Distributed**: No single point of compromise
- **Continuous**: Detects behavioral changes post-deployment
- **Covert**: Evaluations are indistinguishable from normal traffic
- **Verifiable**: Evidence is cryptographically signed and auditable
- **Robust**: Resilient to lying sensors and sybil attacks

---

## 3. Conceptual Model: Terrain, Gossip, and Belief Fields

TerrainGossip introduces three core abstractions:

### 3.1 Terrain: Topology for Locality
The "terrain" is a deterministic overlay topology that partitions the network into regions, chunks, and cells. This can be:
- **Literal**: Geospatial coordinates (for latency-aware routing)
- **Abstract**: Model-capability space (group providers by what they serve)
- **Functional**: Hash-based partitioning for load distribution

Terrain provides **locality**: updates propagate to nearby nodes first, global convergence happens eventually. This bounds bandwidth and prevents flooding.

```
┌─────────────────────────────────────────────────────┐
│                    World Terrain                    │
│  ┌─────────────┬─────────────┬─────────────┐       │
│  │  Region 0   │  Region 1   │  Region 2   │       │
│  │ ┌───┬───┐   │ ┌───┬───┐   │ ┌───┬───┐   │       │
│  │ │C0 │C1 │   │ │C0 │C1 │   │ │C0 │C1 │   │       │
│  │ ├───┼───┤   │ ├───┼───┤   │ ├───┼───┤   │       │
│  │ │C2 │C3 │   │ │C2 │C3 │   │ │C2 │C3 │   │       │
│  │ └───┴───┘   │ └───┴───┘   │ └───┴───┘   │       │
│  └─────────────┴─────────────┴─────────────┘       │
│                                                     │
│  Cells contain: version vectors, bloom filters,    │
│  event summaries, pheromone trails (routing hints) │
└─────────────────────────────────────────────────────┘
```

### 3.2 Gossip: Incremental Evidence Propagation
Nodes don't synchronize a complete database. Instead, they exchange **deltas**—new events since a causal clock (version vector). Each event is:
- Signed by the emitter (prober, router, or verifier)
- Immutable and content-addressed (`EventId = BLAKE3(canonical_bytes(event))`)
- Typed: ProbeReceipt, BehaviorAttestation, Dispute, LinkHint, etc.

Delta sync is bandwidth-efficient: nodes only request events they haven't seen. Anti-entropy cycles (periodic full sync) heal partitions.

### 3.3 Belief Fields: Local Computation, Statistical Convergence
Each node computes its own **belief field**—a local posterior estimate of provider behavior based on the signed events it has observed:

```
belief[provider] = {
  mu:          expected quality (0..1)
  sigma:       uncertainty
  trend:       EWMA of recent changes
  disagreement: dispersion across attestations
}
```

There is **no canonical global score**. Beliefs converge statistically because nodes observe overlapping evidence. This is the key insight: we don't need consensus on "the score"—we need convergence of local beliefs given shared evidence.

**Example flow:**
1. Prober P₁ evaluates Provider X, emits signed `BehaviorAttestation`
2. Attestation propagates via gossip to nodes N₁, N₂, N₃
3. Each node updates its local `belief[X]` using robust aggregation
4. Over time, all honest nodes converge to similar beliefs about X
5. No single node or authority decided "the truth"

---

## 4. Data Model

### 4.1 Memory Item Schema (Events)

```rust
struct Event {
    event_id: [u8; 32],         // = BLAKE3(canonical_bytes(body))
    world: WorldId,              // which evaluation world
    epoch_id: u64,               // time bucket
    event_type: EventType,       // Receipt, Attestation, Dispute, etc.
    body: EventBody,             // typed payload
}

enum EventBody {
    Receipt(ProbeReceipt),           // proof that a probe happened
    Attestation(BehaviorAttestation), // metrics + evidence commitment
    Dispute(DisputeEvent),            // conflicting attestations
    LinkHint(LinkHintEvent),          // evidence two refs are same provider
    RuleEndorsement(RuleEndorsementEvent), // governance
    // ... optional: Shard, Verdict, TrainingManifest
}
```

### 4.2 Delta/Update Schema

Updates are simply new events. The protocol supports:
- **Add**: New event with fresh `event_id`
- **Tombstone**: Dispute events can mark prior events as contested
- **No physical delete**: Append-only log preserves auditability

### 4.3 Per-Cell Summaries (Voxel Metadata)

Each terrain cell maintains:

```rust
struct CellSummary {
    cell_id: TerrainAddress,
    version_vector: HashMap<ReplicaId, u64>,  // causal clock
    bloom_filter: BloomFilter,                 // quick event membership
    event_count: u64,
    centroid: Option<[f32; 768]>,             // embedding centroid (optional)
    pheromone_map: HashMap<ProviderRef, f64>, // routing hints
    last_updated: Timestamp,
}
```

Bloom filters enable fast "do you have event X?" checks without transferring full event lists.

---

## 5. Protocol Sketch

### 5.1 Neighbor Selection

Nodes select gossip partners based on:
1. **Latency**: Prefer low-latency peers for fast propagation
2. **Locality**: Prefer peers in nearby terrain cells
3. **Interest**: Prefer peers evaluating similar model families
4. **Diversity**: Require some random/distant peers to prevent partitions

```
┌─────────┐         ┌─────────┐         ┌─────────┐
│ Prober₁ │◄───────►│ Gossipd │◄───────►│ Router₁ │
└────┬────┘         └────┬────┘         └────┬────┘
     │                   │                   │
     │    Delta Sync     │    Delta Sync     │
     │   (pull missing   │   (push new       │
     │    events)        │    attestations)  │
     ▼                   ▼                   ▼
┌─────────┐         ┌─────────┐         ┌─────────┐
│ Prober₂ │◄───────►│ Gossipd │◄───────►│ Router₂ │
└─────────┘         └─────────┘         └─────────┘
```

### 5.2 Handshake: Summary Exchange

```
1. A → B: DeltaSyncRequest { world, since: A's version_vector, max_events }
2. B → A: DeltaSyncResponse { events: [...], now: B's version_vector }
3. A verifies signatures, inserts events, updates local version_vector
4. A → B: (optional) push A's new events that B lacks
```

The handshake starts with **version vector comparison**—nodes identify the causal frontier before exchanging actual events.

### 5.3 Anti-Entropy Cycle

Periodic full reconciliation:
1. Every `T` seconds (configurable, e.g., 30s), node initiates sync with random subset of peers
2. Bloom filter exchange identifies likely-missing events
3. Full event lists exchanged for cells with high divergence
4. Repairs partitions and stale nodes

### 5.4 Conflict Resolution

Events are immutable, so conflicts are handled at the interpretation layer:

1. **Concurrent attestations**: Multiple probers evaluate same provider → robust aggregation (median, trimmed mean)
2. **Contradictory attestations**: Prober A says "good", Prober B says "bad" → disagreement increases uncertainty (`sigma`)
3. **Disputed events**: Explicit `DisputeEvent` marks conflicting events; nodes increase redundancy
4. **Version vector ordering**: Causally-related updates ordered by vector clocks; concurrent updates both accepted

**Design choice**: We use **CRDT-style semantics** (grow-only set of events, version vectors for ordering) rather than last-writer-wins. This preserves evidence—even "wrong" attestations are valuable for detecting lying probers.

---

## 6. Security and Abuse Considerations

### 6.1 Poisoning and Spam

**Threat**: Adversary floods network with fake attestations to bias beliefs.

**Mitigations**:
- **Proof-of-probe cost**: Attestations can require a `ProbeTicket` (scrypt/argon2 output) to impose computational cost
- **Rate limiting**: Nodes enforce per-source rate limits on event ingestion
- **Signature verification**: All events are signed; unsigned/invalid events rejected immediately
- **Reputation tracking**: Probers accumulate trust based on predictive accuracy; untrusted probers are down-weighted

### 6.2 Sybil Resistance

**Threat**: Adversary creates many fake prober identities to dominate aggregation.

**Mitigations**:
- **Diversity requirements**: Promotions/endorsements require attestations from diverse clusters (not just count)
- **Anti-correlation penalties**: Tightly correlated probers (same IP range, similar timing, identical scores) saturate contribution
- **Robust aggregation**: Median/trimmed mean reduces impact of outliers
- **Exploration floor**: Routers always explore some fraction of providers, preventing cartel lock-in

### 6.3 Provider Manipulation (Sandbagging Detection)

**Threat**: Provider detects evaluation prompts and behaves differently.

**Mitigations**:
- **Metamorphic probes**: Challenge prompts are randomized per epoch (paraphrases, equivalent instances)
- **Indistinguishable traffic**: Some probes route through the same onion circuits as normal traffic
- **Challenge commitments**: `ChallengeId` is a hash commitment, not a literal prompt identifier
- **Provider blindness**: Providers cannot compute the `TargetRef` used in control-plane aggregation (requires key they don't have)

### 6.4 Privacy: What Leaks, What Doesn't

| Leaks | Doesn't Leak |
|-------|--------------|
| That a probe occurred | The observer's local Handle for provider |
| Aggregate metrics (via attestations) | Raw user prompts/responses |
| Provider capability (FAH) | Provider's own control-plane TargetRef |
| Prober identity (transport key) | Correlation between normal traffic and probes |

**Mitigation for prober privacy**: Onion routing for probe traffic; probers can rotate transport keys per epoch.

---

## 7. Relevance to AI Manipulation Defense

TerrainGossip directly addresses several manipulation risks:

### 7.1 Detecting Sandbagging and Capability Elicitation

The continuous, decentralized probing system creates a **persistent behavioral record**. If a model's behavior changes (e.g., starts refusing certain prompts, or suddenly gains new capabilities), the change is detected as:
- Trend shift in belief field
- Disagreement between old and new attestations
- Disputes raised automatically

This enables detection of:
- **Sandbagging**: Model hiding capabilities during evaluation → capabilities revealed later
- **Sycophancy drift**: Model becoming increasingly agreeable over deployment
- **Policy violations**: Model bypassing safety filters under certain conditions

### 7.2 Distributed Evidence Collection

Every behavioral observation is:
- **Signed**: By the prober's transport key
- **Timestamped**: With epoch and freshness anchor
- **Content-addressed**: Immutable EventId
- **Propagated**: Via gossip to all interested nodes

This creates a **distributed evidence log** that cannot be retroactively modified. If a model is later found to be manipulative, the historical behavioral trajectory is preserved and auditable.

### 7.3 Robust to Evaluator Manipulation

The protocol assumes some probers will lie (noisy sensors, adversarial actors). Robust aggregation and trust-based weighting ensure that:
- Minority liars cannot shift aggregate beliefs significantly
- Probers that consistently diverge from consensus lose trust
- No single authority decides what "correct" evaluation looks like

### 7.4 Preventing Manipulation of the Monitoring System

Traditional monitoring systems can be attacked at:
- The benchmark server (compromise)
- The dataset (poisoning)
- The scoring pipeline (bias injection)
- The publication channel (censorship)

TerrainGossip eliminates these by:
- **No central server**: Gossip-based replication
- **Signed evidence**: Tampering is detectable
- **No canonical score**: Local beliefs resist authoritative injection
- **Forkable worlds**: Censorship → participants fork to new rules

---

## 8. Roadmap

### 8.1 MVP Scope (Weekend Hackathon)

**Implemented** (this repository):

| Component | Status | Description |
|-----------|--------|-------------|
| Core types | ✅ | WorldId, FAH, TargetRef, DescriptorId, Events |
| Canonical encoding | ✅ | Postcard serialization, cross-validated Rust ↔ TypeScript |
| Cryptographic primitives | ✅ | BLAKE3 hashing, Ed25519 signatures, X25519 key exchange |
| Test vectors | ✅ | 6 cross-language vectors passing |
| gossipd daemon | ✅ | Event log storage, delta sync, version vectors |
| routerd daemon | ✅ | Terrain map, FAH routing, belief field computation |
| prober daemon | ✅ | Probe scheduling, challenge generation, receipt signing |
| infernode daemon | ✅ | Onion circuit construction, relay forwarding |
| terrain-gossip-net | ✅ | Networking: crypto, framing, transport, circuits |
| Build automation | ✅ | Makefile, justfile, GitHub Actions CI/CD |

**Test coverage**: 57 Rust tests + 6 TypeScript test vectors passing.

### 8.2 Next Milestones

| Milestone | Description | Effort |
|-----------|-------------|--------|
| **Simulator** | Multi-node simulation for convergence testing | 1-2 weeks |
| **Liar robustness benchmarks** | Test belief convergence under X% lying probers | 1 week |
| **Browser integration** | WASM build of core library for browser nodes | 2 weeks |
| **Vector memory** | Embedding index for semantic event retrieval | 2 weeks |
| **Real deployment** | Deploy gossipd cluster, connect external probers | 4 weeks |

### 8.3 Integration Points

TerrainGossip is designed as infrastructure that other tools can build on:

- **Manipulation detection benchmarks**: Plug in new challenge suites as prober modules
- **Model monitoring dashboards**: Subscribe to gossip for real-time behavioral alerts
- **Incident investigation**: Query vector memory for historical behavioral patterns
- **Training verification**: (Optional plugin) Verify training data provenance via shard pipeline

---

## 9. Glossary

| Term | Definition |
|------|------------|
| **Voxel** | Discretization unit in terrain; contains cell summaries, version vectors, bloom filters |
| **Anti-entropy** | Periodic full synchronization cycle to repair partitions |
| **Delta** | Incremental update (new events since a causal clock) |
| **Version vector** | Map of `replica_id → counter` representing causal frontier |
| **Sketch** | Probabilistic data structure (e.g., bloom filter) for set membership |
| **Provenance** | Origin and chain of custody for data/events |
| **FAH** | Functional Address Hash—content-addressed capability claim |
| **TargetRef** | Control-plane reference for a provider; provider-blind |
| **Handle** | Observer-local pseudonym for a provider; never gossiped |
| **Belief field** | Local posterior estimates about provider behavior |
| **Epoch** | Time bucket for batching events and rotating keys |
| **World** | Overlay network defined by phrase seed + rule bundle hash |
| **Attestation** | Signed metrics report with evidence commitment |
| **Receipt** | Signed proof that a probe occurred |

---

## 10. Technical Specifications

### 10.1 Cryptographic Primitives

| Primitive | Usage |
|-----------|-------|
| BLAKE3 | All hash derivations with domain separation |
| Ed25519 | Transport key signatures, attestation signing |
| X25519 | Ephemeral key agreement for onion circuits |
| ChaCha20-Poly1305 | AEAD encryption for circuit cells |
| HKDF-BLAKE3 | Key derivation for circuit hops |

### 10.2 Key Derivations

```
WorldId = BLAKE3("world" || phrase_norm || rule_bundle_hash)
FAH = BLAKE3("fah" || canonical_bytes(CapabilityManifest))
DescriptorId = BLAKE3("descriptor" || canonical_bytes(ProviderDescriptorUnsigned))
TargetRef = BLAKE3_KEYED(control_plane_key, "targetref" || WorldId || DescriptorId)
Handle = BLAKE3("handle" || observer_secret || observed_fingerprint)
EventId = BLAKE3(canonical_bytes(Event))
```

### 10.3 Wire Protocol

Delta sync uses a simple request/response pattern:

```protobuf
message DeltaSyncRequest {
  WorldId world = 1;
  repeated VersionVectorEntry since = 2;
  uint32 max_events = 3;
}

message DeltaSyncResponse {
  WorldId world = 1;
  repeated Event events = 2;
  repeated VersionVectorEntry now = 3;
}
```

---

## 11. Conclusion

TerrainGossip provides the **infrastructure layer** for robust AI evaluation and manipulation defense:

1. **Decentralized by design**: No central point of compromise
2. **Continuous monitoring**: Detects behavioral drift post-deployment
3. **Covert evaluation**: Probes are indistinguishable from normal traffic
4. **Cryptographic evidence**: Signed, timestamped, immutable attestations
5. **Robust aggregation**: Resists lying sensors and sybil attacks
6. **Self-protecting**: The monitoring system resists manipulation of itself

The core insight is that **manipulation resistance requires decentralization**. Any centralized evaluation system is a target. By distributing evidence collection, aggregation, and belief formation across a gossip network with cryptographic attestations and robust statistics, we create evaluation infrastructure that remains trustworthy even when some participants are adversarial.

This is not a complete solution to AI manipulation—that requires work across benchmarks, interpretability, governance, and deployment practices. But it is a **necessary foundation**: the substrate on which manipulation detection tools can be built without themselves becoming vectors for manipulation.

---

## Repository

- **Source**: [github.com/rng-ops/gossip](https://github.com/rng-ops/gossip)
- **Specification**: [docs/rfc-0001.md](docs/rfc-0001.md)
- **License**: MIT / Apache-2.0

---

*Built for the Apart Research AI Manipulation Hackathon, January 2026.*
