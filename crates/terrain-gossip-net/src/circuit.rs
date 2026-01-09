//! Onion circuit construction and management
//!
//! Implements multi-hop encrypted circuits for private inference routing.

use crate::crypto::{CryptoError, EphemeralKeyExchange, SessionKeys};
use crate::peer::PeerId;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use x25519_dalek::PublicKey as X25519Public;

/// Circuit errors
#[derive(Debug, Error)]
pub enum CircuitError {
    #[error("Circuit not found: {0}")]
    NotFound(u64),
    #[error("Circuit already exists: {0}")]
    AlreadyExists(u64),
    #[error("No hops in circuit")]
    NoHops,
    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("Invalid hop index: {0}")]
    InvalidHopIndex(usize),
}

/// A single hop in the circuit
pub struct CircuitHop {
    /// Peer ID of this hop
    pub peer_id: PeerId,
    /// Session keys for this hop
    pub session_keys: SessionKeys,
}

/// An established circuit
pub struct Circuit {
    /// Circuit ID
    pub id: u64,
    /// Hops in order (first = entry, last = exit)
    pub hops: Vec<CircuitHop>,
    /// Whether this circuit is active
    pub active: bool,
    /// Creation timestamp
    pub created_at: u64,
}

impl Circuit {
    /// Create a new empty circuit
    pub fn new(id: u64) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        Self {
            id,
            hops: Vec::new(),
            active: true,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Add a hop to the circuit
    pub fn add_hop(&mut self, hop: CircuitHop) {
        self.hops.push(hop);
    }

    /// Number of hops
    pub fn len(&self) -> usize {
        self.hops.len()
    }

    /// Is the circuit empty
    pub fn is_empty(&self) -> bool {
        self.hops.is_empty()
    }

    /// Encrypt payload with all layers (onion wrap)
    pub fn encrypt_onion(&mut self, plaintext: &[u8], circuit_id: u64, seq: u64) -> Result<Vec<u8>, CircuitError> {
        let mut payload = plaintext.to_vec();

        // Build associated data
        let mut aad = Vec::with_capacity(16);
        aad.extend_from_slice(&circuit_id.to_le_bytes());
        aad.extend_from_slice(&seq.to_le_bytes());

        // Encrypt from inside out (last hop first)
        for hop in self.hops.iter_mut().rev() {
            payload = hop.session_keys.encrypt(&payload, &aad)?;
        }

        Ok(payload)
    }

    /// Decrypt one layer of the onion (as a relay)
    pub fn decrypt_layer(
        &self,
        hop_index: usize,
        ciphertext: &[u8],
        circuit_id: u64,
        seq: u64,
    ) -> Result<Vec<u8>, CircuitError> {
        if hop_index >= self.hops.len() {
            return Err(CircuitError::InvalidHopIndex(hop_index));
        }

        let mut aad = Vec::with_capacity(16);
        aad.extend_from_slice(&circuit_id.to_le_bytes());
        aad.extend_from_slice(&seq.to_le_bytes());

        let hop = &self.hops[hop_index];
        hop.session_keys
            .decrypt(ciphertext, &aad, seq)
            .map_err(CircuitError::Crypto)
    }
}

/// Builder for constructing circuits
pub struct CircuitBuilder {
    circuit_id: u64,
    hops: Vec<CircuitHop>,
}

impl CircuitBuilder {
    /// Create a new circuit builder
    pub fn new(circuit_id: u64) -> Self {
        Self {
            circuit_id,
            hops: Vec::new(),
        }
    }

    /// Add a hop via key exchange
    pub fn add_hop(
        mut self,
        peer_id: PeerId,
        their_public: &[u8; 32],
        context: &[u8],
    ) -> Result<(Self, [u8; 32]), CircuitError> {
        let exchange = EphemeralKeyExchange::new();
        let our_public = exchange.public_key();
        let shared = exchange.exchange(their_public);

        let our_x25519_pub = X25519Public::from(our_public);
        let their_x25519_pub = X25519Public::from(*their_public);

        let session_keys = SessionKeys::derive(&shared, &our_x25519_pub, &their_x25519_pub, context)?;

        self.hops.push(CircuitHop {
            peer_id,
            session_keys,
        });

        Ok((self, our_public))
    }

    /// Build the circuit
    pub fn build(self) -> Result<Circuit, CircuitError> {
        if self.hops.is_empty() {
            return Err(CircuitError::NoHops);
        }

        Ok(Circuit {
            id: self.circuit_id,
            hops: self.hops,
            active: true,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })
    }
}

/// Circuit manager for tracking active circuits
pub struct CircuitManager {
    circuits: RwLock<HashMap<u64, Arc<RwLock<Circuit>>>>,
    next_id: RwLock<u64>,
}

impl CircuitManager {
    /// Create a new circuit manager
    pub fn new() -> Self {
        Self {
            circuits: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    /// Allocate a new circuit ID
    pub fn allocate_id(&self) -> u64 {
        let mut next = self.next_id.write();
        let id = *next;
        *next += 1;
        id
    }

    /// Register a circuit
    pub fn register(&self, circuit: Circuit) -> Result<Arc<RwLock<Circuit>>, CircuitError> {
        let id = circuit.id;
        let circuit = Arc::new(RwLock::new(circuit));

        let mut circuits = self.circuits.write();
        if circuits.contains_key(&id) {
            return Err(CircuitError::AlreadyExists(id));
        }

        circuits.insert(id, Arc::clone(&circuit));
        Ok(circuit)
    }

    /// Get a circuit by ID
    pub fn get(&self, id: u64) -> Option<Arc<RwLock<Circuit>>> {
        self.circuits.read().get(&id).cloned()
    }

    /// Remove a circuit
    pub fn remove(&self, id: u64) -> Option<Arc<RwLock<Circuit>>> {
        self.circuits.write().remove(&id)
    }

    /// List all active circuit IDs
    pub fn list_active(&self) -> Vec<u64> {
        self.circuits
            .read()
            .iter()
            .filter(|(_, c)| c.read().active)
            .map(|(id, _)| *id)
            .collect()
    }
}

impl Default for CircuitManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_manager() {
        let manager = CircuitManager::new();

        let id1 = manager.allocate_id();
        let id2 = manager.allocate_id();
        assert_ne!(id1, id2);

        let circuit = Circuit::new(id1);
        manager.register(circuit).unwrap();

        assert!(manager.get(id1).is_some());
        assert!(manager.get(id2).is_none());
    }
}
