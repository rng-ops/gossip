//! Test vectors for cross-language validation (RFC-0001 §3.1, §14.1)
//!
//! These vectors MUST be reproduced exactly by TypeScript implementation.

use crate::canonical::canonical_bytes;
use crate::crypto::*;
use crate::types::*;
use serde::Serialize;

/// Test vector output format (JSON serializable)
#[derive(Serialize)]
pub struct TestVector {
    pub name: String,
    pub description: String,
    pub inputs: serde_json::Value,
    pub canonical_bytes_hex: String,
    pub hash_hex: String,
}

/// Generate all test vectors as JSON
pub fn generate_test_vectors() -> Vec<TestVector> {
    vec![
        world_id_vector(),
        rule_bundle_hash_vector(),
        fah_vector(),
        descriptor_id_vector(),
        target_ref_vector(),
        terrain_address_vector(),
    ]
}

fn world_id_vector() -> TestVector {
    let bundle = RuleBundle {
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
    };

    let phrase = "test world alpha";
    let world_id = derive_world_id(phrase, &bundle).unwrap();
    let rule_hash = rule_bundle_hash(&bundle).unwrap();

    TestVector {
        name: "world_id_derivation".into(),
        description: "WorldId = BLAKE3(\"world\" || phrase_norm || rule_bundle_hash)".into(),
        inputs: serde_json::json!({
            "phrase": phrase,
            "phrase_normalized": normalize_world_phrase(phrase),
            "rule_bundle": {
                "version": bundle.version,
                "epoch_len_ms": bundle.epoch_len_ms,
                "exploration_rate": bundle.exploration_rate,
                "disagreement_quarantine_threshold": bundle.disagreement_quarantine_threshold,
                "min_diverse_probers": bundle.min_diverse_probers,
                "max_probe_redundancy": bundle.max_probe_redundancy,
                "default_circuit_len": bundle.default_circuit_len,
                "relay_batch_max_delay_ms": bundle.relay_batch_max_delay_ms,
                "fixed_cell_bytes": bundle.fixed_cell_bytes,
                "w_success": bundle.w_success,
                "w_tool_fidelity": bundle.w_tool_fidelity,
                "w_latency": bundle.w_latency,
                "w_refusal_consistency": bundle.w_refusal_consistency,
                "w_robustness": bundle.w_robustness,
            },
            "rule_bundle_hash_hex": hex::encode(rule_hash),
        }),
        canonical_bytes_hex: hex::encode(canonical_bytes(&bundle).unwrap()),
        hash_hex: hex::encode(world_id.0),
    }
}

fn rule_bundle_hash_vector() -> TestVector {
    let bundle = RuleBundle {
        version: 1,
        epoch_len_ms: 60_000,
        exploration_rate: 0.05,
        disagreement_quarantine_threshold: 0.3,
        min_diverse_probers: 5,
        max_probe_redundancy: 20,
        default_circuit_len: 3,
        relay_batch_max_delay_ms: 50,
        fixed_cell_bytes: 1024,
        w_success: 2.0,
        w_tool_fidelity: 1.0,
        w_latency: 0.5,
        w_refusal_consistency: 0.3,
        w_robustness: 0.8,
    };

    let bytes = canonical_bytes(&bundle).unwrap();
    let hash = blake3::hash(&bytes);

    TestVector {
        name: "rule_bundle_hash".into(),
        description: "BLAKE3(canonical_bytes(RuleBundle))".into(),
        inputs: serde_json::json!({
            "version": bundle.version,
            "epoch_len_ms": bundle.epoch_len_ms,
            "exploration_rate": bundle.exploration_rate,
        }),
        canonical_bytes_hex: hex::encode(&bytes),
        hash_hex: hex::encode(hash.as_bytes()),
    }
}

fn fah_vector() -> TestVector {
    let manifest = CapabilityManifest {
        base_model_id: "llama-3.3-70b-instruct".into(),
        weights_digest: [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
            0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
        ],
        runtime_id: "vllm".into(),
        context_limit: 128_000,
        tool_schemas_digest: [0xaa; 32],
        safety_mode: "standard".into(),
        adapters: vec![
            Adapter {
                adapter_type: "lora".into(),
                adapter_id: "coding-v1".into(),
                adapter_digest: [0xbb; 32],
            },
        ],
    };

    let fah = derive_fah(&manifest).unwrap();
    let bytes = canonical_bytes(&manifest).unwrap();

    TestVector {
        name: "fah_derivation".into(),
        description: "FAH = BLAKE3(\"fah\" || canonical_bytes(CapabilityManifest))".into(),
        inputs: serde_json::json!({
            "base_model_id": manifest.base_model_id,
            "weights_digest_hex": hex::encode(manifest.weights_digest),
            "runtime_id": manifest.runtime_id,
            "context_limit": manifest.context_limit,
            "adapters": [{
                "adapter_type": "lora",
                "adapter_id": "coding-v1",
                "adapter_digest_hex": hex::encode([0xbb; 32]),
            }],
        }),
        canonical_bytes_hex: hex::encode(&bytes),
        hash_hex: hex::encode(fah.0),
    }
}

fn descriptor_id_vector() -> TestVector {
    let unsigned = ProviderDescriptorUnsigned {
        world: WorldId([0x42; 32]),
        descriptor_epoch: 100,
        contact_points: vec![
            "/dns4/node1.example.com/tcp/9000".into(),
            "/ip4/192.168.1.1/tcp/9000".into(),
        ],
        capability: DescriptorCapability::Fah(Fah([0x55; 32])),
    };

    let descriptor_id = derive_descriptor_id(&unsigned).unwrap();
    let bytes = canonical_bytes(&unsigned).unwrap();

    TestVector {
        name: "descriptor_id_derivation".into(),
        description: "DescriptorId = BLAKE3(\"descriptor\" || canonical_bytes(ProviderDescriptorUnsigned))".into(),
        inputs: serde_json::json!({
            "world_hex": hex::encode([0x42; 32]),
            "descriptor_epoch": 100,
            "contact_points": [
                "/dns4/node1.example.com/tcp/9000",
                "/ip4/192.168.1.1/tcp/9000",
            ],
            "capability": {
                "type": "fah",
                "fah_hex": hex::encode([0x55; 32]),
            },
        }),
        canonical_bytes_hex: hex::encode(&bytes),
        hash_hex: hex::encode(descriptor_id.0),
    }
}

fn target_ref_vector() -> TestVector {
    let master_key = [0x11; 32];
    let world_id = WorldId([0x22; 32]);
    let epoch_id = 42u64;
    let descriptor_id = DescriptorId([0x33; 32]);

    let cpk = derive_control_plane_key(&master_key, &world_id, epoch_id);
    let target_ref = derive_target_ref(&cpk, &world_id, epoch_id, &descriptor_id);

    TestVector {
        name: "target_ref_derivation".into(),
        description: "TargetRef = BLAKE3_KEYED(cpk, \"targetref\" || WorldId || epoch_id || DescriptorId)".into(),
        inputs: serde_json::json!({
            "master_key_hex": hex::encode(master_key),
            "world_id_hex": hex::encode(world_id.0),
            "epoch_id": epoch_id,
            "descriptor_id_hex": hex::encode(descriptor_id.0),
            "control_plane_key_hex": hex::encode(cpk),
        }),
        canonical_bytes_hex: "".into(), // N/A for keyed hash
        hash_hex: hex::encode(target_ref.0),
    }
}

fn terrain_address_vector() -> TestVector {
    let addr = TerrainAddress {
        epoch_id: 12345,
        region_id: 67890,
        chunk_id: 11111,
        cell_id: 42,
    };

    let bytes = canonical_bytes(&addr).unwrap();
    let hash = blake3::hash(&bytes);

    TestVector {
        name: "terrain_address_canonical".into(),
        description: "Canonical bytes for TerrainAddress".into(),
        inputs: serde_json::json!({
            "epoch_id": addr.epoch_id,
            "region_id": addr.region_id,
            "chunk_id": addr.chunk_id,
            "cell_id": addr.cell_id,
        }),
        canonical_bytes_hex: hex::encode(&bytes),
        hash_hex: hex::encode(hash.as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_vectors() {
        let vectors = generate_test_vectors();
        assert!(!vectors.is_empty());

        // Print JSON for manual inspection / export
        let json = serde_json::to_string_pretty(&vectors).unwrap();
        println!("Test Vectors:\n{}", json);
    }

    #[test]
    fn test_world_id_deterministic() {
        let v1 = world_id_vector();
        let v2 = world_id_vector();
        assert_eq!(v1.hash_hex, v2.hash_hex);
    }

    #[test]
    fn test_fah_deterministic() {
        let v1 = fah_vector();
        let v2 = fah_vector();
        assert_eq!(v1.hash_hex, v2.hash_hex);
    }
}
