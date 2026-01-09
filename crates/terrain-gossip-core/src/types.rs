//! Core protocol types for TerrainGossip (RFC-0001 §16.2)
//!
//! All types here are designed for deterministic serialization via postcard.
//! Field order matters for canonical encoding.

use serde::{Deserialize, Serialize};

/// 32-byte fixed-size array used for hashes and identifiers.
pub type Bytes32 = [u8; 32];

// =============================================================================
// IDENTITY TYPES (newtypes for type safety)
// =============================================================================

/// World identifier: BLAKE3("world" || phrase_norm || rule_bundle_hash)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct WorldId(pub Bytes32);

/// Functional Address Hash: BLAKE3("fah" || canonical_bytes(CapabilityManifest))
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Fah(pub Bytes32);

/// Observer-local handle (strictly local, never gossiped)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Handle(pub Bytes32);

/// Control-plane target reference (world-scoped, provider-blind)
/// Derived: BLAKE3_KEYED(control_plane_key, "targetref" || WorldId || epoch_id || DescriptorId)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TargetRef(pub Bytes32);

/// Descriptor identifier: BLAKE3("descriptor" || canonical_bytes(ProviderDescriptorUnsigned))
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DescriptorId(pub Bytes32);

/// Behavioral Address Hash (observer-local aggregation)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Bah(pub Bytes32);

/// Event identifier: BLAKE3(canonical_bytes(event_without_id))
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EventId(pub Bytes32);

/// Receipt identifier: BLAKE3(canonical_bytes(receipt_without_id))
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ReceiptId(pub Bytes32);

/// Attestation identifier: BLAKE3(canonical_bytes(attestation_without_id))
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AttestationId(pub Bytes32);

/// Challenge identifier (commitment, not literal prompt ID)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ChallengeId(pub Bytes32);

// =============================================================================
// RULE BUNDLE
// =============================================================================

/// World governance configuration (hashed into WorldId)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RuleBundle {
    pub version: u32,
    pub epoch_len_ms: u64,
    pub exploration_rate: f64,
    pub disagreement_quarantine_threshold: f64,
    pub min_diverse_probers: u32,
    pub max_probe_redundancy: u32,
    pub default_circuit_len: u32,
    pub relay_batch_max_delay_ms: u32,
    pub fixed_cell_bytes: u32,
    pub w_success: f64,
    pub w_tool_fidelity: f64,
    pub w_latency: f64,
    pub w_refusal_consistency: f64,
    pub w_robustness: f64,
}

// =============================================================================
// CAPABILITY MANIFEST
// =============================================================================

/// Adapter configuration (LoRA, etc.)
/// Field order: (adapter_type, adapter_id, adapter_digest) for sorting
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Adapter {
    pub adapter_type: String,
    pub adapter_id: String,
    pub adapter_digest: Bytes32,
}

/// Provider capability manifest (hashed to FAH)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CapabilityManifest {
    pub base_model_id: String,
    pub weights_digest: Bytes32,
    pub runtime_id: String,
    pub context_limit: u32,
    pub tool_schemas_digest: Bytes32,
    pub safety_mode: String,
    /// MUST be sorted by (adapter_type, adapter_id, adapter_digest) before hashing
    pub adapters: Vec<Adapter>,
}

// =============================================================================
// PROVIDER DESCRIPTOR
// =============================================================================

/// Capability union for descriptor
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum DescriptorCapability {
    Fah(Fah),
    Manifest(CapabilityManifest),
}

/// Unsigned descriptor content (hashed to produce DescriptorId)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderDescriptorUnsigned {
    pub world: WorldId,
    pub descriptor_epoch: u64,
    /// MUST be sorted lexicographically and deduped before hashing
    pub contact_points: Vec<String>,
    pub capability: DescriptorCapability,
}

/// Signed wrapper (descriptor_id MUST equal computed hash of unsigned)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderDescriptor {
    pub descriptor_id: DescriptorId,
    pub unsigned: ProviderDescriptorUnsigned,
    pub provider_transport_pubkey: Vec<u8>,
    /// Ed25519 signature over ("desc-sig" || world_id || descriptor_id || canonical_bytes(unsigned))
    pub signature: Vec<u8>,
}

// =============================================================================
// PROBE RECEIPT
// =============================================================================

/// Anti-spam / influence ticket (scrypt/argon2 PoW)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProbeTicket {
    pub ticket_bytes: Vec<u8>,
    pub params_n: u32,
    pub params_r: u32,
    pub params_p: u32,
}

/// Proof that a probe occurred (signed, replay-resistant)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProbeReceipt {
    pub receipt_id: ReceiptId,
    pub world: WorldId,
    pub epoch_id: u64,
    pub challenge_id: ChallengeId,
    pub target_ref: TargetRef,
    pub target_fah: Option<Fah>,
    pub outcome_commitment: Bytes32,
    pub ticket: Option<ProbeTicket>,
    pub prober_transport_pubkey: Vec<u8>,
    /// Signature over canonical_bytes(receipt_without_signature)
    pub signature: Vec<u8>,
}

// =============================================================================
// BEHAVIOR ATTESTATION
// =============================================================================

/// Freshness strength for external anchoring
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum FreshnessStrength {
    None = 0,
    Weak = 1,
    Strong = 2,
}

/// Metrics vector from probe evaluation
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MetricsVector {
    pub success_rate: f64,
    pub refusal_consistency: f64,
    pub tool_fidelity: f64,
    pub latency_p50_ms: u32,
    pub latency_p95_ms: u32,
    pub robustness_score: f64,
    pub drift_indicator: f64,
    pub freshness: FreshnessStrength,
}

/// Behavior attestation (metrics report from prober)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BehaviorAttestation {
    pub attestation_id: AttestationId,
    pub world: WorldId,
    pub epoch_id: u64,
    pub challenge_id: ChallengeId,
    pub target_ref: TargetRef,
    pub target_fah: Option<Fah>,
    pub metrics: MetricsVector,
    pub evidence_commitment: Bytes32,
    pub freshness_anchor: Option<Vec<u8>>,
    pub prober_transport_pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

// =============================================================================
// EVENTS
// =============================================================================

/// Dispute event (conflicting attestations or detected fraud)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DisputeEvent {
    pub world: WorldId,
    pub epoch_id: u64,
    pub event_a: EventId,
    pub event_b: EventId,
    pub reason: String,
    pub disputer_transport_pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Link hint (statistical evidence that two TargetRefs are same provider)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LinkHintEvent {
    pub world: WorldId,
    pub epoch_id: u64,
    pub target_a: TargetRef,
    pub target_b: TargetRef,
    pub evidence_commitment: Bytes32,
    pub compatibility_score: f64,
    pub signer_transport_pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Rule bundle endorsement
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RuleEndorsementEvent {
    pub world: WorldId,
    pub epoch_id: u64,
    pub rule_bundle_hash: Bytes32,
    pub weight: f64,
    pub signer_transport_pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Descriptor publication (validity from descriptor signature)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DescriptorPublishEvent {
    pub world: WorldId,
    pub epoch_id: u64,
    pub descriptor: ProviderDescriptor,
}

/// Event type discriminant (matches protobuf EventType)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum EventType {
    Unspecified = 0,
    Receipt = 1,
    Attestation = 2,
    Dispute = 3,
    LinkHint = 4,
    RuleEndorsement = 5,
    Shard = 6,
    Verdict = 7,
    TrainingManifest = 8,
    DescriptorPublish = 9,
}

/// Union of all event bodies
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum EventBody {
    Receipt(ProbeReceipt),
    Attestation(BehaviorAttestation),
    Dispute(DisputeEvent),
    LinkHint(LinkHintEvent),
    RuleEndorsement(RuleEndorsementEvent),
    DescriptorPublish(DescriptorPublishEvent),
    // Shard, Verdict, TrainingManifest omitted (optional plugin)
}

/// Top-level event wrapper (gossip-plane event)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Event {
    /// Unique event identifier
    pub event_id: EventId,
    /// World this event belongs to
    pub world: WorldId,
    /// Epoch at which event was created
    pub epoch_id: u64,
    /// Event type discriminator
    pub event_type: EventType,
    /// Event payload
    pub body: EventBody,
}

// =============================================================================
// DELTA SYNC
// =============================================================================

/// Version vector entry with rotating replica ID
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VersionVectorEntry {
    /// Rotating: BLAKE3("replica" || transport_pubkey || world_id || epoch_id)
    pub replica_id: Bytes32,
    pub counter: u64,
}

// =============================================================================
// CIRCUIT / ONION (inference plane)
// =============================================================================

/// Circuit creation request
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CircuitCreate {
    pub circuit_id: u64,
    pub entry_ephemeral_pubkey: Vec<u8>, // X25519
    pub desired_hops: u32,
}

/// Next hop specification for circuit extension
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum NextHop {
    DescriptorId(DescriptorId),
    DescriptorInline(ProviderDescriptor),
}

/// Circuit extension request
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CircuitExtend {
    pub circuit_id: u64,
    pub next_hop: NextHop,
    pub hop_ephemeral_pubkey: Vec<u8>, // X25519
}

/// Onion cell (AEAD encrypted payload)
/// associated_data = (world_id || circuit_id || seq)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OnionCell {
    pub circuit_id: u64,
    pub seq: u64,
    /// Ciphertext includes AEAD tag; fixed size if RuleBundle.fixed_cell_bytes > 0
    pub ciphertext: Vec<u8>,
}

/// Circuit teardown
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CircuitDestroy {
    pub circuit_id: u64,
    pub reason: String,
}

// =============================================================================
// TERRAIN ADDRESS
// =============================================================================

/// Overlay topology coordinate
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TerrainAddress {
    pub epoch_id: u64,
    pub region_id: u64,
    pub chunk_id: u64,
    pub cell_id: u32,
}
