//! infernode configuration

use clap::Parser;
use std::path::PathBuf;

/// TerrainGossip Inference Node Daemon
#[derive(Parser, Debug, Clone)]
#[command(name = "infernode")]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Listen address for incoming connections
    #[arg(short, long, default_value = "0.0.0.0:9004")]
    pub listen: String,

    /// Address of gossipd for event subscription
    #[arg(long, default_value = "127.0.0.1:9001")]
    pub gossipd: String,

    /// Address of routerd for provider discovery
    #[arg(long, default_value = "127.0.0.1:9002")]
    pub routerd: String,

    /// Data directory for keys and state
    #[arg(long, default_value = "./data/infernode")]
    pub data_dir: PathBuf,

    /// World phrase for authentication
    #[arg(long, env = "TERRAIN_WORLD_PHRASE")]
    pub world_phrase: String,

    /// Maximum concurrent circuits
    #[arg(long, default_value = "100")]
    pub max_circuits: usize,

    /// Circuit timeout (seconds)
    #[arg(long, default_value = "300")]
    pub circuit_timeout_secs: u64,

    /// Maximum onion hops
    #[arg(long, default_value = "3")]
    pub max_hops: u8,

    /// Enable relay mode (forward for other nodes)
    #[arg(long, default_value = "true")]
    pub enable_relay: bool,

    /// Local inference backend URL (if provider)
    #[arg(long)]
    pub inference_backend: Option<String>,

    /// Model family (if provider)
    #[arg(long)]
    pub model_family: Option<String>,

    /// Rate limit: requests per minute
    #[arg(long, default_value = "60")]
    pub rate_limit_rpm: u32,

    /// Rate limit: tokens per minute
    #[arg(long, default_value = "100000")]
    pub rate_limit_tpm: u32,
}

impl Config {
    /// Check if this node is configured as a provider
    pub fn is_provider(&self) -> bool {
        self.inference_backend.is_some() && self.model_family.is_some()
    }
}
