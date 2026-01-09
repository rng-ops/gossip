//! Circuit management for inference requests

use crate::onion::{OnionCell, OnionError, OnionHopKey};
use parking_lot::RwLock;
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Circuit errors
#[derive(Debug, Error)]
pub enum CircuitError {
    #[error("Onion error: {0}")]
    Onion(#[from] OnionError),
    #[error("Circuit not found")]
    NotFound,
    #[error("Circuit expired")]
    Expired,
    #[error("No path available")]
    NoPath,
    #[error("Build failed")]
    BuildFailed,
}

/// Circuit status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitStatus {
    /// Building the circuit
    Building,
    /// Ready for use
    Ready,
    /// Circuit has failed
    Failed,
    /// Circuit is closing
    Closing,
}

/// Client-side circuit
#[derive(Debug)]
pub struct ClientCircuit {
    /// Circuit ID
    pub id: [u8; 16],
    /// Path of hops
    pub path: Vec<[u8; 32]>,
    /// Session keys for each hop
    pub hop_keys: Vec<OnionHopKey>,
    /// Current status
    pub status: CircuitStatus,
    /// Creation time
    pub created_at: Instant,
    /// Last use time
    pub last_used: Instant,
    /// Request counter
    pub request_count: u64,
}

impl ClientCircuit {
    /// Create a new circuit
    pub fn new(path: Vec<[u8; 32]>, hop_keys: Vec<OnionHopKey>) -> Self {
        let mut id = [0u8; 16];
        rand::thread_rng().fill(&mut id);

        Self {
            id,
            path,
            hop_keys,
            status: CircuitStatus::Building,
            created_at: Instant::now(),
            last_used: Instant::now(),
            request_count: 0,
        }
    }

    /// Encrypt a request for this circuit
    pub fn encrypt_request(&mut self, payload: &[u8]) -> Result<OnionCell, CircuitError> {
        if self.status != CircuitStatus::Ready {
            return Err(CircuitError::BuildFailed);
        }

        let cell = OnionCell::encrypt(self.id, payload, &self.hop_keys)?;
        self.last_used = Instant::now();
        self.request_count += 1;

        Ok(cell)
    }

    /// Check if circuit is expired
    pub fn is_expired(&self, max_age: Duration) -> bool {
        self.created_at.elapsed() > max_age
    }

    /// Check if circuit is idle
    pub fn is_idle(&self, idle_timeout: Duration) -> bool {
        self.last_used.elapsed() > idle_timeout
    }
}

/// Circuit manager for client-side circuits
pub struct CircuitManager {
    /// Active circuits
    circuits: RwLock<HashMap<[u8; 16], ClientCircuit>>,
    /// Maximum circuits
    max_circuits: usize,
    /// Circuit timeout
    timeout: Duration,
}

impl CircuitManager {
    pub fn new(max_circuits: usize, timeout_secs: u64) -> Self {
        Self {
            circuits: RwLock::new(HashMap::new()),
            max_circuits,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Create a new circuit
    pub fn create_circuit(
        &self,
        path: Vec<[u8; 32]>,
        hop_keys: Vec<OnionHopKey>,
    ) -> Result<[u8; 16], CircuitError> {
        let circuit = ClientCircuit::new(path, hop_keys);
        let id = circuit.id;

        let mut circuits = self.circuits.write();
        
        // Check capacity
        if circuits.len() >= self.max_circuits {
            // Try to evict an idle circuit
            let idle_id = circuits
                .iter()
                .filter(|(_, c)| c.is_idle(Duration::from_secs(60)))
                .min_by_key(|(_, c)| c.last_used)
                .map(|(id, _)| *id);

            if let Some(evict_id) = idle_id {
                circuits.remove(&evict_id);
            } else {
                return Err(CircuitError::NoPath);
            }
        }

        circuits.insert(id, circuit);
        Ok(id)
    }

    /// Mark circuit as ready
    pub fn mark_ready(&self, id: &[u8; 16]) -> Result<(), CircuitError> {
        let mut circuits = self.circuits.write();
        let circuit = circuits.get_mut(id).ok_or(CircuitError::NotFound)?;
        circuit.status = CircuitStatus::Ready;
        Ok(())
    }

    /// Encrypt a request
    pub fn encrypt_request(
        &self,
        circuit_id: &[u8; 16],
        payload: &[u8],
    ) -> Result<OnionCell, CircuitError> {
        let mut circuits = self.circuits.write();
        let circuit = circuits.get_mut(circuit_id).ok_or(CircuitError::NotFound)?;

        if circuit.is_expired(self.timeout) {
            circuit.status = CircuitStatus::Failed;
            return Err(CircuitError::Expired);
        }

        circuit.encrypt_request(payload)
    }

    /// Close a circuit
    pub fn close_circuit(&self, id: &[u8; 16]) -> bool {
        self.circuits.write().remove(id).is_some()
    }

    /// Get circuit info
    pub fn get_circuit_info(&self, id: &[u8; 16]) -> Option<CircuitInfo> {
        self.circuits.read().get(id).map(|c| CircuitInfo {
            id: c.id,
            hops: c.path.len(),
            status: c.status,
            age_secs: c.created_at.elapsed().as_secs(),
            requests: c.request_count,
        })
    }

    /// Prune expired and idle circuits
    pub fn prune(&self) -> usize {
        let mut circuits = self.circuits.write();
        let before = circuits.len();
        
        circuits.retain(|_, c| {
            !c.is_expired(self.timeout) && !c.is_idle(Duration::from_secs(300))
        });

        before - circuits.len()
    }

    /// Get statistics
    pub fn stats(&self) -> CircuitStats {
        let circuits = self.circuits.read();
        CircuitStats {
            total: circuits.len(),
            ready: circuits.values().filter(|c| c.status == CircuitStatus::Ready).count(),
            building: circuits.values().filter(|c| c.status == CircuitStatus::Building).count(),
            failed: circuits.values().filter(|c| c.status == CircuitStatus::Failed).count(),
        }
    }
}

/// Circuit information
#[derive(Debug, Clone)]
pub struct CircuitInfo {
    pub id: [u8; 16],
    pub hops: usize,
    pub status: CircuitStatus,
    pub age_secs: u64,
    pub requests: u64,
}

/// Circuit statistics
#[derive(Debug, Clone)]
pub struct CircuitStats {
    pub total: usize,
    pub ready: usize,
    pub building: usize,
    pub failed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_hop_keys() -> Vec<OnionHopKey> {
        vec![
            OnionHopKey::derive([1u8; 32], &[0u8; 32], 0),
            OnionHopKey::derive([2u8; 32], &[0u8; 32], 1),
            OnionHopKey::derive([3u8; 32], &[0u8; 32], 2),
        ]
    }

    #[test]
    fn test_circuit_creation() {
        let manager = CircuitManager::new(10, 300);
        
        let path = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        let keys = test_hop_keys();

        let id = manager.create_circuit(path, keys).unwrap();
        
        let info = manager.get_circuit_info(&id).unwrap();
        assert_eq!(info.hops, 3);
        assert_eq!(info.status, CircuitStatus::Building);
    }

    #[test]
    fn test_circuit_lifecycle() {
        let manager = CircuitManager::new(10, 300);
        
        let path = vec![[1u8; 32], [2u8; 32]];
        let keys = vec![
            OnionHopKey::derive([1u8; 32], &[0u8; 32], 0),
            OnionHopKey::derive([2u8; 32], &[0u8; 32], 1),
        ];

        let id = manager.create_circuit(path, keys).unwrap();
        manager.mark_ready(&id).unwrap();

        let info = manager.get_circuit_info(&id).unwrap();
        assert_eq!(info.status, CircuitStatus::Ready);

        assert!(manager.close_circuit(&id));
        assert!(manager.get_circuit_info(&id).is_none());
    }
}
