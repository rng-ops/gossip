//! Configuration for gossipd

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// gossipd - TerrainGossip Event Log Daemon
#[derive(Parser, Debug, Clone)]
#[command(name = "gossipd")]
#[command(about = "TerrainGossip event log and delta sync daemon")]
pub struct Config {
    /// Listen address for control-plane connections
    #[arg(short, long, default_value = "0.0.0.0:9100")]
    pub listen: SocketAddr,

    /// Data directory for persistent storage
    #[arg(short, long, default_value = "./data/gossipd")]
    pub data_dir: PathBuf,

    /// World phrase (creates WorldId with rule bundle)
    #[arg(long, env = "GOSSIP_WORLD_PHRASE")]
    pub world_phrase: String,

    /// Path to the rule bundle JSON file
    #[arg(long)]
    pub rule_bundle: Option<PathBuf>,

    /// Bootstrap peers (comma-separated addresses)
    #[arg(long, value_delimiter = ',')]
    pub bootstrap: Vec<SocketAddr>,

    /// Maximum events per delta sync response
    #[arg(long, default_value = "1000")]
    pub max_sync_events: u32,

    /// Sync interval in seconds
    #[arg(long, default_value = "30")]
    pub sync_interval_secs: u64,

    /// Enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    /// Log format (json or pretty)
    #[arg(long, default_value = "pretty")]
    pub log_format: String,
}

impl Config {
    /// Validate configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.world_phrase.is_empty() {
            anyhow::bail!("World phrase cannot be empty");
        }
        if self.world_phrase.split_whitespace().count() < 2 {
            anyhow::bail!("World phrase should contain at least 2 words");
        }
        Ok(())
    }
}

/// Persisted node state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    /// Our transport keypair seed (deterministic recovery)
    pub keypair_seed: [u8; 32],
    /// World ID we're operating in
    pub world_id: [u8; 32],
    /// Control plane key (if we have it)
    pub control_plane_key: Option<[u8; 32]>,
}
