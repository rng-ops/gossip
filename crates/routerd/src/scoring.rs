//! Provider scoring using multi-factor ranking

use crate::provider::ProviderState;
use crate::terrain::{TerrainCoord, TerrainMap};
use std::cmp::Ordering;

/// Scoring weights
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    /// Pheromone weight (FAH routing)
    pub pheromone: f64,
    /// Reputation weight
    pub reputation: f64,
    /// Success rate weight
    pub success_rate: f64,
    /// Latency weight (inverse)
    pub latency: f64,
    /// Exploration bonus for low-traffic providers
    pub exploration: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            pheromone: 0.3,
            reputation: 0.25,
            success_rate: 0.2,
            latency: 0.15,
            exploration: 0.1,
        }
    }
}

/// Scored provider for ranking
#[derive(Debug, Clone)]
pub struct ScoredProvider {
    pub id: [u8; 32],
    pub score: f64,
    pub components: ScoreComponents,
}

/// Individual score components for debugging
#[derive(Debug, Clone)]
pub struct ScoreComponents {
    pub pheromone: f64,
    pub reputation: f64,
    pub success_rate: f64,
    pub latency: f64,
    pub exploration: f64,
}

/// Provider scorer
pub struct Scorer {
    weights: ScoringWeights,
    /// Alpha for exploration vs exploitation
    alpha: f64,
}

impl Scorer {
    pub fn new(weights: ScoringWeights, alpha: f64) -> Self {
        Self { weights, alpha }
    }

    /// Score a single provider for a coordinate
    pub fn score(
        &self,
        provider: &ProviderState,
        coord: &TerrainCoord,
        terrain: &TerrainMap,
    ) -> ScoredProvider {
        let id = provider.descriptor.descriptor_id.0;

        // Pheromone component (0.0 - 1.0 normalized)
        let pheromone_raw = terrain.pheromone_strength(coord, &id);
        let pheromone = (pheromone_raw / 100.0).min(1.0);

        // Reputation component
        let reputation = provider.reputation;

        // Success rate component
        let success_rate = provider.success_rate();

        // Latency component (inverse, normalized)
        let latency = if provider.avg_latency_ms > 0.0 {
            (1000.0 / provider.avg_latency_ms).min(1.0)
        } else {
            0.5 // Unknown latency gets neutral score
        };

        // Exploration bonus (inverse of usage)
        let total_usage = provider.successes + provider.failures;
        let exploration = if total_usage < 10 {
            1.0 // High bonus for rarely-used providers
        } else if total_usage < 100 {
            0.5
        } else {
            0.1 // Low bonus for heavily-used providers
        };

        // Compute weighted score
        let score = self.weights.pheromone * pheromone
            + self.weights.reputation * reputation
            + self.weights.success_rate * success_rate
            + self.weights.latency * latency
            + self.weights.exploration * exploration * (1.0 - self.alpha);

        ScoredProvider {
            id,
            score,
            components: ScoreComponents {
                pheromone,
                reputation,
                success_rate,
                latency,
                exploration,
            },
        }
    }

    /// Score and rank multiple providers
    pub fn rank(
        &self,
        providers: &[ProviderState],
        coord: &TerrainCoord,
        terrain: &TerrainMap,
    ) -> Vec<ScoredProvider> {
        let mut scored: Vec<ScoredProvider> = providers
            .iter()
            .map(|p| self.score(p, coord, terrain))
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(Ordering::Equal)
        });

        scored
    }

    /// Select top N providers
    pub fn select_top(
        &self,
        providers: &[ProviderState],
        coord: &TerrainCoord,
        terrain: &TerrainMap,
        n: usize,
    ) -> Vec<ScoredProvider> {
        let ranked = self.rank(providers, coord, terrain);
        ranked.into_iter().take(n).collect()
    }

    /// Probabilistic selection weighted by score
    pub fn probabilistic_select(
        &self,
        providers: &[ProviderState],
        coord: &TerrainCoord,
        terrain: &TerrainMap,
    ) -> Option<ScoredProvider> {
        let scored = self.rank(providers, coord, terrain);
        if scored.is_empty() {
            return None;
        }

        // Compute selection probabilities
        let total_score: f64 = scored.iter().map(|s| s.score).sum();
        if total_score <= 0.0 {
            return scored.into_iter().next();
        }

        // Weighted random selection
        let mut rng = rand::thread_rng();
        use rand::Rng;
        let threshold = rng.gen::<f64>() * total_score;

        let mut cumulative = 0.0;
        for sp in scored {
            cumulative += sp.score;
            if cumulative >= threshold {
                return Some(sp);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ProviderState;
    use terrain_gossip_core::types::*;

    fn test_provider(id: u8, reputation: f64) -> ProviderState {
        let capability = DescriptorCapability::Manifest(CapabilityManifest {
            base_model_id: "llama-3".to_string(),
            weights_digest: [0u8; 32],
            runtime_id: "vllm".to_string(),
            context_limit: 8192,
            tool_schemas_digest: [0u8; 32],
            safety_mode: "default".to_string(),
            adapters: vec![],
        });

        let unsigned = ProviderDescriptorUnsigned {
            world: WorldId([0u8; 32]),
            descriptor_epoch: 1,
            contact_points: vec![format!("127.0.0.1:900{}", id)],
            capability,
        };

        let desc = ProviderDescriptor {
            descriptor_id: DescriptorId([id; 32]),
            unsigned,
            provider_transport_pubkey: vec![id; 32],
            signature: vec![],
        };

        let mut state = ProviderState::new(desc);
        state.reputation = reputation;
        state
    }

    #[test]
    fn test_scoring() {
        let scorer = Scorer::new(ScoringWeights::default(), 0.8);
        let terrain = TerrainMap::new();
        let coord = TerrainCoord::new("llama-3", 1);

        let provider = test_provider(1, 0.9);
        let scored = scorer.score(&provider, &coord, &terrain);

        assert!(scored.score > 0.0);
        assert!(scored.components.reputation == 0.9);
    }

    #[test]
    fn test_ranking() {
        let scorer = Scorer::new(ScoringWeights::default(), 0.8);
        let terrain = TerrainMap::new();
        let coord = TerrainCoord::new("llama-3", 1);

        let providers = vec![
            test_provider(1, 0.3),
            test_provider(2, 0.9),
            test_provider(3, 0.6),
        ];

        let ranked = scorer.rank(&providers, &coord, &terrain);
        assert_eq!(ranked.len(), 3);
        // Higher reputation should rank higher
        assert_eq!(ranked[0].id, [2; 32]);
    }
}
