//! prober - TerrainGossip prober daemon
//!
//! This daemon continuously probes providers to verify availability
//! and performance, generating probe receipts for the gossip mesh.

use clap::Parser;
use prober::config::Config;
use prober::scheduler::Scheduler;
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
        .with(EnvFilter::from_default_env().add_directive("prober=info".parse().unwrap()))
        .init();

    // Parse configuration
    let config = Config::parse();

    info!(
        "prober v{} - TerrainGossip Prober Daemon",
        env!("CARGO_PKG_VERSION")
    );
    info!("Probe interval: {}s", config.probe_interval_secs);
    info!("Concurrent probes: {}", config.concurrent_probes);

    // Create scheduler
    let scheduler = Arc::new(Scheduler::new(
        config.challenge_token_count,
        config.probe_timeout_secs,
        1000,
    ));

    // Spawn probe scheduling task
    let schedule_scheduler = scheduler.clone();
    let min_providers = config.min_providers_per_round;
    let max_providers = config.max_providers_per_round;
    let probe_interval = config.probe_interval_secs;
    
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(probe_interval));
        loop {
            ticker.tick().await;
            let scheduled = schedule_scheduler.schedule_due_probes(max_providers);
            info!("Scheduled {} probes", scheduled);
        }
    });

    // Spawn probe execution task
    let exec_scheduler = scheduler.clone();
    let concurrent = config.concurrent_probes;
    
    tokio::spawn(async move {
        loop {
            // Get available slots
            let stats = exec_scheduler.stats();
            let available_slots = concurrent.saturating_sub(stats.in_flight);

            for _ in 0..available_slots {
                if let Some(probe) = exec_scheduler.next_probe() {
                    let exec_scheduler = exec_scheduler.clone();
                    tokio::spawn(async move {
                        // TODO: Execute probe against provider
                        // For now, simulate with random result
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        let passed = rand::random::<bool>();
                        exec_scheduler.report_result(
                            &probe.provider_id,
                            passed,
                            [0u8; 32],
                        );
                    });
                }
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    // TODO: Connect to gossipd for provider discovery
    // TODO: Publish probe receipts to gossip mesh

    info!("Prober started (placeholder - press Ctrl+C to exit)");

    // Wait for shutdown
    tokio::signal::ctrl_c().await.ok();
    info!("Shutting down...");

    // Print final stats
    let stats = scheduler.stats();
    info!(
        "Final stats: {} providers, {} in-flight",
        stats.providers,
        stats.in_flight
    );

    ExitCode::SUCCESS
}
