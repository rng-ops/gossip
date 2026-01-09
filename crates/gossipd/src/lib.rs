//! gossipd - TerrainGossip Event Log and Delta Sync Daemon
//!
//! This daemon provides:
//! - Append-only event log storage
//! - Delta synchronization with peers
//! - Version vector management
//! - Event validation and verification
//! - Control-plane membership gating

pub mod config;
pub mod event_log;
pub mod membership;
pub mod server;
pub mod storage;
pub mod sync;

pub use config::Config;
pub use event_log::EventLog;
pub use membership::MembershipManager;
pub use server::Server;
pub use storage::Storage;
pub use sync::SyncManager;
