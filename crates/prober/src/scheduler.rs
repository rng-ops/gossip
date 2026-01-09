//! Probe scheduling

use crate::challenge::{Challenge, ChallengeVerifier};
use crate::receipt::ProbeReceipt;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::debug;

/// Scheduler errors
#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("No providers available")]
    NoProviders,
    #[error("Provider already scheduled: {0:?}")]
    AlreadyScheduled([u8; 32]),
    #[error("Queue full")]
    QueueFull,
}

/// Provider priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbePriority {
    /// High priority - new or unreliable providers
    High,
    /// Normal priority - regular probing
    Normal,
    /// Low priority - well-established providers
    Low,
}

/// Scheduled probe task
#[derive(Debug, Clone)]
pub struct ScheduledProbe {
    /// Target provider ID
    pub provider_id: [u8; 32],
    /// Generated challenge
    pub challenge: Challenge,
    /// Priority level
    pub priority: ProbePriority,
    /// When the probe was scheduled
    pub scheduled_at: Instant,
    /// Number of retry attempts
    pub attempts: u32,
}

/// Provider probe history
#[derive(Debug, Clone, Default)]
pub struct ProbeHistory {
    /// Last probe timestamp
    pub last_probe: Option<Instant>,
    /// Total probes sent
    pub total_probes: u64,
    /// Successful probes
    pub successful: u64,
    /// Failed probes
    pub failed: u64,
    /// Consecutive failures
    pub consecutive_failures: u32,
}

impl ProbeHistory {
    /// Calculate probe interval based on history
    pub fn suggested_interval(&self) -> Duration {
        match self.consecutive_failures {
            0 => Duration::from_secs(300),  // 5 min for reliable
            1..=2 => Duration::from_secs(120),  // 2 min for slightly unreliable
            3..=5 => Duration::from_secs(60),   // 1 min for unreliable
            _ => Duration::from_secs(30),       // 30 sec for very unreliable
        }
    }

    /// Determine priority based on history
    pub fn priority(&self) -> ProbePriority {
        if self.total_probes < 5 {
            ProbePriority::High // New provider
        } else if self.consecutive_failures > 2 {
            ProbePriority::High // Unreliable
        } else if self.success_rate() > 0.95 {
            ProbePriority::Low // Very reliable
        } else {
            ProbePriority::Normal
        }
    }

    /// Success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_probes == 0 {
            0.0
        } else {
            self.successful as f64 / self.total_probes as f64
        }
    }

    /// Record probe result
    pub fn record(&mut self, passed: bool) {
        self.last_probe = Some(Instant::now());
        self.total_probes += 1;
        if passed {
            self.successful += 1;
            self.consecutive_failures = 0;
        } else {
            self.failed += 1;
            self.consecutive_failures += 1;
        }
    }
}

/// Probe scheduler
pub struct Scheduler {
    /// Known providers
    providers: RwLock<HashSet<[u8; 32]>>,
    /// Probe history per provider
    history: RwLock<HashMap<[u8; 32], ProbeHistory>>,
    /// Pending probes queue (priority ordered)
    high_priority: RwLock<VecDeque<ScheduledProbe>>,
    normal_priority: RwLock<VecDeque<ScheduledProbe>>,
    low_priority: RwLock<VecDeque<ScheduledProbe>>,
    /// Currently in-flight probes
    in_flight: RwLock<HashSet<[u8; 32]>>,
    /// Challenge verifier
    verifier: ChallengeVerifier,
    /// Configuration
    challenge_token_count: usize,
    probe_timeout_secs: u64,
    max_queue_size: usize,
}

impl Scheduler {
    pub fn new(
        challenge_token_count: usize,
        probe_timeout_secs: u64,
        max_queue_size: usize,
    ) -> Self {
        Self {
            providers: RwLock::new(HashSet::new()),
            history: RwLock::new(HashMap::new()),
            high_priority: RwLock::new(VecDeque::new()),
            normal_priority: RwLock::new(VecDeque::new()),
            low_priority: RwLock::new(VecDeque::new()),
            in_flight: RwLock::new(HashSet::new()),
            verifier: ChallengeVerifier::new(0.5, probe_timeout_secs),
            challenge_token_count,
            probe_timeout_secs,
            max_queue_size,
        }
    }

    /// Register a provider for probing
    pub fn register_provider(&self, provider_id: [u8; 32]) {
        self.providers.write().insert(provider_id);
        self.history.write().entry(provider_id).or_default();
    }

    /// Remove a provider
    pub fn remove_provider(&self, provider_id: &[u8; 32]) {
        self.providers.write().remove(provider_id);
    }

    /// Schedule a probe for a provider
    pub fn schedule_probe(&self, provider_id: [u8; 32]) -> Result<(), SchedulerError> {
        // Check if already in flight
        if self.in_flight.read().contains(&provider_id) {
            return Err(SchedulerError::AlreadyScheduled(provider_id));
        }

        // Get history and priority
        let priority = self
            .history
            .read()
            .get(&provider_id)
            .map(|h| h.priority())
            .unwrap_or(ProbePriority::High);

        // Generate challenge
        let challenge = Challenge::generate(
            provider_id,
            self.challenge_token_count,
            self.probe_timeout_secs,
        );

        let probe = ScheduledProbe {
            provider_id,
            challenge,
            priority,
            scheduled_at: Instant::now(),
            attempts: 0,
        };

        // Add to appropriate queue
        match priority {
            ProbePriority::High => {
                let mut queue = self.high_priority.write();
                if queue.len() >= self.max_queue_size {
                    return Err(SchedulerError::QueueFull);
                }
                queue.push_back(probe);
            }
            ProbePriority::Normal => {
                let mut queue = self.normal_priority.write();
                if queue.len() >= self.max_queue_size {
                    return Err(SchedulerError::QueueFull);
                }
                queue.push_back(probe);
            }
            ProbePriority::Low => {
                let mut queue = self.low_priority.write();
                if queue.len() >= self.max_queue_size {
                    return Err(SchedulerError::QueueFull);
                }
                queue.push_back(probe);
            }
        }

        Ok(())
    }

    /// Get next probe to execute (priority-ordered)
    pub fn next_probe(&self) -> Option<ScheduledProbe> {
        // Try high priority first
        if let Some(probe) = self.high_priority.write().pop_front() {
            self.in_flight.write().insert(probe.provider_id);
            return Some(probe);
        }

        // Then normal priority
        if let Some(probe) = self.normal_priority.write().pop_front() {
            self.in_flight.write().insert(probe.provider_id);
            return Some(probe);
        }

        // Finally low priority
        if let Some(probe) = self.low_priority.write().pop_front() {
            self.in_flight.write().insert(probe.provider_id);
            return Some(probe);
        }

        None
    }

    /// Report probe result
    pub fn report_result(
        &self,
        provider_id: &[u8; 32],
        passed: bool,
        prober_pubkey: [u8; 32],
    ) -> Option<ProbeReceipt> {
        // Remove from in-flight
        self.in_flight.write().remove(provider_id);

        // Update history
        if let Some(history) = self.history.write().get_mut(provider_id) {
            history.record(passed);
        }

        debug!(
            "Probe result for {:?}: {}",
            hex::encode(&provider_id[..8]),
            if passed { "PASS" } else { "FAIL" }
        );

        None // Would return receipt if we had the full challenge/response
    }

    /// Schedule probes for providers that need it
    pub fn schedule_due_probes(&self, max_count: usize) -> usize {
        let now = Instant::now();
        let providers: Vec<[u8; 32]> = self.providers.read().iter().copied().collect();
        
        let mut scheduled = 0;
        let mut candidates: Vec<([u8; 32], ProbePriority, Duration)> = Vec::new();

        for provider_id in providers {
            // Skip if already in queue or in flight
            if self.in_flight.read().contains(&provider_id) {
                continue;
            }

            let history = self.history.read();
            let hist = history.get(&provider_id);

            let should_probe = match hist {
                None => true,
                Some(h) => match h.last_probe {
                    None => true,
                    Some(last) => now.duration_since(last) >= h.suggested_interval(),
                },
            };

            if should_probe {
                let priority = hist.map(|h| h.priority()).unwrap_or(ProbePriority::High);
                let age = hist
                    .and_then(|h| h.last_probe)
                    .map(|lp| now.duration_since(lp))
                    .unwrap_or(Duration::from_secs(3600));
                candidates.push((provider_id, priority, age));
            }
        }

        // Sort by priority, then by age (oldest first)
        candidates.sort_by(|a, b| {
            match (a.1, b.1) {
                (ProbePriority::High, ProbePriority::High) => b.2.cmp(&a.2),
                (ProbePriority::High, _) => std::cmp::Ordering::Less,
                (_, ProbePriority::High) => std::cmp::Ordering::Greater,
                (ProbePriority::Normal, ProbePriority::Normal) => b.2.cmp(&a.2),
                (ProbePriority::Normal, _) => std::cmp::Ordering::Less,
                (_, ProbePriority::Normal) => std::cmp::Ordering::Greater,
                (ProbePriority::Low, ProbePriority::Low) => b.2.cmp(&a.2),
            }
        });

        for (provider_id, _, _) in candidates.into_iter().take(max_count) {
            if self.schedule_probe(provider_id).is_ok() {
                scheduled += 1;
            }
        }

        scheduled
    }

    /// Get scheduler statistics
    pub fn stats(&self) -> SchedulerStats {
        SchedulerStats {
            providers: self.providers.read().len(),
            high_priority_queued: self.high_priority.read().len(),
            normal_priority_queued: self.normal_priority.read().len(),
            low_priority_queued: self.low_priority.read().len(),
            in_flight: self.in_flight.read().len(),
        }
    }
}

/// Scheduler statistics
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    pub providers: usize,
    pub high_priority_queued: usize,
    pub normal_priority_queued: usize,
    pub low_priority_queued: usize,
    pub in_flight: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_history() {
        let mut history = ProbeHistory::default();
        assert_eq!(history.priority(), ProbePriority::High); // New provider

        for _ in 0..10 {
            history.record(true);
        }
        assert!(history.success_rate() == 1.0);
        assert_eq!(history.priority(), ProbePriority::Low);
    }

    #[test]
    fn test_scheduling() {
        let scheduler = Scheduler::new(5, 30, 100);
        let provider = [1u8; 32];

        scheduler.register_provider(provider);
        scheduler.schedule_probe(provider).unwrap();

        let probe = scheduler.next_probe();
        assert!(probe.is_some());
        assert_eq!(probe.unwrap().provider_id, provider);
    }

    #[test]
    fn test_due_probes() {
        let scheduler = Scheduler::new(5, 30, 100);

        for i in 0..5u8 {
            scheduler.register_provider([i; 32]);
        }

        let scheduled = scheduler.schedule_due_probes(3);
        assert!(scheduled <= 3);
    }
}
