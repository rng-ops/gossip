//! gossipd server - main service loop

use crate::config::Config;
use crate::event_log::EventLog;
use crate::membership::MembershipManager;
use crate::storage::Storage;
use crate::sync::SyncManager;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use terrain_gossip_core::types::*;
use terrain_gossip_net::crypto::KeyPair;
use terrain_gossip_net::peer::{PeerId, PeerRoles};
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Server errors
#[derive(Debug, Error)]
pub enum ServerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
    #[error("Bind failed: {0}")]
    BindFailed(SocketAddr),
    #[error("Server shutdown")]
    Shutdown,
}

/// Server state
pub struct Server {
    config: Config,
    keypair: KeyPair,
    storage: Arc<Storage>,
    event_log: Arc<EventLog>,
    membership: Arc<MembershipManager>,
    sync_manager: Arc<SyncManager>,
    /// Connected peers
    peers: RwLock<HashMap<[u8; 32], ConnectedPeer>>,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
}

/// Connected peer state
#[derive(Debug)]
pub struct ConnectedPeer {
    pub peer_id: PeerId,
    pub addr: SocketAddr,
    pub roles: PeerRoles,
    pub connected_at: std::time::Instant,
}

impl Server {
    /// Create a new server instance
    pub fn new(config: Config) -> Result<Self, ServerError> {
        // Generate or load keypair
        let keypair = KeyPair::generate();
        
        // Open storage
        let storage = Arc::new(Storage::open(&config.data_dir)?);
        
        // Create membership manager
        let membership = Arc::new(MembershipManager::new(
            &config.world_phrase,
            1000, // Default rate limit RPM
        ));
        
        // Create event log
        let event_log = Arc::new(EventLog::new(
            storage.clone(),
            WorldId(membership.world_id()),
            keypair.public_key(),
        ));
        
        // Create sync manager
        let sync_manager = Arc::new(SyncManager::new(
            event_log.clone(),
            Duration::from_secs(config.sync_interval_secs),
            config.max_sync_events as usize,
        ));
        
        let (shutdown_tx, _) = broadcast::channel(1);
        
        Ok(Self {
            config,
            keypair,
            storage,
            event_log,
            membership,
            sync_manager,
            peers: RwLock::new(HashMap::new()),
            shutdown_tx,
        })
    }

    /// Get the server's public key
    pub fn public_key(&self) -> [u8; 32] {
        self.keypair.public_key()
    }

    /// Get the world ID
    pub fn world_id(&self) -> [u8; 32] {
        self.membership.world_id()
    }

    /// Run the server
    pub async fn run(&self) -> Result<(), ServerError> {
        info!(
            "Starting gossipd on {} (world: {:02x?})",
            self.config.listen,
            &self.world_id()[..8]
        );

        // Bootstrap peers
        for addr in &self.config.bootstrap {
            info!("Bootstrap peer: {}", addr);
            // TODO: Connect to bootstrap peers
        }

        // Spawn background tasks
        let sync_handle = self.spawn_sync_task();
        let prune_handle = self.spawn_prune_task();

        // Start TCP listener
        let listener = TcpListener::bind(&self.config.listen).await?;
        info!("Listening on {}", self.config.listen);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            debug!("Accepted connection from {}", addr);
                            let server = self.clone_arc();
                            tokio::spawn(async move {
                                if let Err(e) = server.handle_connection(stream, addr).await {
                                    warn!("Connection error from {}: {}", addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutting down...");
                    break;
                }
            }
        }

        // Wait for background tasks
        sync_handle.abort();
        prune_handle.abort();

        // Flush storage
        self.storage.flush()?;

        Ok(())
    }

    /// Clone as Arc for spawning tasks
    fn clone_arc(&self) -> Arc<Self> {
        // This is a simplified version - in production we'd use Arc<Self>
        // For now, we'll create a new instance with shared state
        Arc::new(Self {
            config: self.config.clone(),
            keypair: self.keypair.clone(),
            storage: self.storage.clone(),
            event_log: self.event_log.clone(),
            membership: self.membership.clone(),
            sync_manager: self.sync_manager.clone(),
            peers: RwLock::new(HashMap::new()),
            shutdown_tx: self.shutdown_tx.clone(),
        })
    }

    /// Handle an incoming connection
    async fn handle_connection(
        self: Arc<Self>,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<(), ServerError> {
        // TODO: Implement full protocol handshake
        // 1. Receive HELLO with peer's pubkey and world phrase
        // 2. Verify world phrase matches
        // 3. Admit peer to membership
        // 4. Exchange version vectors
        // 5. Start delta sync
        
        info!("Connection handler for {} (placeholder)", addr);
        
        // Placeholder: just keep connection alive briefly
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        Ok(())
    }

    /// Spawn background sync task
    fn spawn_sync_task(&self) -> tokio::task::JoinHandle<()> {
        let sync_manager = self.sync_manager.clone();
        let interval_secs = self.config.sync_interval_secs;
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(interval_secs));
            
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let peers = sync_manager.peers_needing_sync();
                        for peer_id in peers {
                            debug!("Syncing with peer {:02x?}", &peer_id[..8]);
                            // TODO: Perform actual sync
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        })
    }

    /// Spawn background prune task
    fn spawn_prune_task(&self) -> tokio::task::JoinHandle<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(3600)); // Hourly
            
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        debug!("Running prune cycle");
                        // TODO: Prune old events, stale peers
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        })
    }

    /// Shutdown the server
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    /// Get server statistics
    pub fn stats(&self) -> ServerStats {
        ServerStats {
            peer_count: self.peers.read().len(),
            event_count: self.event_log.event_count(),
            member_count: self.membership.member_count(),
            sync_stats: self.sync_manager.stats(),
        }
    }
}

/// Server statistics
#[derive(Debug, Clone)]
pub struct ServerStats {
    pub peer_count: usize,
    pub event_count: usize,
    pub member_count: usize,
    pub sync_stats: crate::sync::SyncStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_config() -> Config {
        let dir = tempdir().unwrap();
        Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            data_dir: dir.into_path(),
            world_phrase: "test-world phrase".to_string(),
            rule_bundle: None,
            bootstrap: vec![],
            max_sync_events: 100,
            sync_interval_secs: 30,
            verbose: false,
            log_format: "pretty".to_string(),
        }
    }

    #[test]
    fn test_server_creation() {
        let config = test_config();
        let server = Server::new(config).unwrap();
        
        assert_eq!(server.stats().peer_count, 0);
        assert_eq!(server.stats().event_count, 0);
    }

    #[test]
    fn test_world_id() {
        let config = test_config();
        let server = Server::new(config).unwrap();
        
        let world_id = server.world_id();
        assert_ne!(world_id, [0; 32]);
    }
}
