//! Onion routing implementation

use blake3::Hasher;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use thiserror::Error;

/// Onion routing errors
#[derive(Debug, Error)]
pub enum OnionError {
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Decryption failed")]
    DecryptionFailed,
    #[error("Invalid layer")]
    InvalidLayer,
    #[error("No more hops")]
    NoMoreHops,
    #[error("Circuit not found")]
    CircuitNotFound,
}

/// Onion layer header (fixed size for padding)
#[derive(Debug, Clone)]
pub struct OnionHeader {
    /// Next hop address (or final destination marker)
    pub next_hop: [u8; 32],
    /// Is this the final hop?
    pub is_final: bool,
    /// Padding to fixed size
    pub padding: [u8; 31],
}

impl OnionHeader {
    pub const SIZE: usize = 64;

    pub fn new(next_hop: [u8; 32], is_final: bool) -> Self {
        Self {
            next_hop,
            is_final,
            padding: [0; 31],
        }
    }

    pub fn final_destination(destination: [u8; 32]) -> Self {
        Self::new(destination, true)
    }

    pub fn relay(next_hop: [u8; 32]) -> Self {
        Self::new(next_hop, false)
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[..32].copy_from_slice(&self.next_hop);
        bytes[32] = self.is_final as u8;
        bytes[33..64].copy_from_slice(&self.padding);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, OnionError> {
        if bytes.len() < Self::SIZE {
            return Err(OnionError::InvalidLayer);
        }
        let mut next_hop = [0u8; 32];
        next_hop.copy_from_slice(&bytes[..32]);
        let is_final = bytes[32] != 0;
        let mut padding = [0u8; 31];
        padding.copy_from_slice(&bytes[33..64]);
        Ok(Self {
            next_hop,
            is_final,
            padding,
        })
    }
}

/// Onion-encrypted cell
#[derive(Debug, Clone)]
pub struct OnionCell {
    /// Circuit ID
    pub circuit_id: [u8; 16],
    /// Encrypted layers (outermost first)
    pub payload: Vec<u8>,
    /// Current hop index (for tracking)
    pub hop_index: u8,
}

impl OnionCell {
    /// Encrypt a payload for a path of hops
    pub fn encrypt(
        circuit_id: [u8; 16],
        payload: &[u8],
        hop_keys: &[OnionHopKey],
    ) -> Result<Self, OnionError> {
        // Encrypt from innermost to outermost
        let mut current = payload.to_vec();

        for (i, hop_key) in hop_keys.iter().rev().enumerate() {
            let is_final = i == 0;
            let next_hop = if is_final {
                hop_key.peer_id
            } else {
                hop_keys[hop_keys.len() - i].peer_id
            };

            // Create header
            let header = OnionHeader::new(next_hop, is_final);
            let mut plaintext = header.to_bytes().to_vec();
            plaintext.extend_from_slice(&current);

            // Encrypt layer
            let cipher = ChaCha20Poly1305::new_from_slice(&hop_key.session_key)
                .map_err(|_| OnionError::EncryptionFailed)?;
            let nonce = Nonce::from_slice(&hop_key.nonce);
            current = cipher
                .encrypt(nonce, plaintext.as_ref())
                .map_err(|_| OnionError::EncryptionFailed)?;
        }

        Ok(Self {
            circuit_id,
            payload: current,
            hop_index: 0,
        })
    }

    /// Decrypt one layer
    pub fn decrypt_layer(&mut self, key: &OnionHopKey) -> Result<OnionHeader, OnionError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&key.session_key)
            .map_err(|_| OnionError::DecryptionFailed)?;
        let nonce = Nonce::from_slice(&key.nonce);

        let plaintext = cipher
            .decrypt(nonce, self.payload.as_ref())
            .map_err(|_| OnionError::DecryptionFailed)?;

        if plaintext.len() < OnionHeader::SIZE {
            return Err(OnionError::InvalidLayer);
        }

        let header = OnionHeader::from_bytes(&plaintext[..OnionHeader::SIZE])?;
        self.payload = plaintext[OnionHeader::SIZE..].to_vec();
        self.hop_index += 1;

        Ok(header)
    }
}

/// Session key for one hop
#[derive(Debug, Clone)]
pub struct OnionHopKey {
    /// Peer ID for this hop
    pub peer_id: [u8; 32],
    /// Session key (derived from ECDH)
    pub session_key: [u8; 32],
    /// Nonce for this hop
    pub nonce: [u8; 12],
}

impl OnionHopKey {
    /// Derive hop key from shared secret
    pub fn derive(peer_id: [u8; 32], shared_secret: &[u8], hop_index: u8) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(b"terrain-gossip-hop-key-v1:");
        hasher.update(shared_secret);
        hasher.update(&[hop_index]);
        
        let hash = hasher.finalize();
        let mut session_key = [0u8; 32];
        session_key.copy_from_slice(hash.as_bytes());

        // Derive nonce
        let mut hasher = Hasher::new();
        hasher.update(b"terrain-gossip-hop-nonce-v1:");
        hasher.update(&session_key);
        let nonce_hash = hasher.finalize();
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&nonce_hash.as_bytes()[..12]);

        Self {
            peer_id,
            session_key,
            nonce,
        }
    }
}

/// Circuit state at a relay node
#[derive(Debug)]
pub struct CircuitState {
    /// Circuit ID
    pub circuit_id: [u8; 16],
    /// Our hop key for this circuit
    pub hop_key: OnionHopKey,
    /// Next hop address (if not final)
    pub next_hop: Option<[u8; 32]>,
    /// Previous hop address (for responses)
    pub prev_hop: [u8; 32],
    /// Created timestamp
    pub created_at: std::time::Instant,
}

/// Circuit table for relay nodes
pub struct CircuitTable {
    circuits: RwLock<HashMap<[u8; 16], CircuitState>>,
    max_circuits: usize,
}

impl CircuitTable {
    pub fn new(max_circuits: usize) -> Self {
        Self {
            circuits: RwLock::new(HashMap::new()),
            max_circuits,
        }
    }

    /// Register a new circuit
    pub fn register(&self, state: CircuitState) -> Result<(), OnionError> {
        let mut circuits = self.circuits.write();
        if circuits.len() >= self.max_circuits {
            // Evict oldest
            if let Some(oldest_id) = circuits
                .iter()
                .min_by_key(|(_, s)| s.created_at)
                .map(|(id, _)| *id)
            {
                circuits.remove(&oldest_id);
            }
        }
        circuits.insert(state.circuit_id, state);
        Ok(())
    }

    /// Get circuit state
    pub fn get(&self, circuit_id: &[u8; 16]) -> Option<CircuitState> {
        // Can't clone CircuitState directly, so we return relevant info
        let circuits = self.circuits.read();
        circuits.get(circuit_id).map(|s| CircuitState {
            circuit_id: s.circuit_id,
            hop_key: s.hop_key.clone(),
            next_hop: s.next_hop,
            prev_hop: s.prev_hop,
            created_at: s.created_at,
        })
    }

    /// Remove a circuit
    pub fn remove(&self, circuit_id: &[u8; 16]) -> bool {
        self.circuits.write().remove(circuit_id).is_some()
    }

    /// Count active circuits
    pub fn count(&self) -> usize {
        self.circuits.read().len()
    }

    /// Prune expired circuits
    pub fn prune_expired(&self, max_age: std::time::Duration) -> usize {
        let now = std::time::Instant::now();
        let mut circuits = self.circuits.write();
        let before = circuits.len();
        circuits.retain(|_, s| now.duration_since(s.created_at) < max_age);
        before - circuits.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onion_header() {
        let header = OnionHeader::relay([1u8; 32]);
        let bytes = header.to_bytes();
        let parsed = OnionHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.next_hop, [1u8; 32]);
        assert!(!parsed.is_final);
    }

    #[test]
    fn test_hop_key_derivation() {
        let peer_id = [1u8; 32];
        let secret = [2u8; 32];
        
        let key1 = OnionHopKey::derive(peer_id, &secret, 0);
        let key2 = OnionHopKey::derive(peer_id, &secret, 1);

        assert_ne!(key1.session_key, key2.session_key);
    }

    #[test]
    fn test_circuit_table() {
        let table = CircuitTable::new(10);
        
        let state = CircuitState {
            circuit_id: [1u8; 16],
            hop_key: OnionHopKey::derive([0u8; 32], &[0u8; 32], 0),
            next_hop: Some([2u8; 32]),
            prev_hop: [3u8; 32],
            created_at: std::time::Instant::now(),
        };

        table.register(state).unwrap();
        assert_eq!(table.count(), 1);

        assert!(table.get(&[1u8; 16]).is_some());
        assert!(table.remove(&[1u8; 16]));
        assert_eq!(table.count(), 0);
    }
}
