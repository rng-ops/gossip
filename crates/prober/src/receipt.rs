//! Probe receipts and attestations

use crate::challenge::{Challenge, VerificationResult};
use blake3::Hasher;
use std::time::{SystemTime, UNIX_EPOCH};
use terrain_gossip_core::types::*;

/// A probe receipt documenting a probe attempt (local representation)
#[derive(Debug, Clone)]
pub struct ProbeReceipt {
    /// Unique receipt ID
    pub receipt_id: [u8; 32],
    /// Challenge that was issued
    pub challenge_id: [u8; 32],
    /// Target provider
    pub provider_id: [u8; 32],
    /// Prober's public key
    pub prober_pubkey: [u8; 32],
    /// Whether the probe passed
    pub passed: bool,
    /// Verification details
    pub token_ratio: f64,
    pub latency_secs: u64,
    /// Timestamp of receipt creation
    pub timestamp: u64,
    /// Prober's signature over receipt
    pub prober_signature: Vec<u8>,
    /// Provider's counter-signature (if any)
    pub provider_signature: Option<Vec<u8>>,
}

impl ProbeReceipt {
    /// Create a new probe receipt
    pub fn new(
        challenge: &Challenge,
        result: &VerificationResult,
        prober_pubkey: [u8; 32],
    ) -> Self {
        // Generate receipt ID
        let mut hasher = Hasher::new();
        hasher.update(&challenge.id);
        hasher.update(&result.challenge_hash);
        hasher.update(&result.response_hash);
        let receipt_id = *hasher.finalize().as_bytes();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            receipt_id,
            challenge_id: challenge.id,
            provider_id: challenge.target_provider,
            prober_pubkey,
            passed: result.passed,
            token_ratio: result.token_ratio,
            latency_secs: result.latency_secs,
            timestamp,
            prober_signature: Vec::new(),
            provider_signature: None,
        }
    }

    /// Compute the receipt hash for signing
    pub fn receipt_hash(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(&self.receipt_id);
        hasher.update(&self.challenge_id);
        hasher.update(&self.provider_id);
        hasher.update(&self.prober_pubkey);
        hasher.update(&[self.passed as u8]);
        hasher.update(&self.token_ratio.to_le_bytes());
        hasher.update(&self.latency_secs.to_le_bytes());
        hasher.update(&self.timestamp.to_le_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Convert to a core protocol ProbeReceipt for gossip propagation
    pub fn to_core_receipt(&self, world_id: WorldId, epoch_id: u64) -> terrain_gossip_core::types::ProbeReceipt {
        terrain_gossip_core::types::ProbeReceipt {
            receipt_id: ReceiptId(self.receipt_id),
            world: world_id,
            epoch_id,
            challenge_id: ChallengeId(self.challenge_id),
            target_ref: TargetRef(self.provider_id),
            target_fah: None,
            outcome_commitment: self.receipt_hash(),
            ticket: None,
            prober_transport_pubkey: self.prober_pubkey.to_vec(),
            signature: self.prober_signature.clone(),
        }
    }

    /// Convert to an Event for gossip propagation
    pub fn to_event(&self, world_id: WorldId, epoch_id: u64) -> Event {
        let core_receipt = self.to_core_receipt(world_id, epoch_id);
        Event {
            event_id: EventId(self.receipt_id),
            world: world_id,
            epoch_id,
            event_type: EventType::Receipt,
            body: EventBody::Receipt(core_receipt),
        }
    }
}

/// Attestation from a trusted prober
#[derive(Debug, Clone)]
pub struct ProberAttestation {
    /// Prober's public key
    pub prober_pubkey: [u8; 32],
    /// Summary statistics
    pub total_probes: u64,
    pub passed_probes: u64,
    pub avg_latency_secs: f64,
    /// Time period covered
    pub period_start: u64,
    pub period_end: u64,
    /// Signature over attestation
    pub signature: Vec<u8>,
}

impl ProberAttestation {
    /// Create from a collection of receipts
    pub fn from_receipts(
        prober_pubkey: [u8; 32],
        receipts: &[ProbeReceipt],
    ) -> Self {
        let total_probes = receipts.len() as u64;
        let passed_probes = receipts.iter().filter(|r| r.passed).count() as u64;
        
        let avg_latency_secs = if receipts.is_empty() {
            0.0
        } else {
            receipts.iter().map(|r| r.latency_secs as f64).sum::<f64>()
                / receipts.len() as f64
        };

        let period_start = receipts.iter().map(|r| r.timestamp).min().unwrap_or(0);
        let period_end = receipts.iter().map(|r| r.timestamp).max().unwrap_or(0);

        Self {
            prober_pubkey,
            total_probes,
            passed_probes,
            avg_latency_secs,
            period_start,
            period_end,
            signature: Vec::new(),
        }
    }

    /// Compute attestation hash for signing
    pub fn attestation_hash(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(&self.prober_pubkey);
        hasher.update(&self.total_probes.to_le_bytes());
        hasher.update(&self.passed_probes.to_le_bytes());
        hasher.update(&self.avg_latency_secs.to_le_bytes());
        hasher.update(&self.period_start.to_le_bytes());
        hasher.update(&self.period_end.to_le_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Pass rate as a percentage
    pub fn pass_rate(&self) -> f64 {
        if self.total_probes == 0 {
            0.0
        } else {
            self.passed_probes as f64 / self.total_probes as f64
        }
    }
}

/// Receipt storage interface
pub trait ReceiptStore {
    /// Store a receipt
    fn store(&self, receipt: ProbeReceipt) -> Result<(), String>;
    
    /// Get receipts for a provider
    fn get_for_provider(&self, provider_id: &[u8; 32]) -> Vec<ProbeReceipt>;
    
    /// Get receipts in a time range
    fn get_in_range(&self, start: u64, end: u64) -> Vec<ProbeReceipt>;
    
    /// Get recent receipts
    fn get_recent(&self, count: usize) -> Vec<ProbeReceipt>;
}

/// In-memory receipt store for testing
pub struct MemoryReceiptStore {
    receipts: parking_lot::RwLock<Vec<ProbeReceipt>>,
}

impl MemoryReceiptStore {
    pub fn new() -> Self {
        Self {
            receipts: parking_lot::RwLock::new(Vec::new()),
        }
    }
}

impl Default for MemoryReceiptStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiptStore for MemoryReceiptStore {
    fn store(&self, receipt: ProbeReceipt) -> Result<(), String> {
        self.receipts.write().push(receipt);
        Ok(())
    }

    fn get_for_provider(&self, provider_id: &[u8; 32]) -> Vec<ProbeReceipt> {
        self.receipts
            .read()
            .iter()
            .filter(|r| &r.provider_id == provider_id)
            .cloned()
            .collect()
    }

    fn get_in_range(&self, start: u64, end: u64) -> Vec<ProbeReceipt> {
        self.receipts
            .read()
            .iter()
            .filter(|r| r.timestamp >= start && r.timestamp <= end)
            .cloned()
            .collect()
    }

    fn get_recent(&self, count: usize) -> Vec<ProbeReceipt> {
        let receipts = self.receipts.read();
        let skip = receipts.len().saturating_sub(count);
        receipts.iter().skip(skip).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge::Challenge;

    #[test]
    fn test_receipt_creation() {
        let challenge = Challenge::generate([1u8; 32], 5, 300);
        let result = crate::challenge::VerificationResult {
            passed: true,
            token_ratio: 1.0,
            latency_secs: 5,
            challenge_hash: [0; 32],
            response_hash: [0; 32],
        };

        let receipt = ProbeReceipt::new(&challenge, &result, [2u8; 32]);
        assert!(receipt.passed);
        assert_eq!(receipt.provider_id, [1u8; 32]);
    }

    #[test]
    fn test_attestation() {
        let challenge = Challenge::generate([1u8; 32], 5, 300);
        let result = crate::challenge::VerificationResult {
            passed: true,
            token_ratio: 1.0,
            latency_secs: 5,
            challenge_hash: [0; 32],
            response_hash: [0; 32],
        };

        let receipt = ProbeReceipt::new(&challenge, &result, [2u8; 32]);
        let attestation = ProberAttestation::from_receipts([2u8; 32], &[receipt]);

        assert_eq!(attestation.total_probes, 1);
        assert_eq!(attestation.pass_rate(), 1.0);
    }

    #[test]
    fn test_memory_store() {
        let store = MemoryReceiptStore::new();
        
        let challenge = Challenge::generate([1u8; 32], 5, 300);
        let result = crate::challenge::VerificationResult {
            passed: true,
            token_ratio: 1.0,
            latency_secs: 5,
            challenge_hash: [0; 32],
            response_hash: [0; 32],
        };
        let receipt = ProbeReceipt::new(&challenge, &result, [2u8; 32]);

        store.store(receipt).unwrap();

        let receipts = store.get_for_provider(&[1u8; 32]);
        assert_eq!(receipts.len(), 1);
    }
}
