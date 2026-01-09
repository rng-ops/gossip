//! Provider registry and scoring

use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use terrain_gossip_core::types::*;

/// Extract model family/base_model_id from a descriptor
pub fn get_model_family(descriptor: &ProviderDescriptor) -> Option<String> {
    match &descriptor.unsigned.capability {
        DescriptorCapability::Manifest(manifest) => Some(manifest.base_model_id.clone()),
        DescriptorCapability::Fah(_) => None,
    }
}

/// Provider state in the router
#[derive(Debug, Clone)]
pub struct ProviderState {
    /// Provider descriptor
    pub descriptor: ProviderDescriptor,
    /// Cached model family for quick lookup
    pub model_family: Option<String>,
    /// Current reputation score (0.0-1.0)
    pub reputation: f64,
    /// Last seen timestamp
    pub last_seen: Instant,
    /// Total successful inferences
    pub successes: u64,
    /// Total failed inferences
    pub failures: u64,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// Is provider currently reachable?
    pub reachable: bool,
}

impl ProviderState {
    pub fn new(descriptor: ProviderDescriptor) -> Self {
        let model_family = get_model_family(&descriptor);
        Self {
            descriptor,
            model_family,
            reputation: 1.0,
            last_seen: Instant::now(),
            successes: 0,
            failures: 0,
            avg_latency_ms: 0.0,
            reachable: true,
        }
    }

    /// Update with successful inference
    pub fn record_success(&mut self, latency_ms: f64) {
        self.successes += 1;
        self.last_seen = Instant::now();
        self.reachable = true;

        // Exponential moving average for latency
        if self.avg_latency_ms == 0.0 {
            self.avg_latency_ms = latency_ms;
        } else {
            self.avg_latency_ms = 0.9 * self.avg_latency_ms + 0.1 * latency_ms;
        }

        // Boost reputation slightly
        self.reputation = (self.reputation + 0.01).min(1.0);
    }

    /// Update with failed inference
    pub fn record_failure(&mut self) {
        self.failures += 1;

        // Decrease reputation
        self.reputation = (self.reputation - 0.05).max(0.0);
    }

    /// Mark as unreachable
    pub fn mark_unreachable(&mut self) {
        self.reachable = false;
        self.reputation = (self.reputation - 0.1).max(0.0);
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        let total = self.successes + self.failures;
        if total == 0 {
            1.0
        } else {
            self.successes as f64 / total as f64
        }
    }

    /// Check if provider is stale
    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.last_seen.elapsed() > max_age
    }
}

/// Provider registry
pub struct ProviderRegistry {
    /// Providers indexed by ID
    providers: RwLock<HashMap<[u8; 32], ProviderState>>,
    /// Providers indexed by model family
    by_model: RwLock<HashMap<String, Vec<[u8; 32]>>>,
    /// Minimum reputation threshold
    min_reputation: f64,
}

impl ProviderRegistry {
    pub fn new(min_reputation: f64) -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            by_model: RwLock::new(HashMap::new()),
            min_reputation,
        }
    }

    /// Register or update a provider
    pub fn register(&self, descriptor: ProviderDescriptor) {
        let id = descriptor.descriptor_id.0;
        let model = get_model_family(&descriptor);

        let mut providers = self.providers.write();
        
        if let Some(existing) = providers.get_mut(&id) {
            // Update existing - refresh model_family from new descriptor
            existing.model_family = get_model_family(&descriptor);
            existing.descriptor = descriptor;
            existing.last_seen = Instant::now();
            existing.reachable = true;
        } else {
            // Add new
            providers.insert(id, ProviderState::new(descriptor));

            // Index by model (only if it's an LLM manifest, not FAH)
            if let Some(model_family) = model {
                let mut by_model = self.by_model.write();
                by_model.entry(model_family).or_insert_with(Vec::new).push(id);
            }
        }
    }

    /// Remove a provider
    pub fn remove(&self, id: &[u8; 32]) {
        if let Some(state) = self.providers.write().remove(id) {
            let mut by_model = self.by_model.write();
            if let Some(ref model_family) = state.model_family {
                if let Some(list) = by_model.get_mut(model_family) {
                    list.retain(|p| p != id);
                }
            }
        }
    }

    /// Get provider by ID
    pub fn get(&self, id: &[u8; 32]) -> Option<ProviderState> {
        self.providers.read().get(id).cloned()
    }

    /// Get providers for a model family
    pub fn by_model(&self, model_family: &str) -> Vec<ProviderState> {
        let by_model = self.by_model.read();
        let providers = self.providers.read();

        by_model
            .get(model_family)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| providers.get(id).cloned())
                    .filter(|p| p.reputation >= self.min_reputation && p.reachable)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all available providers
    pub fn all_available(&self) -> Vec<ProviderState> {
        self.providers
            .read()
            .values()
            .filter(|p| p.reputation >= self.min_reputation && p.reachable)
            .cloned()
            .collect()
    }

    /// Record successful inference
    pub fn record_success(&self, id: &[u8; 32], latency_ms: f64) {
        if let Some(state) = self.providers.write().get_mut(id) {
            state.record_success(latency_ms);
        }
    }

    /// Record failed inference
    pub fn record_failure(&self, id: &[u8; 32]) {
        if let Some(state) = self.providers.write().get_mut(id) {
            state.record_failure();
        }
    }

    /// Mark provider as unreachable
    pub fn mark_unreachable(&self, id: &[u8; 32]) {
        if let Some(state) = self.providers.write().get_mut(id) {
            state.mark_unreachable();
        }
    }

    /// Prune stale providers
    pub fn prune_stale(&self, max_age: Duration) -> usize {
        let mut removed = Vec::new();
        
        {
            let providers = self.providers.read();
            for (id, state) in providers.iter() {
                if state.is_stale(max_age) {
                    removed.push(*id);
                }
            }
        }

        for id in &removed {
            self.remove(id);
        }

        removed.len()
    }

    /// Get registry statistics
    pub fn stats(&self) -> RegistryStats {
        let providers = self.providers.read();
        RegistryStats {
            total: providers.len(),
            reachable: providers.values().filter(|p| p.reachable).count(),
            above_threshold: providers
                .values()
                .filter(|p| p.reputation >= self.min_reputation)
                .count(),
        }
    }
}

/// Registry statistics
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total: usize,
    pub reachable: usize,
    pub above_threshold: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_descriptor(id: u8) -> ProviderDescriptor {
        ProviderDescriptor {
            descriptor_id: DescriptorId([id; 32]),
            unsigned: ProviderDescriptorUnsigned {
                world: WorldId([0u8; 32]),
                descriptor_epoch: 1,
                contact_points: vec![format!("127.0.0.1:900{}", id)],
                capability: DescriptorCapability::Manifest(CapabilityManifest {
                    base_model_id: "llama-3".to_string(),
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
    fn test_provider_registration() {
        let registry = ProviderRegistry::new(0.5);
        let desc = test_descriptor(1);

        registry.register(desc.clone());

        let providers = registry.by_model("llama-3");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].descriptor.descriptor_id.0, [1; 32]);
    }

    #[test]
    fn test_success_tracking() {
        let registry = ProviderRegistry::new(0.5);
        let desc = test_descriptor(2);
        let id = desc.descriptor_id.0;

        registry.register(desc);
        registry.record_success(&id, 100.0);

        let state = registry.get(&id).unwrap();
        assert_eq!(state.successes, 1);
        assert!(state.avg_latency_ms > 0.0);
    }

    #[test]
    fn test_reputation_decrease() {
        let registry = ProviderRegistry::new(0.5);
        let desc = test_descriptor(3);
        let id = desc.descriptor_id.0;

        registry.register(desc);
        
        let initial_rep = registry.get(&id).unwrap().reputation;
        registry.record_failure(&id);
        let final_rep = registry.get(&id).unwrap().reputation;

        assert!(final_rep < initial_rep);
    }
}
