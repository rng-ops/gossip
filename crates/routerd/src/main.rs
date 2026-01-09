//! routerd - TerrainGossip terrain router daemon
//!
//! This daemon maintains the terrain map for FAH routing and
//! provides provider selection based on pheromone trails.

use clap::Parser;
use routerd::config::Config;
use routerd::router::Router;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> ExitCode {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("routerd=info".parse().unwrap()))
        .init();

    // Parse configuration
    let config = Config::parse();

    info!(
        "routerd v{} - TerrainGossip Terrain Router",
        env!("CARGO_PKG_VERSION")
    );
    info!("Listening on {}", config.listen);
    info!("Connecting to gossipd at {}", config.gossipd);

    // Create router
    let router = Arc::new(Router::new(config.clone()));

    // Spawn maintenance task
    let maintenance_router = router.clone();
    let update_interval = config.update_interval_secs;
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(update_interval));
        loop {
            ticker.tick().await;
            maintenance_router.maintenance();
        }
    });

    // TODO: Connect to gossipd for descriptor events
    // TODO: Start HTTP/gRPC server for routing requests

    info!("Router started (placeholder - press Ctrl+C to exit)");

    // Wait for shutdown
    tokio::signal::ctrl_c().await.ok();
    info!("Shutting down...");

    ExitCode::SUCCESS
}
