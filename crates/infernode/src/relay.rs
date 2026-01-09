//! Relay functionality for forwarding onion cells

use crate::onion::{CircuitState, CircuitTable, OnionCell, OnionError, OnionHopKey};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::debug;

/// Relay errors
#[derive(Debug, Error)]
pub enum RelayError {
    #[error("Onion error: {0}")]
    Onion(#[from] OnionError),
    #[error("Unknown circuit")]
    UnknownCircuit,
    #[error("Forward failed")]
    ForwardFailed,
    #[error("Rate limited")]
    RateLimited,
}

/// Incoming cell from the network
#[derive(Debug)]
pub struct IncomingCell {
    /// Source peer
    pub from_peer: [u8; 32],
    /// The onion cell
    pub cell: OnionCell,
    /// Receive timestamp
    pub received_at: Instant,
}

/// Outgoing cell to send
#[derive(Debug)]
pub struct OutgoingCell {
    /// Destination peer
    pub to_peer: [u8; 32],
    /// The onion cell
    pub cell: OnionCell,
}

/// Relay node handler
pub struct Relay {
    /// Our node ID
    node_id: [u8; 32],
    /// Circuit table
    circuits: Arc<CircuitTable>,
    /// Rate limiting state
    rate_limits: RwLock<HashMap<[u8; 32], RateLimitState>>,
    /// Rate limit: cells per minute per peer
    rate_limit_cpm: u32,
    /// Enable relay mode
    enabled: bool,
    /// Statistics
    stats: RwLock<RelayStats>,
}

#[derive(Debug, Clone)]
struct RateLimitState {
    count: u32,
    window_start: Instant,
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self {
            count: 0,
            window_start: Instant::now(),
        }
    }
}

impl Relay {
    pub fn new(node_id: [u8; 32], max_circuits: usize, rate_limit_cpm: u32, enabled: bool) -> Self {
        Self {
            node_id,
            circuits: Arc::new(CircuitTable::new(max_circuits)),
            rate_limits: RwLock::new(HashMap::new()),
            rate_limit_cpm,
            enabled,
            stats: RwLock::new(RelayStats::default()),
        }
    }

    /// Process an incoming cell
    pub fn process_cell(
        &self,
        incoming: IncomingCell,
        hop_key: &OnionHopKey,
    ) -> Result<RelayAction, RelayError> {
        if !self.enabled {
            return Ok(RelayAction::Drop);
        }

        // Rate limit check
        self.check_rate_limit(&incoming.from_peer)?;

        // Get or create circuit state
        let mut cell = incoming.cell;
        
        // Decrypt our layer
        let header = cell.decrypt_layer(hop_key)?;

        // Update stats
        {
            let mut stats = self.stats.write();
            stats.cells_processed += 1;
        }

        if header.is_final {
            // We are the destination - process locally
            debug!(
                "Final destination for circuit {:?}",
                hex::encode(&cell.circuit_id)
            );
            
            {
                let mut stats = self.stats.write();
                stats.cells_delivered += 1;
            }

            Ok(RelayAction::Deliver {
                circuit_id: cell.circuit_id,
                payload: cell.payload,
            })
        } else {
            // Forward to next hop
            debug!(
                "Forwarding circuit {:?} to {:?}",
                hex::encode(&cell.circuit_id),
                hex::encode(&header.next_hop[..8])
            );

            {
                let mut stats = self.stats.write();
                stats.cells_forwarded += 1;
            }

            Ok(RelayAction::Forward {
                to_peer: header.next_hop,
                cell,
            })
        }
    }

    /// Register a new circuit (when we receive a circuit creation request)
    pub fn register_circuit(
        &self,
        circuit_id: [u8; 16],
        hop_key: OnionHopKey,
        prev_hop: [u8; 32],
        next_hop: Option<[u8; 32]>,
    ) -> Result<(), RelayError> {
        let state = CircuitState {
            circuit_id,
            hop_key,
            next_hop,
            prev_hop,
            created_at: Instant::now(),
        };

        self.circuits.register(state)?;
        Ok(())
    }

    /// Check rate limit for a peer
    fn check_rate_limit(&self, peer_id: &[u8; 32]) -> Result<(), RelayError> {
        let now = Instant::now();
        let window = Duration::from_secs(60);

        let mut limits = self.rate_limits.write();
        let state = limits.entry(*peer_id).or_insert(RateLimitState {
            count: 0,
            window_start: now,
        });

        // Reset window if expired
        if now.duration_since(state.window_start) >= window {
            state.count = 0;
            state.window_start = now;
        }

        // Check limit
        if state.count >= self.rate_limit_cpm {
            return Err(RelayError::RateLimited);
        }

        state.count += 1;
        Ok(())
    }

    /// Prune expired circuits
    pub fn prune_circuits(&self, max_age: Duration) -> usize {
        self.circuits.prune_expired(max_age)
    }

    /// Get circuit count
    pub fn circuit_count(&self) -> usize {
        self.circuits.count()
    }

    /// Get relay statistics
    pub fn stats(&self) -> RelayStats {
        self.stats.read().clone()
    }

    /// Is relay enabled?
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Result of processing a cell
#[derive(Debug)]
pub enum RelayAction {
    /// Forward to next hop
    Forward {
        to_peer: [u8; 32],
        cell: OnionCell,
    },
    /// Deliver locally (we are final destination)
    Deliver {
        circuit_id: [u8; 16],
        payload: Vec<u8>,
    },
    /// Drop the cell (relay disabled or error)
    Drop,
}

/// Relay statistics
#[derive(Debug, Clone, Default)]
pub struct RelayStats {
    pub cells_processed: u64,
    pub cells_forwarded: u64,
    pub cells_delivered: u64,
    pub cells_dropped: u64,
    pub rate_limited: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_creation() {
        let relay = Relay::new([0u8; 32], 100, 1000, true);
        assert!(relay.is_enabled());
        assert_eq!(relay.circuit_count(), 0);
    }

    #[test]
    fn test_circuit_registration() {
        let relay = Relay::new([0u8; 32], 100, 1000, true);
        
        let circuit_id = [1u8; 16];
        let hop_key = OnionHopKey::derive([0u8; 32], &[0u8; 32], 0);
        let prev_hop = [2u8; 32];
        let next_hop = Some([3u8; 32]);

        relay.register_circuit(circuit_id, hop_key, prev_hop, next_hop).unwrap();
        assert_eq!(relay.circuit_count(), 1);
    }

    #[test]
    fn test_rate_limiting() {
        let relay = Relay::new([0u8; 32], 100, 3, true);  // 3 cells per minute
        let peer = [1u8; 32];

        // First 3 should pass
        assert!(relay.check_rate_limit(&peer).is_ok());
        assert!(relay.check_rate_limit(&peer).is_ok());
        assert!(relay.check_rate_limit(&peer).is_ok());

        // 4th should fail
        assert!(relay.check_rate_limit(&peer).is_err());
    }
}
