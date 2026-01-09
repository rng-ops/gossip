//! Delta-state CRDT synchronization protocol

use crate::event_log::{EventLog, EventLogError};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use terrain_gossip_core::types::*;
use thiserror::Error;

/// Sync protocol errors
#[derive(Debug, Error)]
pub enum SyncError {
    #[error("Event log error: {0}")]
    EventLog(#[from] EventLogError),
    #[error("Peer not found: {0:?}")]
    PeerNotFound([u8; 32]),
    #[error("Sync timeout")]
    Timeout,
    #[error("Channel closed")]
    ChannelClosed,
}

/// Delta sync request message
#[derive(Debug, Clone)]
pub struct DeltaSyncRequest {
    /// Sender's version vector
    pub version_vector: Vec<VersionVectorEntry>,
    /// Maximum events to receive
    pub max_events: u32,
}

/// Delta sync response message
#[derive(Debug, Clone)]
pub struct DeltaSyncResponse {
    /// Events the sender has that receiver lacks
    pub events: Vec<Event>,
    /// Sender's current version vector
    pub version_vector: Vec<VersionVectorEntry>,
    /// Whether there are more events available
    pub has_more: bool,
}

/// Anti-entropy sync state for a peer
#[derive(Debug)]
pub struct PeerSyncState {
    /// Peer's last known version vector
    pub last_version: Vec<VersionVectorEntry>,
    /// Last sync timestamp
    pub last_sync: Instant,
    /// Number of sync rounds
    pub sync_count: u64,
    /// Consecutive failures
    pub failures: u32,
}

impl Default for PeerSyncState {
    fn default() -> Self {
        Self {
            last_version: Vec::new(),
            last_sync: Instant::now(),
            sync_count: 0,
            failures: 0,
        }
    }
}

/// Synchronization manager
pub struct SyncManager {
    event_log: Arc<EventLog>,
    /// Peer sync states
    peers: RwLock<std::collections::HashMap<[u8; 32], PeerSyncState>>,
    /// Sync interval
    interval: Duration,
    /// Maximum batch size
    max_batch: usize,
}

impl SyncManager {
    pub fn new(event_log: Arc<EventLog>, interval: Duration, max_batch: usize) -> Self {
        Self {
            event_log,
            peers: RwLock::new(std::collections::HashMap::new()),
            interval,
            max_batch,
        }
    }

    /// Register a peer for synchronization
    pub fn register_peer(&self, peer_id: [u8; 32]) {
        let mut peers = self.peers.write();
        peers.entry(peer_id).or_insert_with(PeerSyncState::default);
    }

    /// Remove a peer from synchronization
    pub fn unregister_peer(&self, peer_id: &[u8; 32]) {
        self.peers.write().remove(peer_id);
    }

    /// Handle incoming sync request
    pub fn handle_request(&self, request: DeltaSyncRequest) -> Result<DeltaSyncResponse, SyncError> {
        // Compute delta based on peer's version vector
        let events = self.event_log.compute_delta(&request.version_vector)?;
        
        let limited_events: Vec<Event> = events
            .into_iter()
            .take(request.max_events as usize)
            .collect();

        let has_more = limited_events.len() >= request.max_events as usize;

        Ok(DeltaSyncResponse {
            events: limited_events,
            version_vector: self.event_log.get_version_vector(),
            has_more,
        })
    }

    /// Process incoming sync response
    pub fn handle_response(
        &self,
        peer_id: [u8; 32],
        response: DeltaSyncResponse,
    ) -> Result<usize, SyncError> {
        let mut merged_count = 0;

        // Merge each event
        for event in response.events {
            if self.event_log.merge(event, peer_id)? {
                merged_count += 1;
            }
        }

        // Update peer state
        {
            let mut peers = self.peers.write();
            if let Some(state) = peers.get_mut(&peer_id) {
                state.last_version = response.version_vector;
                state.last_sync = Instant::now();
                state.sync_count += 1;
                state.failures = 0;
            }
        }

        Ok(merged_count)
    }

    /// Create a sync request for a peer
    pub fn create_request(&self, _peer_id: &[u8; 32]) -> DeltaSyncRequest {
        DeltaSyncRequest {
            version_vector: self.event_log.get_version_vector(),
            max_events: self.max_batch as u32,
        }
    }

    /// Get peers that need synchronization
    pub fn peers_needing_sync(&self) -> Vec<[u8; 32]> {
        let now = Instant::now();
        self.peers
            .read()
            .iter()
            .filter(|(_, state)| now.duration_since(state.last_sync) >= self.interval)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Mark peer sync as failed
    pub fn mark_failure(&self, peer_id: &[u8; 32]) {
        let mut peers = self.peers.write();
        if let Some(state) = peers.get_mut(peer_id) {
            state.failures += 1;
        }
    }

    /// Get sync statistics
    pub fn stats(&self) -> SyncStats {
        let peers = self.peers.read();
        SyncStats {
            peer_count: peers.len(),
            total_syncs: peers.values().map(|s| s.sync_count).sum(),
            event_count: self.event_log.event_count(),
        }
    }
}

/// Sync statistics
#[derive(Debug, Clone)]
pub struct SyncStats {
    pub peer_count: usize,
    pub total_syncs: u64,
    pub event_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;
    use tempfile::tempdir;

    fn create_test_manager() -> (SyncManager, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let storage = Arc::new(Storage::open(dir.path()).unwrap());
        let event_log = Arc::new(EventLog::new(
            storage,
            WorldId([0; 32]),
            [1; 32],
        ));
        (
            SyncManager::new(event_log, Duration::from_secs(30), 100),
            dir,
        )
    }

    #[test]
    fn test_peer_registration() {
        let (manager, _dir) = create_test_manager();
        let peer_id = [2; 32];

        manager.register_peer(peer_id);
        assert!(manager.peers.read().contains_key(&peer_id));

        manager.unregister_peer(&peer_id);
        assert!(!manager.peers.read().contains_key(&peer_id));
    }

    #[test]
    fn test_sync_request_response() {
        let (manager, _dir) = create_test_manager();

        let request = manager.create_request(&[2; 32]);
        assert_eq!(request.max_events, 100);

        let response = manager.handle_request(request).unwrap();
        assert!(response.events.is_empty());
        assert!(!response.has_more);
    }
}
