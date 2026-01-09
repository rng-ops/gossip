//! Append-only event log with version vectors

use crate::storage::{Storage, StorageError};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use terrain_gossip_core::types::*;
use thiserror::Error;

/// Event log errors
#[derive(Debug, Error)]
pub enum EventLogError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Duplicate event: {0:?}")]
    DuplicateEvent(EventId),
    #[error("Invalid event signature")]
    InvalidSignature,
    #[error("World mismatch")]
    WorldMismatch,
}

/// Append-only event log with delta-state CRDT semantics
pub struct EventLog {
    storage: Arc<Storage>,
    world_id: WorldId,
    /// Local replica ID (our node's identity)
    replica_id: [u8; 32],
    /// Cached version vector
    version_vector: RwLock<HashMap<[u8; 32], u64>>,
}

impl EventLog {
    /// Create a new event log
    pub fn new(storage: Arc<Storage>, world_id: WorldId, replica_id: [u8; 32]) -> Self {
        // Load existing version vector from storage
        let mut vv = HashMap::new();
        if let Ok(entries) = storage.get_all_versions() {
            for entry in entries {
                vv.insert(entry.replica_id, entry.counter);
            }
        }

        Self {
            storage,
            world_id,
            replica_id,
            version_vector: RwLock::new(vv),
        }
    }

    /// Append a new event locally
    pub fn append(&self, event: Event) -> Result<(), EventLogError> {
        // Validate world
        if event.world.0 != self.world_id.0 {
            return Err(EventLogError::WorldMismatch);
        }

        // Check for duplicate
        if self.storage.has_event(&event.event_id)? {
            return Err(EventLogError::DuplicateEvent(event.event_id));
        }

        // Store event
        self.storage.put_event(&event)?;

        // Update version vector
        let mut vv = self.version_vector.write();
        let counter = vv.entry(self.replica_id).or_insert(0);
        *counter += 1;
        self.storage.put_version(&self.replica_id, *counter)?;

        Ok(())
    }

    /// Merge a remote event (from delta sync)
    pub fn merge(&self, event: Event, source_replica: [u8; 32]) -> Result<bool, EventLogError> {
        // Validate world
        if event.world.0 != self.world_id.0 {
            return Err(EventLogError::WorldMismatch);
        }

        // Skip if already present
        if self.storage.has_event(&event.event_id)? {
            return Ok(false);
        }

        // Store event
        self.storage.put_event(&event)?;

        // Update source's version in our vector
        let mut vv = self.version_vector.write();
        let counter = vv.entry(source_replica).or_insert(0);
        *counter += 1;
        self.storage.put_version(&source_replica, *counter)?;

        Ok(true)
    }

    /// Get current version vector
    pub fn get_version_vector(&self) -> Vec<VersionVectorEntry> {
        self.version_vector
            .read()
            .iter()
            .map(|(replica_id, counter)| VersionVectorEntry {
                replica_id: *replica_id,
                counter: *counter,
            })
            .collect()
    }

    /// Compute delta: events we have that peer doesn't
    pub fn compute_delta(&self, peer_vector: &[VersionVectorEntry]) -> Result<Vec<Event>, EventLogError> {
        let peer_vv: HashMap<[u8; 32], u64> = peer_vector
            .iter()
            .map(|e| (e.replica_id, e.counter))
            .collect();

        let mut delta = Vec::new();

        // Iterate all events and include those peer doesn't have
        // This is a simplified approach - in production, we'd track events per replica
        for result in self.storage.all_events() {
            let event = result?;
            // Include if peer version is behind
            // Note: proper implementation would track event->replica mapping
            delta.push(event);
        }

        // Limit batch size
        if delta.len() > 1000 {
            delta.truncate(1000);
        }

        Ok(delta)
    }

    /// Get an event by ID
    pub fn get_event(&self, event_id: &EventId) -> Result<Option<Event>, EventLogError> {
        Ok(self.storage.get_event(event_id)?)
    }

    /// Check if we have an event
    pub fn has_event(&self, event_id: &EventId) -> Result<bool, EventLogError> {
        Ok(self.storage.has_event(event_id)?)
    }

    /// Count all events
    pub fn event_count(&self) -> usize {
        self.storage.event_count()
    }

    /// Get all descriptor publish events
    pub fn get_descriptors(&self) -> Result<Vec<ProviderDescriptor>, EventLogError> {
        let mut descriptors = Vec::new();
        for result in self.storage.all_events() {
            let event = result?;
            if let EventBody::DescriptorPublish(desc_event) = event.body {
                descriptors.push(desc_event.descriptor);
            }
        }
        Ok(descriptors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_log() -> (EventLog, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let storage = Arc::new(Storage::open(dir.path()).unwrap());
        let world_id = WorldId([0; 32]);
        let replica_id = [1; 32];
        (EventLog::new(storage, world_id, replica_id), dir)
    }

    #[test]
    fn test_append_event() {
        let (log, _dir) = create_test_log();

        let event = Event {
            event_id: EventId([1; 32]),
            world: WorldId([0; 32]),
            epoch_id: 1,
            event_type: EventType::RuleEndorsement,
            body: EventBody::RuleEndorsement(RuleEndorsementEvent {
                world: WorldId([0; 32]),
                epoch_id: 1,
                rule_bundle_hash: [0; 32],
                weight: 1.0,
                signer_transport_pubkey: vec![],
                signature: vec![],
            }),
        };

        log.append(event.clone()).unwrap();
        assert!(log.has_event(&event.event_id).unwrap());
        assert_eq!(log.event_count(), 1);
    }

    #[test]
    fn test_duplicate_detection() {
        let (log, _dir) = create_test_log();

        let event = Event {
            event_id: EventId([2; 32]),
            world: WorldId([0; 32]),
            epoch_id: 1,
            event_type: EventType::RuleEndorsement,
            body: EventBody::RuleEndorsement(RuleEndorsementEvent {
                world: WorldId([0; 32]),
                epoch_id: 1,
                rule_bundle_hash: [0; 32],
                weight: 1.0,
                signer_transport_pubkey: vec![],
                signature: vec![],
            }),
        };

        log.append(event.clone()).unwrap();
        let result = log.append(event);
        assert!(matches!(result, Err(EventLogError::DuplicateEvent(_))));
    }

    #[test]
    fn test_version_vector() {
        let (log, _dir) = create_test_log();

        let event1 = Event {
            event_id: EventId([3; 32]),
            world: WorldId([0; 32]),
            epoch_id: 1,
            event_type: EventType::RuleEndorsement,
            body: EventBody::RuleEndorsement(RuleEndorsementEvent {
                world: WorldId([0; 32]),
                epoch_id: 1,
                rule_bundle_hash: [0; 32],
                weight: 1.0,
                signer_transport_pubkey: vec![],
                signature: vec![],
            }),
        };

        let event2 = Event {
            event_id: EventId([4; 32]),
            world: WorldId([0; 32]),
            epoch_id: 2,
            event_type: EventType::RuleEndorsement,
            body: EventBody::RuleEndorsement(RuleEndorsementEvent {
                world: WorldId([0; 32]),
                epoch_id: 2,
                rule_bundle_hash: [0; 32],
                weight: 1.0,
                signer_transport_pubkey: vec![],
                signature: vec![],
            }),
        };

        log.append(event1).unwrap();
        log.append(event2).unwrap();

        let vv = log.get_version_vector();
        assert_eq!(vv.len(), 1);
        assert_eq!(vv[0].counter, 2);
    }
}
