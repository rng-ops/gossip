//! Terrain map for FAH (Foraging Ant Heuristic) routing

use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::Instant;

/// Pheromone decay rate per second
const PHEROMONE_DECAY_RATE: f64 = 0.01;

/// Maximum pheromone level
const MAX_PHEROMONE: f64 = 100.0;

/// Minimum pheromone level (prevents zero)
const MIN_PHEROMONE: f64 = 0.1;

/// Terrain coordinates (model family + capability)
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TerrainCoord {
    /// Model family hash (first 8 bytes)
    pub model_family: [u8; 8],
    /// Capability flags
    pub capabilities: u64,
}

impl TerrainCoord {
    pub fn new(model_family: &str, capabilities: u64) -> Self {
        let hash = blake3::hash(model_family.as_bytes());
        let mut family = [0u8; 8];
        family.copy_from_slice(&hash.as_bytes()[..8]);
        Self {
            model_family: family,
            capabilities,
        }
    }
}

/// Pheromone trail on an edge
#[derive(Debug, Clone)]
pub struct PheromoneTrail {
    /// Pheromone strength (0.0 - MAX_PHEROMONE)
    pub strength: f64,
    /// Last update timestamp
    pub last_update: Instant,
    /// Success count for this trail
    pub successes: u64,
    /// Failure count for this trail
    pub failures: u64,
}

impl Default for PheromoneTrail {
    fn default() -> Self {
        Self {
            strength: MIN_PHEROMONE,
            last_update: Instant::now(),
            successes: 0,
            failures: 0,
        }
    }
}

impl PheromoneTrail {
    /// Apply decay based on elapsed time
    pub fn decay(&mut self) {
        let elapsed = self.last_update.elapsed().as_secs_f64();
        self.strength = (self.strength * (-PHEROMONE_DECAY_RATE * elapsed).exp())
            .max(MIN_PHEROMONE);
        self.last_update = Instant::now();
    }

    /// Deposit pheromone (positive reinforcement)
    pub fn deposit(&mut self, amount: f64) {
        self.decay();
        self.strength = (self.strength + amount).min(MAX_PHEROMONE);
        self.successes += 1;
    }

    /// Evaporate pheromone (negative reinforcement)
    pub fn evaporate(&mut self, amount: f64) {
        self.decay();
        self.strength = (self.strength - amount).max(MIN_PHEROMONE);
        self.failures += 1;
    }

    /// Get success ratio
    pub fn success_ratio(&self) -> f64 {
        let total = self.successes + self.failures;
        if total == 0 {
            0.5 // neutral
        } else {
            self.successes as f64 / total as f64
        }
    }
}

/// Edge in the terrain map (from coord to provider)
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TerrainEdge {
    pub coord: TerrainCoord,
    pub provider_id: [u8; 32],
}

/// Terrain map for FAH routing
pub struct TerrainMap {
    /// Pheromone trails indexed by edge
    trails: RwLock<HashMap<TerrainEdge, PheromoneTrail>>,
    /// Providers at each coordinate
    providers_at: RwLock<HashMap<TerrainCoord, Vec<[u8; 32]>>>,
    /// Last global decay
    last_decay: RwLock<Instant>,
}

impl TerrainMap {
    pub fn new() -> Self {
        Self {
            trails: RwLock::new(HashMap::new()),
            providers_at: RwLock::new(HashMap::new()),
            last_decay: RwLock::new(Instant::now()),
        }
    }

    /// Register a provider at a terrain coordinate
    pub fn register_provider(&self, coord: TerrainCoord, provider_id: [u8; 32]) {
        let mut providers = self.providers_at.write();
        providers
            .entry(coord.clone())
            .or_insert_with(Vec::new)
            .push(provider_id);

        // Initialize trail
        let edge = TerrainEdge { coord, provider_id };
        self.trails.write().entry(edge).or_insert_with(PheromoneTrail::default);
    }

    /// Remove a provider
    pub fn remove_provider(&self, provider_id: &[u8; 32]) {
        let mut providers = self.providers_at.write();
        for (_, list) in providers.iter_mut() {
            list.retain(|p| p != provider_id);
        }

        let mut trails = self.trails.write();
        trails.retain(|edge, _| &edge.provider_id != provider_id);
    }

    /// Get providers at a coordinate
    pub fn providers_at(&self, coord: &TerrainCoord) -> Vec<[u8; 32]> {
        self.providers_at
            .read()
            .get(coord)
            .cloned()
            .unwrap_or_default()
    }

    /// Get pheromone strength for an edge
    pub fn pheromone_strength(&self, coord: &TerrainCoord, provider_id: &[u8; 32]) -> f64 {
        let edge = TerrainEdge {
            coord: coord.clone(),
            provider_id: *provider_id,
        };
        
        self.trails
            .read()
            .get(&edge)
            .map(|t| {
                // Apply decay in calculation
                let elapsed = t.last_update.elapsed().as_secs_f64();
                (t.strength * (-PHEROMONE_DECAY_RATE * elapsed).exp()).max(MIN_PHEROMONE)
            })
            .unwrap_or(MIN_PHEROMONE)
    }

    /// Deposit pheromone (successful inference)
    pub fn deposit(&self, coord: &TerrainCoord, provider_id: &[u8; 32], amount: f64) {
        let edge = TerrainEdge {
            coord: coord.clone(),
            provider_id: *provider_id,
        };

        let mut trails = self.trails.write();
        trails
            .entry(edge)
            .or_insert_with(PheromoneTrail::default)
            .deposit(amount);
    }

    /// Evaporate pheromone (failed inference)
    pub fn evaporate(&self, coord: &TerrainCoord, provider_id: &[u8; 32], amount: f64) {
        let edge = TerrainEdge {
            coord: coord.clone(),
            provider_id: *provider_id,
        };

        let mut trails = self.trails.write();
        if let Some(trail) = trails.get_mut(&edge) {
            trail.evaporate(amount);
        }
    }

    /// Get trail statistics
    pub fn trail_stats(&self, coord: &TerrainCoord, provider_id: &[u8; 32]) -> Option<(u64, u64)> {
        let edge = TerrainEdge {
            coord: coord.clone(),
            provider_id: *provider_id,
        };

        self.trails
            .read()
            .get(&edge)
            .map(|t| (t.successes, t.failures))
    }

    /// Run global decay on all trails
    pub fn global_decay(&self) {
        let mut trails = self.trails.write();
        for trail in trails.values_mut() {
            trail.decay();
        }
        *self.last_decay.write() = Instant::now();
    }

    /// Get terrain statistics
    pub fn stats(&self) -> TerrainStats {
        let trails = self.trails.read();
        let providers = self.providers_at.read();

        TerrainStats {
            trail_count: trails.len(),
            coord_count: providers.len(),
            provider_count: providers.values().map(|v| v.len()).sum(),
            avg_pheromone: if trails.is_empty() {
                0.0
            } else {
                trails.values().map(|t| t.strength).sum::<f64>() / trails.len() as f64
            },
        }
    }
}

impl Default for TerrainMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Terrain statistics
#[derive(Debug, Clone)]
pub struct TerrainStats {
    pub trail_count: usize,
    pub coord_count: usize,
    pub provider_count: usize,
    pub avg_pheromone: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terrain_coord() {
        let coord1 = TerrainCoord::new("llama-3", 0b0001);
        let coord2 = TerrainCoord::new("llama-3", 0b0001);
        let coord3 = TerrainCoord::new("gpt-4", 0b0001);

        assert_eq!(coord1, coord2);
        assert_ne!(coord1, coord3);
    }

    #[test]
    fn test_pheromone_deposit() {
        let mut trail = PheromoneTrail::default();
        assert_eq!(trail.strength, MIN_PHEROMONE);

        trail.deposit(10.0);
        assert!(trail.strength > MIN_PHEROMONE);
        assert_eq!(trail.successes, 1);
    }

    #[test]
    fn test_terrain_map() {
        let map = TerrainMap::new();
        let coord = TerrainCoord::new("llama-3", 0b0001);
        let provider = [1u8; 32];

        map.register_provider(coord.clone(), provider);

        let providers = map.providers_at(&coord);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0], provider);

        // Deposit pheromone
        map.deposit(&coord, &provider, 5.0);
        let strength = map.pheromone_strength(&coord, &provider);
        assert!(strength > MIN_PHEROMONE);
    }
}
