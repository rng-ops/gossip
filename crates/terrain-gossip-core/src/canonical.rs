//! Canonical encoding for TerrainGossip (RFC-0001 §3.1)
//!
//! All hashed/signed objects use postcard serialization with strict constraints:
//! - No maps/hashmaps
//! - Floats must be finite and normalized (-0.0 → +0.0)
//! - Repeated fields must be sorted and deduped
//! - Field order is Rust struct field order

use crate::error::{Error, Result};
use crate::types::*;
use serde::Serialize;

/// Serialize a value to canonical bytes using postcard.
///
/// This is the normative encoding for all hashing and signing operations.
/// Implementations in other languages MUST produce identical bytes.
pub fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    postcard::to_allocvec(value).map_err(Error::from)
}

/// Normalize a float for canonical encoding.
/// - NaN and Inf are rejected
/// - -0.0 is converted to +0.0
pub fn normalize_f64(v: f64) -> Result<f64> {
    if v.is_nan() {
        return Err(Error::FloatNormalization("NaN not allowed".into()));
    }
    if v.is_infinite() {
        return Err(Error::FloatNormalization("Infinity not allowed".into()));
    }
    // Convert -0.0 to +0.0
    if v == 0.0 && v.is_sign_negative() {
        return Ok(0.0);
    }
    Ok(v)
}

/// Validate and normalize a RuleBundle for canonical encoding.
pub fn normalize_rule_bundle(bundle: &mut RuleBundle) -> Result<()> {
    bundle.exploration_rate = normalize_f64(bundle.exploration_rate)?;
    bundle.disagreement_quarantine_threshold =
        normalize_f64(bundle.disagreement_quarantine_threshold)?;
    bundle.w_success = normalize_f64(bundle.w_success)?;
    bundle.w_tool_fidelity = normalize_f64(bundle.w_tool_fidelity)?;
    bundle.w_latency = normalize_f64(bundle.w_latency)?;
    bundle.w_refusal_consistency = normalize_f64(bundle.w_refusal_consistency)?;
    bundle.w_robustness = normalize_f64(bundle.w_robustness)?;
    Ok(())
}

/// Validate and normalize a MetricsVector for canonical encoding.
pub fn normalize_metrics_vector(metrics: &mut MetricsVector) -> Result<()> {
    metrics.success_rate = normalize_f64(metrics.success_rate)?;
    metrics.refusal_consistency = normalize_f64(metrics.refusal_consistency)?;
    metrics.tool_fidelity = normalize_f64(metrics.tool_fidelity)?;
    metrics.robustness_score = normalize_f64(metrics.robustness_score)?;
    metrics.drift_indicator = normalize_f64(metrics.drift_indicator)?;
    Ok(())
}

/// Validate and normalize a CapabilityManifest for canonical encoding.
/// Sorts adapters by (adapter_type, adapter_id, adapter_digest).
pub fn normalize_capability_manifest(manifest: &mut CapabilityManifest) -> Result<()> {
    // Sort adapters by (adapter_type, adapter_id, adapter_digest)
    manifest.adapters.sort();
    // Dedup (in case of duplicates)
    manifest.adapters.dedup();
    Ok(())
}

/// Validate and normalize a ProviderDescriptorUnsigned for canonical encoding.
/// Sorts and dedupes contact_points lexicographically.
pub fn normalize_descriptor_unsigned(desc: &mut ProviderDescriptorUnsigned) -> Result<()> {
    // Sort contact points lexicographically
    desc.contact_points.sort();
    // Dedup
    desc.contact_points.dedup();

    // If capability is a manifest, normalize it too
    if let DescriptorCapability::Manifest(ref mut manifest) = desc.capability {
        normalize_capability_manifest(manifest)?;
    }
    Ok(())
}

/// Check that contact_points are properly sorted and deduped.
pub fn validate_contact_points_sorted(points: &[String]) -> Result<()> {
    for i in 1..points.len() {
        if points[i] <= points[i - 1] {
            return Err(Error::UnsortedRepeatedField {
                field: "contact_points".into(),
            });
        }
    }
    Ok(())
}

/// Check that adapters are properly sorted.
pub fn validate_adapters_sorted(adapters: &[Adapter]) -> Result<()> {
    for i in 1..adapters.len() {
        if adapters[i] <= adapters[i - 1] {
            return Err(Error::UnsortedRepeatedField {
                field: "adapters".into(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_normalization() {
        assert_eq!(normalize_f64(1.5).unwrap(), 1.5);
        assert_eq!(normalize_f64(0.0).unwrap(), 0.0);
        assert_eq!(normalize_f64(-0.0).unwrap(), 0.0); // -0.0 → +0.0
        assert!(normalize_f64(f64::NAN).is_err());
        assert!(normalize_f64(f64::INFINITY).is_err());
        assert!(normalize_f64(f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn test_canonical_bytes_deterministic() {
        let addr = TerrainAddress {
            epoch_id: 42,
            region_id: 100,
            chunk_id: 200,
            cell_id: 5,
        };

        let bytes1 = canonical_bytes(&addr).unwrap();
        let bytes2 = canonical_bytes(&addr).unwrap();
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_contact_points_sorting() {
        let points = vec!["b".into(), "a".into(), "c".into()];
        assert!(validate_contact_points_sorted(&points).is_err());

        let sorted = vec!["a".into(), "b".into(), "c".into()];
        assert!(validate_contact_points_sorted(&sorted).is_ok());
    }
}
