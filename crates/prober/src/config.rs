//! prober configuration

use clap::Parser;
use std::path::PathBuf;

/// TerrainGossip Prober Daemon
#[derive(Parser, Debug, Clone)]
#[command(name = "prober")]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Address of gossipd for event subscription
    #[arg(long, default_value = "127.0.0.1:9001")]
    pub gossipd: String,

    /// Address of routerd for provider discovery
    #[arg(long, default_value = "127.0.0.1:9002")]
    pub routerd: String,

    /// Data directory for probe history
    #[arg(long, default_value = "./data/prober")]
    pub data_dir: PathBuf,

    /// World phrase for authentication
    #[arg(long, env = "TERRAIN_WORLD_PHRASE")]
    pub world_phrase: String,

    /// Probe interval (seconds)
    #[arg(long, default_value = "300")]
    pub probe_interval_secs: u64,

    /// Number of concurrent probes
    #[arg(long, default_value = "10")]
    pub concurrent_probes: usize,

    /// Probe timeout (seconds)
    #[arg(long, default_value = "30")]
    pub probe_timeout_secs: u64,

    /// Challenge token count
    #[arg(long, default_value = "5")]
    pub challenge_token_count: usize,

    /// Minimum providers per round
    #[arg(long, default_value = "5")]
    pub min_providers_per_round: usize,

    /// Maximum providers per round
    #[arg(long, default_value = "50")]
    pub max_providers_per_round: usize,
}
