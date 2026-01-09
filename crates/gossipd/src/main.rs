//! gossipd - TerrainGossip event synchronization daemon
//!
//! This daemon maintains the append-only event log and synchronizes
//! events across the gossip mesh using delta-state CRDT semantics.

use clap::Parser;
use gossipd::config::Config;
use gossipd::server::Server;
use std::process::ExitCode;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> ExitCode {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("gossipd=info".parse().unwrap()))
        .init();

    // Parse configuration
    let config = Config::parse();

    info!(
        "gossipd v{} - TerrainGossip Event Sync Daemon",
        env!("CARGO_PKG_VERSION")
    );

    // Create and run server
    match Server::new(config) {
        Ok(server) => {
            // Install signal handlers
            let shutdown_server = {
                let server_ref = &server;
                move || server_ref.shutdown()
            };

            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                info!("Received shutdown signal");
            });

            if let Err(e) = server.run().await {
                error!("Server error: {}", e);
                return ExitCode::FAILURE;
            }
        }
        Err(e) => {
            error!("Failed to initialize server: {}", e);
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}
