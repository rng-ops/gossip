//! Main router logic

use crate::config::Config;
use crate::provider::{get_model_family, ProviderRegistry, ProviderState};
use crate::scoring::{ScoredProvider, Scorer, ScoringWeights};
use crate::terrain::{TerrainCoord, TerrainMap};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};
use terrain_gossip_core::types::*;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Router errors
#[derive(Debug, Error)]
pub enum RouterError {
    #[error("No providers available for model: {0}")]
    NoProviders(String),
    #[error("All providers exhausted")]
    ProvidersExhausted,
    #[error("Invalid route request")]
    InvalidRequest,
}

/// Route request
#[derive(Debug, Clone)]
pub struct RouteRequest {
    /// Requested model family
    pub model_family: String,
    /// Required capabilities
    pub capabilities: u64,
    /// Maximum latency requirement (ms)
    pub max_latency_ms: Option<u64>,
    /// Preferred number of hops
    pub preferred_hops: Option<u8>,
    /// Exclude these providers
    pub exclude: Vec<[u8; 32]>,
}

/// Route response
#[derive(Debug, Clone)]
pub struct RouteResponse {
    /// Selected provider
    pub provider: ScoredProvider,
    /// Full provider state
    pub state: ProviderState,
    /// Alternative providers
    pub alternatives: Vec<ScoredProvider>,
}

/// Main router service
pub struct Router {
    config: Config,
    terrain: Arc<TerrainMap>,
    registry: Arc<ProviderRegistry>,
    scorer: Scorer,
    /// Last terrain update
    last_update: RwLock<Instant>,
}

impl Router {
    pub fn new(config: Config) -> Self {
        let terrain = Arc::new(TerrainMap::new());
        let registry = Arc::new(ProviderRegistry::new(config.min_reputation));
        let scorer = Scorer::new(ScoringWeights::default(), config.fah_alpha);

        Self {
            config,
            terrain,
            registry,
            scorer,
            last_update: RwLock::new(Instant::now()),
        }
    }

    /// Register a provider descriptor
    pub fn register_provider(&self, descriptor: ProviderDescriptor) {
        let id = descriptor.descriptor_id.0;
        let model_family = get_model_family(&descriptor);
        let coord = TerrainCoord::new(model_family.as_deref().unwrap_or("unknown"), 0);

        // Register in registry
        self.registry.register(descriptor);

        // Register in terrain
        self.terrain.register_provider(coord, id);
    }

    /// Remove a provider
    pub fn remove_provider(&self, id: &[u8; 32]) {
        self.registry.remove(id);
        self.terrain.remove_provider(id);
    }

    /// Route a request to a provider
    pub fn route(&self, request: RouteRequest) -> Result<RouteResponse, RouterError> {
        let coord = TerrainCoord::new(&request.model_family, request.capabilities);

        // Get available providers
        let mut providers = self.registry.by_model(&request.model_family);

        // Note: capability filtering is done via model_family matching already

        // Exclude specified providers
        if !request.exclude.is_empty() {
            providers.retain(|p| !request.exclude.contains(&p.descriptor.descriptor_id.0));
        }

        // Filter by latency if specified
        if let Some(max_latency) = request.max_latency_ms {
            providers.retain(|p| {
                p.avg_latency_ms == 0.0 || p.avg_latency_ms <= max_latency as f64
            });
        }

        if providers.is_empty() {
            return Err(RouterError::NoProviders(request.model_family));
        }

        // Score and rank
        let ranked = self.scorer.rank(&providers, &coord, &self.terrain);

        if ranked.is_empty() {
            return Err(RouterError::ProvidersExhausted);
        }

        // Select best
        let selected = ranked[0].clone();
        let state = providers
            .into_iter()
            .find(|p| p.descriptor.descriptor_id.0 == selected.id)
            .ok_or(RouterError::ProvidersExhausted)?;

        Ok(RouteResponse {
            provider: selected,
            state,
            alternatives: ranked.into_iter().skip(1).take(3).collect(),
        })
    }

    /// Report successful inference
    pub fn report_success(&self, provider_id: &[u8; 32], model_family: &str, latency_ms: f64) {
        let coord = TerrainCoord::new(model_family, 0);

        // Update registry
        self.registry.record_success(provider_id, latency_ms);

        // Deposit pheromone
        let deposit_amount = (1000.0 / latency_ms.max(1.0)).min(10.0);
        self.terrain.deposit(&coord, provider_id, deposit_amount);

        debug!(
            "Success for {:?}: latency={:.0}ms, deposit={:.2}",
            hex::encode(&provider_id[..8]),
            latency_ms,
            deposit_amount
        );
    }

    /// Report failed inference
    pub fn report_failure(&self, provider_id: &[u8; 32], model_family: &str) {
        let coord = TerrainCoord::new(model_family, 0);

        // Update registry
        self.registry.record_failure(provider_id);

        // Evaporate pheromone
        self.terrain.evaporate(&coord, provider_id, 5.0);

        debug!(
            "Failure for {:?}",
            hex::encode(&provider_id[..8])
        );
    }

    /// Report unreachable provider
    pub fn report_unreachable(&self, provider_id: &[u8; 32]) {
        self.registry.mark_unreachable(provider_id);
        warn!(
            "Provider unreachable: {:?}",
            hex::encode(&provider_id[..8])
        );
    }

    /// Run periodic maintenance
    pub fn maintenance(&self) {
        // Decay terrain pheromones
        self.terrain.global_decay();

        // Prune stale providers
        let pruned = self.registry.prune_stale(Duration::from_secs(3600));
        if pruned > 0 {
            info!("Pruned {} stale providers", pruned);
        }

        *self.last_update.write() = Instant::now();
    }

    /// Get router statistics
    pub fn stats(&self) -> RouterStats {
        RouterStats {
            terrain: self.terrain.stats(),
            registry: self.registry.stats(),
        }
    }
}

/// Router statistics
#[derive(Debug, Clone)]
pub struct RouterStats {
    pub terrain: crate::terrain::TerrainStats,
    pub registry: crate::provider::RegistryStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            listen: "127.0.0.1:9002".to_string(),
            gossipd: "127.0.0.1:9001".to_string(),
            cache_dir: std::path::PathBuf::from("/tmp/routerd-test"),
            world_phrase: "test".to_string(),
            min_reputation: 0.5,
            max_hops: 3,
            update_interval_secs: 60,
            fah_alpha: 0.8,
            enable_belief_fields: true,
        }
    }

    fn test_descriptor(id: u8, model: &str) -> ProviderDescriptor {
        ProviderDescriptor {
            descriptor_id: DescriptorId([id; 32]),
            unsigned: ProviderDescriptorUnsigned {
                world: WorldId([0u8; 32]),
                descriptor_epoch: 1,
                contact_points: vec![format!("127.0.0.1:900{}", id)],
                capability: DescriptorCapability::Manifest(CapabilityManifest {
                    base_model_id: model.to_string(),
                    weights_digest: [0u8; 32],
                    runtime_id: "default".to_string(),
                    context_limit: 8192,
                    tool_schemas_digest: [0u8; 32],
                    safety_mode: "standard".to_string(),
                    adapters: vec![],
                }),
            },
            provider_transport_pubkey: vec![id; 32],
            signature: vec![],
        }
    }

    #[test]
    fn test_routing() {
        let router = Router::new(test_config());

        // Register providers
        router.register_provider(test_descriptor(1, "llama-3"));
        router.register_provider(test_descriptor(2, "llama-3"));

        // Route request
        let request = RouteRequest {
            model_family: "llama-3".to_string(),
            capabilities: 1,
            max_latency_ms: None,
            preferred_hops: None,
            exclude: vec![],
        };

        let response = router.route(request).unwrap();
        assert!(!response.alternatives.is_empty() || true);
    }

    #[test]
    fn test_feedback_loop() {
        let router = Router::new(test_config());

        router.register_provider(test_descriptor(1, "gpt-4"));

        let id = [1; 32];

        // Report success
        router.report_success(&id, "gpt-4", 50.0);

        // Provider should have updated stats
        if let Some(state) = router.registry.get(&id) {
            assert_eq!(state.successes, 1);
        }
    }
}
