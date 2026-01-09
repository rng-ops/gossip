//! Cryptographic hash derivations for TerrainGossip (RFC-0001 §3)
//!
//! All hash derivations use BLAKE3 with domain separation prefixes.
//! This module provides the normative implementations.

use crate::canonical::{canonical_bytes, normalize_descriptor_unsigned};
use crate::error::{Error, Result};
use crate::types::*;
use blake3::Hasher;

// =============================================================================
// DOMAIN SEPARATION PREFIXES
// =============================================================================

/// Domain prefix for WorldId derivation
pub const DOMAIN_WORLD: &[u8] = b"world";
/// Domain prefix for FAH derivation
pub const DOMAIN_FAH: &[u8] = b"fah";
/// Domain prefix for DescriptorId derivation
pub const DOMAIN_DESCRIPTOR: &[u8] = b"descriptor";
/// Domain prefix for TargetRef derivation
pub const DOMAIN_TARGETREF: &[u8] = b"targetref";
/// Domain prefix for Handle derivation (local only)
pub const DOMAIN_HANDLE: &[u8] = b"handle";
/// Domain prefix for BAH derivation
pub const DOMAIN_BAH: &[u8] = b"bah";
/// Domain prefix for replica ID derivation
pub const DOMAIN_REPLICA: &[u8] = b"replica";
/// Domain prefix for control-plane key derivation
pub const DOMAIN_CPK: &[u8] = b"cpk";
/// Domain prefix for descriptor signature
pub const DOMAIN_DESC_SIG: &[u8] = b"desc-sig";

// =============================================================================
// WORLD IDENTITY
// =============================================================================

/// Normalize a world phrase: lowercase, trim, collapse whitespace to single hyphen.
pub fn normalize_world_phrase(phrase: &str) -> String {
    phrase
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// Derive WorldId from phrase and rule bundle.
///
/// `WorldId = BLAKE3("world" || phrase_norm || rule_bundle_hash)`
pub fn derive_world_id(phrase: &str, rule_bundle: &RuleBundle) -> Result<WorldId> {
    let phrase_norm = normalize_world_phrase(phrase);
    let rule_bundle_bytes = canonical_bytes(rule_bundle)?;
    let rule_bundle_hash = blake3::hash(&rule_bundle_bytes);

    let mut hasher = Hasher::new();
    hasher.update(DOMAIN_WORLD);
    hasher.update(phrase_norm.as_bytes());
    hasher.update(rule_bundle_hash.as_bytes());

    Ok(WorldId(*hasher.finalize().as_bytes()))
}

/// Compute rule bundle hash (used in WorldId derivation).
pub fn rule_bundle_hash(rule_bundle: &RuleBundle) -> Result<Bytes32> {
    let bytes = canonical_bytes(rule_bundle)?;
    Ok(*blake3::hash(&bytes).as_bytes())
}

// =============================================================================
// FAH (Functional Address Hash)
// =============================================================================

/// Derive FAH from CapabilityManifest.
///
/// `FAH = BLAKE3("fah" || canonical_bytes(CapabilityManifest))`
///
/// Note: The manifest should be normalized (adapters sorted) before calling.
pub fn derive_fah(manifest: &CapabilityManifest) -> Result<Fah> {
    let manifest_bytes = canonical_bytes(manifest)?;

    let mut hasher = Hasher::new();
    hasher.update(DOMAIN_FAH);
    hasher.update(&manifest_bytes);

    Ok(Fah(*hasher.finalize().as_bytes()))
}

// =============================================================================
// DESCRIPTOR ID
// =============================================================================

/// Derive DescriptorId from unsigned descriptor.
///
/// `DescriptorId = BLAKE3("descriptor" || canonical_bytes(ProviderDescriptorUnsigned))`
///
/// Note: The descriptor should be normalized (contact_points sorted) before calling.
pub fn derive_descriptor_id(unsigned: &ProviderDescriptorUnsigned) -> Result<DescriptorId> {
    let bytes = canonical_bytes(unsigned)?;

    let mut hasher = Hasher::new();
    hasher.update(DOMAIN_DESCRIPTOR);
    hasher.update(&bytes);

    Ok(DescriptorId(*hasher.finalize().as_bytes()))
}

/// Compute the bytes to sign for a descriptor signature.
///
/// `sign_bytes = "desc-sig" || world_id || descriptor_id || canonical_bytes(unsigned)`
pub fn descriptor_sign_bytes(
    world_id: &WorldId,
    descriptor_id: &DescriptorId,
    unsigned: &ProviderDescriptorUnsigned,
) -> Result<Vec<u8>> {
    let unsigned_bytes = canonical_bytes(unsigned)?;

    let mut bytes = Vec::with_capacity(
        DOMAIN_DESC_SIG.len() + 32 + 32 + unsigned_bytes.len(),
    );
    bytes.extend_from_slice(DOMAIN_DESC_SIG);
    bytes.extend_from_slice(&world_id.0);
    bytes.extend_from_slice(&descriptor_id.0);
    bytes.extend_from_slice(&unsigned_bytes);

    Ok(bytes)
}

/// Verify that a ProviderDescriptor's descriptor_id matches computed value.
pub fn verify_descriptor_id(descriptor: &ProviderDescriptor) -> Result<()> {
    let computed = derive_descriptor_id(&descriptor.unsigned)?;
    if computed != descriptor.descriptor_id {
        return Err(Error::HashMismatch {
            computed: hex::encode(computed.0),
            transmitted: hex::encode(descriptor.descriptor_id.0),
        });
    }
    Ok(())
}

// =============================================================================
// TARGET REF (Control-Plane, Provider-Blind)
// =============================================================================

/// Derive epoch-specific control-plane key.
///
/// `control_plane_key(world, epoch) = BLAKE3_KEYED(master_key, "cpk" || world_id || epoch_id)`
pub fn derive_control_plane_key(
    master_key: &Bytes32,
    world_id: &WorldId,
    epoch_id: u64,
) -> Bytes32 {
    let mut hasher = blake3::Hasher::new_keyed(master_key);
    hasher.update(DOMAIN_CPK);
    hasher.update(&world_id.0);
    hasher.update(&epoch_id.to_le_bytes());
    *hasher.finalize().as_bytes()
}

/// Derive TargetRef from descriptor and control-plane key.
///
/// `TargetRef = BLAKE3_KEYED(control_plane_key, "targetref" || WorldId || epoch_id || DescriptorId)`
pub fn derive_target_ref(
    control_plane_key: &Bytes32,
    world_id: &WorldId,
    epoch_id: u64,
    descriptor_id: &DescriptorId,
) -> TargetRef {
    let mut hasher = blake3::Hasher::new_keyed(control_plane_key);
    hasher.update(DOMAIN_TARGETREF);
    hasher.update(&world_id.0);
    hasher.update(&epoch_id.to_le_bytes());
    hasher.update(&descriptor_id.0);
    TargetRef(*hasher.finalize().as_bytes())
}

// =============================================================================
// HANDLE (Local Only)
// =============================================================================

/// Derive observer-local handle.
///
/// `Handle = BLAKE3("handle" || observer_secret || observed_fingerprint)`
///
/// Note: This is strictly local and MUST NOT appear in gossiped events.
pub fn derive_handle(observer_secret: &Bytes32, observed_fingerprint: &Bytes32) -> Handle {
    let mut hasher = Hasher::new();
    hasher.update(DOMAIN_HANDLE);
    hasher.update(observer_secret);
    hasher.update(observed_fingerprint);
    Handle(*hasher.finalize().as_bytes())
}

// =============================================================================
// REPLICA ID (Rotating)
// =============================================================================

/// Derive rotating replica ID for delta sync.
///
/// `replica_id = BLAKE3("replica" || transport_pubkey || world_id || epoch_id)`
pub fn derive_replica_id(
    transport_pubkey: &[u8],
    world_id: &WorldId,
    epoch_id: u64,
) -> Bytes32 {
    let mut hasher = Hasher::new();
    hasher.update(DOMAIN_REPLICA);
    hasher.update(transport_pubkey);
    hasher.update(&world_id.0);
    hasher.update(&epoch_id.to_le_bytes());
    *hasher.finalize().as_bytes()
}

// =============================================================================
// EVENT / RECEIPT / ATTESTATION IDs
// =============================================================================

/// Compute EventId from event body (without the event_id field).
pub fn compute_event_id<T: serde::Serialize>(body: &T) -> Result<EventId> {
    let bytes = canonical_bytes(body)?;
    Ok(EventId(*blake3::hash(&bytes).as_bytes()))
}

/// Compute ReceiptId from receipt (without receipt_id and signature).
pub fn compute_receipt_id(receipt: &ProbeReceipt) -> Result<ReceiptId> {
    // Create a version without receipt_id and signature for hashing
    #[derive(serde::Serialize)]
    struct ReceiptHashable<'a> {
        world: &'a WorldId,
        epoch_id: u64,
        challenge_id: &'a ChallengeId,
        target_ref: &'a TargetRef,
        target_fah: &'a Option<Fah>,
        outcome_commitment: &'a Bytes32,
        ticket: &'a Option<ProbeTicket>,
        prober_transport_pubkey: &'a [u8],
    }

    let hashable = ReceiptHashable {
        world: &receipt.world,
        epoch_id: receipt.epoch_id,
        challenge_id: &receipt.challenge_id,
        target_ref: &receipt.target_ref,
        target_fah: &receipt.target_fah,
        outcome_commitment: &receipt.outcome_commitment,
        ticket: &receipt.ticket,
        prober_transport_pubkey: &receipt.prober_transport_pubkey,
    };

    let bytes = canonical_bytes(&hashable)?;
    Ok(ReceiptId(*blake3::hash(&bytes).as_bytes()))
}

/// Verify that a ProbeReceipt's receipt_id matches computed value.
pub fn verify_receipt_id(receipt: &ProbeReceipt) -> Result<()> {
    let computed = compute_receipt_id(receipt)?;
    if computed != receipt.receipt_id {
        return Err(Error::HashMismatch {
            computed: hex::encode(computed.0),
            transmitted: hex::encode(receipt.receipt_id.0),
        });
    }
    Ok(())
}

// =============================================================================
// CONVENIENCE: Create normalized descriptor
// =============================================================================

/// Create a ProviderDescriptor with computed descriptor_id.
/// Normalizes the unsigned content (sorts contact_points, adapters).
pub fn create_provider_descriptor(
    mut unsigned: ProviderDescriptorUnsigned,
    provider_transport_pubkey: Vec<u8>,
    sign_fn: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Result<ProviderDescriptor> {
    // Normalize before hashing
    normalize_descriptor_unsigned(&mut unsigned)?;

    // Compute descriptor_id
    let descriptor_id = derive_descriptor_id(&unsigned)?;

    // Compute signature bytes
    let sign_bytes = descriptor_sign_bytes(&unsigned.world, &descriptor_id, &unsigned)?;
    let signature = sign_fn(&sign_bytes);

    Ok(ProviderDescriptor {
        descriptor_id,
        unsigned,
        provider_transport_pubkey,
        signature,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rule_bundle() -> RuleBundle {
        RuleBundle {
            version: 1,
            epoch_len_ms: 300_000,
            exploration_rate: 0.1,
            disagreement_quarantine_threshold: 0.5,
            min_diverse_probers: 3,
            max_probe_redundancy: 10,
            default_circuit_len: 3,
            relay_batch_max_delay_ms: 100,
            fixed_cell_bytes: 512,
            w_success: 1.0,
            w_tool_fidelity: 0.5,
            w_latency: 0.3,
            w_refusal_consistency: 0.2,
            w_robustness: 0.4,
        }
    }

    #[test]
    fn test_world_phrase_normalization() {
        assert_eq!(normalize_world_phrase("  Hello World  "), "hello-world");
        assert_eq!(normalize_world_phrase("ONE"), "one");
        assert_eq!(normalize_world_phrase("a  b   c"), "a-b-c");
    }

    #[test]
    fn test_world_id_derivation() {
        let bundle = test_rule_bundle();
        let world_id = derive_world_id("test world", &bundle).unwrap();

        // Should be deterministic
        let world_id2 = derive_world_id("test world", &bundle).unwrap();
        assert_eq!(world_id, world_id2);

        // Different phrase = different world
        let world_id3 = derive_world_id("other world", &bundle).unwrap();
        assert_ne!(world_id, world_id3);
    }

    #[test]
    fn test_fah_derivation() {
        let manifest = CapabilityManifest {
            base_model_id: "llama-3.3-70b".into(),
            weights_digest: [0u8; 32],
            runtime_id: "vllm".into(),
            context_limit: 128_000,
            tool_schemas_digest: [0u8; 32],
            safety_mode: "standard".into(),
            adapters: vec![],
        };

        let fah = derive_fah(&manifest).unwrap();
        let fah2 = derive_fah(&manifest).unwrap();
        assert_eq!(fah, fah2);
    }

    #[test]
    fn test_target_ref_requires_key() {
        let master_key = [1u8; 32];
        let world_id = WorldId([2u8; 32]);
        let descriptor_id = DescriptorId([3u8; 32]);
        let epoch_id = 100;

        let cpk = derive_control_plane_key(&master_key, &world_id, epoch_id);
        let target_ref = derive_target_ref(&cpk, &world_id, epoch_id, &descriptor_id);

        // Different master key = different target ref
        let other_master = [99u8; 32];
        let other_cpk = derive_control_plane_key(&other_master, &world_id, epoch_id);
        let other_ref = derive_target_ref(&other_cpk, &world_id, epoch_id, &descriptor_id);

        assert_ne!(target_ref, other_ref);
    }

    #[test]
    fn test_replica_id_rotation() {
        let pubkey = [1u8; 32];
        let world_id = WorldId([2u8; 32]);

        let r1 = derive_replica_id(&pubkey, &world_id, 1);
        let r2 = derive_replica_id(&pubkey, &world_id, 2);

        // Different epoch = different replica ID
        assert_ne!(r1, r2);
    }
}
