//! Persistent storage using sled

use sled::Db;
use std::path::Path;
use terrain_gossip_core::types::*;
use thiserror::Error;

/// Storage errors
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] postcard::Error),
    #[error("Event not found: {0}")]
    EventNotFound(String),
    #[error("Descriptor not found: {0}")]
    DescriptorNotFound(String),
}

/// Storage backend for gossipd
pub struct Storage {
    db: Db,
    /// Event tree: event_id -> Event
    events: sled::Tree,
    /// Descriptor tree: descriptor_id -> ProviderDescriptor
    descriptors: sled::Tree,
    /// Version vector tree: replica_id -> counter
    version_vectors: sled::Tree,
    /// Metadata tree: key -> value
    metadata: sled::Tree,
}

impl Storage {
    /// Open storage at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let db = sled::open(path)?;
        let events = db.open_tree("events")?;
        let descriptors = db.open_tree("descriptors")?;
        let version_vectors = db.open_tree("version_vectors")?;
        let metadata = db.open_tree("metadata")?;

        Ok(Self {
            db,
            events,
            descriptors,
            version_vectors,
            metadata,
        })
    }

    /// Store an event
    pub fn put_event(&self, event: &Event) -> Result<(), StorageError> {
        let key = event.event_id.0;
        let value = postcard::to_allocvec(event)?;
        self.events.insert(key, value)?;
        Ok(())
    }

    /// Get an event by ID
    pub fn get_event(&self, event_id: &EventId) -> Result<Option<Event>, StorageError> {
        match self.events.get(event_id.0)? {
            Some(bytes) => {
                let event: Event = postcard::from_bytes(&bytes)?;
                Ok(Some(event))
            }
            None => Ok(None),
        }
    }

    /// Check if event exists
    pub fn has_event(&self, event_id: &EventId) -> Result<bool, StorageError> {
        Ok(self.events.contains_key(event_id.0)?)
    }

    /// Get all events (for iteration)
    pub fn all_events(&self) -> impl Iterator<Item = Result<Event, StorageError>> + '_ {
        self.events.iter().map(|result| {
            let (_, bytes) = result?;
            let event: Event = postcard::from_bytes(&bytes)?;
            Ok(event)
        })
    }

    /// Count events
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Store a descriptor
    pub fn put_descriptor(&self, descriptor: &ProviderDescriptor) -> Result<(), StorageError> {
        let key = descriptor.descriptor_id.0;
        let value = postcard::to_allocvec(descriptor)?;
        self.descriptors.insert(key, value)?;
        Ok(())
    }

    /// Get a descriptor by ID
    pub fn get_descriptor(
        &self,
        descriptor_id: &DescriptorId,
    ) -> Result<Option<ProviderDescriptor>, StorageError> {
        match self.descriptors.get(descriptor_id.0)? {
            Some(bytes) => {
                let desc: ProviderDescriptor = postcard::from_bytes(&bytes)?;
                Ok(Some(desc))
            }
            None => Ok(None),
        }
    }

    /// Get version vector entry
    pub fn get_version(&self, replica_id: &[u8; 32]) -> Result<u64, StorageError> {
        match self.version_vectors.get(replica_id)? {
            Some(bytes) => {
                let counter = u64::from_le_bytes(bytes.as_ref().try_into().unwrap_or([0; 8]));
                Ok(counter)
            }
            None => Ok(0),
        }
    }

    /// Update version vector entry
    pub fn put_version(&self, replica_id: &[u8; 32], counter: u64) -> Result<(), StorageError> {
        self.version_vectors
            .insert(replica_id, &counter.to_le_bytes())?;
        Ok(())
    }

    /// Get all version vector entries
    pub fn get_all_versions(&self) -> Result<Vec<VersionVectorEntry>, StorageError> {
        let mut entries = Vec::new();
        for result in self.version_vectors.iter() {
            let (key, value) = result?;
            let replica_id: [u8; 32] = key.as_ref().try_into().unwrap_or([0; 32]);
            let counter = u64::from_le_bytes(value.as_ref().try_into().unwrap_or([0; 8]));
            entries.push(VersionVectorEntry { replica_id, counter });
        }
        Ok(entries)
    }

    /// Store metadata
    pub fn put_metadata(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        self.metadata.insert(key, value)?;
        Ok(())
    }

    /// Get metadata
    pub fn get_metadata(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.metadata.get(key)?.map(|v| v.to_vec()))
    }

    /// Flush all pending writes
    pub fn flush(&self) -> Result<(), StorageError> {
        self.db.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_storage_events() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path()).unwrap();

        let event = Event {
            event_id: EventId([1; 32]),
            world: WorldId([2; 32]),
            epoch_id: 42,
            event_type: EventType::RuleEndorsement,
            body: EventBody::RuleEndorsement(RuleEndorsementEvent {
                world: WorldId([2; 32]),
                epoch_id: 42,
                rule_bundle_hash: [3; 32],
                weight: 1.0,
                signer_transport_pubkey: vec![4; 32],
                signature: vec![5; 64],
            }),
        };

        storage.put_event(&event).unwrap();
        assert!(storage.has_event(&event.event_id).unwrap());

        let retrieved = storage.get_event(&event.event_id).unwrap().unwrap();
        assert_eq!(retrieved.epoch_id, 42);
    }

    #[test]
    fn test_storage_versions() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path()).unwrap();

        let replica_id = [99; 32];
        assert_eq!(storage.get_version(&replica_id).unwrap(), 0);

        storage.put_version(&replica_id, 42).unwrap();
        assert_eq!(storage.get_version(&replica_id).unwrap(), 42);
    }
}
