//! infernode - TerrainGossip inference node daemon
//!
//! This daemon handles onion-routed inference requests, acting as
//! either a relay node or a final destination (provider).

use clap::Parser;
use infernode::circuit::CircuitManager;
use infernode::config::Config;
use infernode::relay::Relay;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
use terrain_gossip_net::crypto::KeyPair;
use tokio::time::interval;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> ExitCode {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("infernode=info".parse().unwrap()))
        .init();

    // Parse configuration
    let config = Config::parse();

    info!(
        "infernode v{} - TerrainGossip Inference Node",
        env!("CARGO_PKG_VERSION")
    );
    info!("Listening on {}", config.listen);

    if config.is_provider() {
        info!(
            "Running as provider: {} @ {}",
            config.model_family.as_ref().unwrap(),
            config.inference_backend.as_ref().unwrap()
        );
    }

    if config.enable_relay {
        info!("Relay mode enabled");
    }

    // Generate node keypair
    let keypair = KeyPair::generate();
    let node_id = keypair.public_key();
    info!("Node ID: {:02x?}", &node_id[..16]);

    // Create circuit manager (for client-side circuits)
    let circuit_manager = Arc::new(CircuitManager::new(
        config.max_circuits,
        config.circuit_timeout_secs,
    ));

    // Create relay handler
    let relay = Arc::new(Relay::new(
        node_id,
        config.max_circuits,
        config.rate_limit_rpm * 10, // cells per minute
        config.enable_relay,
    ));

    // Spawn maintenance task
    let maint_circuits = circuit_manager.clone();
    let maint_relay = relay.clone();
    let circuit_timeout = config.circuit_timeout_secs;
    
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            
            // Prune client circuits
            let pruned_circuits = maint_circuits.prune();
            if pruned_circuits > 0 {
                info!("Pruned {} client circuits", pruned_circuits);
            }

            // Prune relay circuits
            let pruned_relay = maint_relay.prune_circuits(Duration::from_secs(circuit_timeout));
            if pruned_relay > 0 {
                info!("Pruned {} relay circuits", pruned_relay);
            }
        }
    });

    // Spawn stats logging task
    let stats_circuits = circuit_manager.clone();
    let stats_relay = relay.clone();
    
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(300));
        loop {
            ticker.tick().await;
            
            let circuit_stats = stats_circuits.stats();
            let relay_stats = stats_relay.stats();

            info!(
                "Stats: {} client circuits ({} ready), {} relay circuits, {} cells processed",
                circuit_stats.total,
                circuit_stats.ready,
                stats_relay.circuit_count(),
                relay_stats.cells_processed
            );
        }
    });

    // TODO: Start network listener
    // TODO: Connect to gossipd for descriptor updates
    // TODO: Register as provider (if configured)
    // TODO: Handle incoming circuit requests
    // TODO: Forward inference requests

    info!("Inference node started (placeholder - press Ctrl+C to exit)");

    // Wait for shutdown
    tokio::signal::ctrl_c().await.ok();
    info!("Shutting down...");

    // Print final stats
    let circuit_stats = circuit_manager.stats();
    let relay_stats = relay.stats();
    info!(
        "Final stats: {} circuits, {} cells processed, {} forwarded, {} delivered",
        circuit_stats.total,
        relay_stats.cells_processed,
        relay_stats.cells_forwarded,
        relay_stats.cells_delivered
    );

    ExitCode::SUCCESS
}
