//! Peer identity and information

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Unique peer identifier (derived from transport public key)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub [u8; 32]);

impl PeerId {
    /// Create from transport public key
    pub fn from_public_key(public_key: &[u8; 32]) -> Self {
        // PeerId is just the public key for now
        Self(*public_key)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0[..8]))
    }
}

/// Information about a peer
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer identifier
    pub id: PeerId,
    /// Network addresses
    pub addrs: Vec<SocketAddr>,
    /// Transport public key
    pub transport_pubkey: [u8; 32],
    /// Roles this peer serves
    pub roles: PeerRoles,
    /// Last seen timestamp (unix millis)
    pub last_seen: u64,
}

/// Roles a peer can serve
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct PeerRoles {
    /// Can relay inference traffic
    pub relay: bool,
    /// Serves gossip/delta sync
    pub gossipd: bool,
    /// Routes client requests
    pub router: bool,
    /// Runs probes
    pub prober: bool,
    /// Provides inference
    pub provider: bool,
}

impl PeerInfo {
    /// Create new peer info
    pub fn new(transport_pubkey: [u8; 32], addrs: Vec<SocketAddr>) -> Self {
        Self {
            id: PeerId::from_public_key(&transport_pubkey),
            addrs,
            transport_pubkey,
            roles: PeerRoles::default(),
            last_seen: 0,
        }
    }

    /// Update last seen time
    pub fn touch(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        self.last_seen = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
    }
}
