//! QUIC-based transport layer
//!
//! Provides secure, multiplexed connections between nodes.

use crate::crypto::KeyPair;
use crate::framing::{Frame, FrameError};
use crate::peer::{PeerId, PeerInfo};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Transport errors
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("QUIC error: {0}")]
    Quic(String),
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Frame error: {0}")]
    Frame(#[from] FrameError),
    #[error("Peer not found: {0}")]
    PeerNotFound(PeerId),
    #[error("Already connected to peer: {0}")]
    AlreadyConnected(PeerId),
}

/// Connection to a peer
pub struct Connection {
    /// Peer info
    pub peer: PeerInfo,
    /// Send channel
    tx: mpsc::Sender<Frame>,
    /// Is the connection open
    open: Arc<RwLock<bool>>,
}

impl Connection {
    /// Send a frame to this peer
    pub async fn send(&self, frame: Frame) -> Result<(), TransportError> {
        if !*self.open.read() {
            return Err(TransportError::ConnectionClosed);
        }
        self.tx
            .send(frame)
            .await
            .map_err(|_| TransportError::ConnectionClosed)
    }

    /// Check if connection is open
    pub fn is_open(&self) -> bool {
        *self.open.read()
    }

    /// Close the connection
    pub fn close(&self) {
        *self.open.write() = false;
    }
}

/// Event from the transport layer
#[derive(Debug)]
pub enum TransportEvent {
    /// New peer connected
    PeerConnected(PeerId),
    /// Peer disconnected
    PeerDisconnected(PeerId),
    /// Frame received from peer
    FrameReceived { from: PeerId, frame: Frame },
}

/// Transport layer managing connections
pub struct Transport {
    /// Our keypair
    keypair: KeyPair,
    /// Our peer ID
    our_id: PeerId,
    /// Listen address
    listen_addr: SocketAddr,
    /// Connected peers
    connections: RwLock<HashMap<PeerId, Arc<Connection>>>,
    /// Known peers (may not be connected)
    known_peers: RwLock<HashMap<PeerId, PeerInfo>>,
    /// Frame codec settings
    fixed_cell_bytes: usize,
}

impl Transport {
    /// Create a new transport
    pub fn new(keypair: KeyPair, listen_addr: SocketAddr) -> Self {
        let our_id = PeerId::from_public_key(&keypair.public_key());
        Self {
            keypair,
            our_id,
            listen_addr,
            connections: RwLock::new(HashMap::new()),
            known_peers: RwLock::new(HashMap::new()),
            fixed_cell_bytes: 0,
        }
    }

    /// Set fixed cell size for circuit cells
    pub fn with_fixed_cells(mut self, size: usize) -> Self {
        self.fixed_cell_bytes = size;
        self
    }

    /// Get our peer ID
    pub fn our_id(&self) -> PeerId {
        self.our_id
    }

    /// Get our public key
    pub fn public_key(&self) -> [u8; 32] {
        self.keypair.public_key()
    }

    /// Add a known peer
    pub fn add_peer(&self, peer: PeerInfo) {
        self.known_peers.write().insert(peer.id, peer);
    }

    /// Get a known peer
    pub fn get_peer(&self, id: &PeerId) -> Option<PeerInfo> {
        self.known_peers.read().get(id).cloned()
    }

    /// List connected peers
    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.connections.read().keys().copied().collect()
    }

    /// Check if connected to a peer
    pub fn is_connected(&self, id: &PeerId) -> bool {
        self.connections
            .read()
            .get(id)
            .map(|c| c.is_open())
            .unwrap_or(false)
    }

    /// Get a connection to a peer
    pub fn get_connection(&self, id: &PeerId) -> Option<Arc<Connection>> {
        self.connections.read().get(id).cloned()
    }

    /// Send a frame to a peer
    pub async fn send(&self, to: &PeerId, frame: Frame) -> Result<(), TransportError> {
        let conn = self
            .connections
            .read()
            .get(to)
            .cloned()
            .ok_or_else(|| TransportError::PeerNotFound(*to))?;

        conn.send(frame).await
    }

    /// Broadcast a frame to all connected peers
    pub async fn broadcast(&self, frame: Frame) {
        let connections: Vec<_> = self.connections.read().values().cloned().collect();
        for conn in connections {
            if let Err(e) = conn.send(frame.clone()).await {
                warn!("Failed to broadcast to {}: {}", conn.peer.id, e);
            }
        }
    }

    /// Start the transport layer (placeholder for QUIC implementation)
    pub async fn run(
        self: Arc<Self>,
        event_tx: mpsc::Sender<TransportEvent>,
    ) -> Result<(), TransportError> {
        info!("Transport listening on {}", self.listen_addr);

        // TODO: Implement QUIC server/client
        // For now, this is a placeholder that shows the API
        // Real implementation would:
        // 1. Create QUIC endpoint
        // 2. Accept incoming connections
        // 3. Handle connection establishment with key exchange
        // 4. Spawn tasks for reading/writing frames
        // 5. Send events via event_tx

        // Keep running
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }
}

/// Message serialization helpers
pub mod messages {
    use super::*;
    use serde::{Deserialize, Serialize};
    use terrain_gossip_core::types::*;

    /// Delta sync request message
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct DeltaSyncRequest {
        pub world: WorldId,
        pub since: Vec<VersionVectorEntry>,
        pub max_events: u32,
    }

    /// Delta sync response message
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct DeltaSyncResponse {
        pub world: WorldId,
        pub events: Vec<Event>,
        pub now: Vec<VersionVectorEntry>,
    }

    /// Descriptor query message
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct DescriptorQuery {
        pub world: WorldId,
        pub descriptor_id: DescriptorId,
    }

    /// Descriptor response message
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct DescriptorResponse {
        pub descriptor: Option<ProviderDescriptor>,
    }

    impl DeltaSyncRequest {
        pub fn to_frame(&self) -> Result<Frame, postcard::Error> {
            let payload = postcard::to_allocvec(self)?;
            Ok(Frame::new(crate::framing::FrameType::DeltaSyncRequest, payload))
        }

        pub fn from_frame(frame: &Frame) -> Result<Self, postcard::Error> {
            postcard::from_bytes(&frame.payload)
        }
    }

    impl DeltaSyncResponse {
        pub fn to_frame(&self) -> Result<Frame, postcard::Error> {
            let payload = postcard::to_allocvec(self)?;
            Ok(Frame::new(crate::framing::FrameType::DeltaSyncResponse, payload))
        }

        pub fn from_frame(frame: &Frame) -> Result<Self, postcard::Error> {
            postcard::from_bytes(&frame.payload)
        }
    }
}
