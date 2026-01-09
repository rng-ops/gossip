//! routerd configuration

use clap::Parser;
use std::path::PathBuf;

/// TerrainGossip Terrain Router Daemon
#[derive(Parser, Debug, Clone)]
#[command(name = "routerd")]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Listen address for routing requests
    #[arg(short, long, default_value = "127.0.0.1:9002")]
    pub listen: String,

    /// Address of gossipd for event subscription
    #[arg(long, default_value = "127.0.0.1:9001")]
    pub gossipd: String,

    /// Cache directory for terrain maps
    #[arg(long, default_value = "./data/routerd")]
    pub cache_dir: PathBuf,

    /// World phrase for authentication
    #[arg(long, env = "TERRAIN_WORLD_PHRASE")]
    pub world_phrase: String,

    /// Minimum provider reputation score (0.0-1.0)
    #[arg(long, default_value = "0.5")]
    pub min_reputation: f64,

    /// Maximum route hops
    #[arg(long, default_value = "3")]
    pub max_hops: u8,

    /// Terrain map update interval (seconds)
    #[arg(long, default_value = "60")]
    pub update_interval_secs: u64,

    /// FAH routing alpha parameter (exploitation vs exploration)
    #[arg(long, default_value = "0.8")]
    pub fah_alpha: f64,

    /// Enable belief field routing
    #[arg(long, default_value = "true")]
    pub enable_belief_fields: bool,
}
