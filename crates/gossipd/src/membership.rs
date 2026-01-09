//! Membership and control-plane gating

use blake3::Hasher;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Membership errors
#[derive(Debug, Error)]
pub enum MembershipError {
    #[error("Invalid world phrase")]
    InvalidWorldPhrase,
    #[error("Peer not admitted: {0:?}")]
    NotAdmitted([u8; 32]),
    #[error("Peer banned: {0:?}")]
    Banned([u8; 32]),
    #[error("Rate limited")]
    RateLimited,
}

/// Member status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberStatus {
    /// Pending admission
    Pending,
    /// Admitted member
    Admitted,
    /// Temporarily suspended
    Suspended { until: Instant },
    /// Permanently banned
    Banned,
}

/// Member information
#[derive(Debug, Clone)]
pub struct Member {
    /// Peer public key
    pub pubkey: [u8; 32],
    /// Current status
    pub status: MemberStatus,
    /// When the member joined
    pub joined_at: Instant,
    /// Last activity timestamp
    pub last_seen: Instant,
    /// Number of events contributed
    pub event_count: u64,
    /// Reputation score (0.0-1.0)
    pub reputation: f64,
}

/// Membership manager with control-plane gating
pub struct MembershipManager {
    /// World ID derived from phrase
    world_id: [u8; 32],
    /// World admission phrase
    world_phrase: String,
    /// Member registry
    members: RwLock<HashMap<[u8; 32], Member>>,
    /// Banned peers (permanent)
    banned: RwLock<HashSet<[u8; 32]>>,
    /// Rate limiting state
    rate_limits: RwLock<HashMap<[u8; 32], RateLimitState>>,
    /// Maximum requests per minute
    rate_limit_rpm: u32,
}

#[derive(Debug, Clone)]
struct RateLimitState {
    count: u32,
    window_start: Instant,
}

impl MembershipManager {
    /// Create a new membership manager
    pub fn new(world_phrase: impl Into<String>, rate_limit_rpm: u32) -> Self {
        let phrase = world_phrase.into();
        let world_id = Self::derive_world_id(&phrase);

        Self {
            world_id,
            world_phrase: phrase,
            members: RwLock::new(HashMap::new()),
            banned: RwLock::new(HashSet::new()),
            rate_limits: RwLock::new(HashMap::new()),
            rate_limit_rpm,
        }
    }

    /// Derive world ID from phrase using BLAKE3
    fn derive_world_id(phrase: &str) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(b"terrain-gossip-world-v1:");
        hasher.update(phrase.as_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Get the world ID
    pub fn world_id(&self) -> [u8; 32] {
        self.world_id
    }

    /// Verify world phrase and admit peer
    pub fn admit_peer(&self, pubkey: [u8; 32], offered_phrase: &str) -> Result<(), MembershipError> {
        // Check if banned
        if self.banned.read().contains(&pubkey) {
            return Err(MembershipError::Banned(pubkey));
        }

        // Verify phrase
        if offered_phrase != self.world_phrase {
            return Err(MembershipError::InvalidWorldPhrase);
        }

        // Admit member
        let now = Instant::now();
        let member = Member {
            pubkey,
            status: MemberStatus::Admitted,
            joined_at: now,
            last_seen: now,
            event_count: 0,
            reputation: 1.0,
        };

        self.members.write().insert(pubkey, member);
        Ok(())
    }

    /// Check if peer is admitted
    pub fn is_admitted(&self, pubkey: &[u8; 32]) -> bool {
        self.members
            .read()
            .get(pubkey)
            .map(|m| matches!(m.status, MemberStatus::Admitted))
            .unwrap_or(false)
    }

    /// Check peer authorization (with rate limiting)
    pub fn check_authorized(&self, pubkey: &[u8; 32]) -> Result<(), MembershipError> {
        // Check banned
        if self.banned.read().contains(pubkey) {
            return Err(MembershipError::Banned(*pubkey));
        }

        // Check admitted
        let members = self.members.read();
        match members.get(pubkey) {
            None => return Err(MembershipError::NotAdmitted(*pubkey)),
            Some(m) => match &m.status {
                MemberStatus::Admitted => {}
                MemberStatus::Suspended { until } if Instant::now() >= *until => {}
                MemberStatus::Suspended { .. } => {
                    return Err(MembershipError::NotAdmitted(*pubkey))
                }
                MemberStatus::Banned => return Err(MembershipError::Banned(*pubkey)),
                MemberStatus::Pending => return Err(MembershipError::NotAdmitted(*pubkey)),
            },
        }
        drop(members);

        // Check rate limit
        self.check_rate_limit(pubkey)?;

        // Update last seen
        if let Some(member) = self.members.write().get_mut(pubkey) {
            member.last_seen = Instant::now();
        }

        Ok(())
    }

    /// Check rate limit for a peer
    fn check_rate_limit(&self, pubkey: &[u8; 32]) -> Result<(), MembershipError> {
        let now = Instant::now();
        let window = Duration::from_secs(60);

        let mut limits = self.rate_limits.write();
        let state = limits.entry(*pubkey).or_insert(RateLimitState {
            count: 0,
            window_start: now,
        });

        // Reset window if expired
        if now.duration_since(state.window_start) >= window {
            state.count = 0;
            state.window_start = now;
        }

        // Check limit
        if state.count >= self.rate_limit_rpm {
            return Err(MembershipError::RateLimited);
        }

        state.count += 1;
        Ok(())
    }

    /// Update member's event count
    pub fn record_event(&self, pubkey: &[u8; 32]) {
        if let Some(member) = self.members.write().get_mut(pubkey) {
            member.event_count += 1;
        }
    }

    /// Suspend a peer temporarily
    pub fn suspend_peer(&self, pubkey: &[u8; 32], duration: Duration) {
        if let Some(member) = self.members.write().get_mut(pubkey) {
            member.status = MemberStatus::Suspended {
                until: Instant::now() + duration,
            };
        }
    }

    /// Ban a peer permanently
    pub fn ban_peer(&self, pubkey: &[u8; 32]) {
        self.banned.write().insert(*pubkey);
        if let Some(member) = self.members.write().get_mut(pubkey) {
            member.status = MemberStatus::Banned;
        }
    }

    /// Update reputation for a peer
    pub fn update_reputation(&self, pubkey: &[u8; 32], delta: f64) {
        if let Some(member) = self.members.write().get_mut(pubkey) {
            member.reputation = (member.reputation + delta).clamp(0.0, 1.0);
        }
    }

    /// Get member count
    pub fn member_count(&self) -> usize {
        self.members
            .read()
            .values()
            .filter(|m| matches!(m.status, MemberStatus::Admitted))
            .count()
    }

    /// List all admitted members
    pub fn list_members(&self) -> Vec<[u8; 32]> {
        self.members
            .read()
            .iter()
            .filter(|(_, m)| matches!(m.status, MemberStatus::Admitted))
            .map(|(k, _)| *k)
            .collect()
    }

    /// Get membership stats
    pub fn stats(&self) -> MembershipStats {
        let members = self.members.read();
        MembershipStats {
            total: members.len(),
            admitted: members
                .values()
                .filter(|m| matches!(m.status, MemberStatus::Admitted))
                .count(),
            suspended: members
                .values()
                .filter(|m| matches!(m.status, MemberStatus::Suspended { .. }))
                .count(),
            banned: self.banned.read().len(),
        }
    }
}

/// Membership statistics
#[derive(Debug, Clone)]
pub struct MembershipStats {
    pub total: usize,
    pub admitted: usize,
    pub suspended: usize,
    pub banned: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_id_derivation() {
        let manager = MembershipManager::new("test-world", 100);
        let world_id = manager.world_id();
        
        // Same phrase should produce same ID
        let manager2 = MembershipManager::new("test-world", 100);
        assert_eq!(world_id, manager2.world_id());

        // Different phrase should produce different ID
        let manager3 = MembershipManager::new("other-world", 100);
        assert_ne!(world_id, manager3.world_id());
    }

    #[test]
    fn test_admission() {
        let manager = MembershipManager::new("secret-phrase", 100);
        let pubkey = [1; 32];

        // Wrong phrase should fail
        assert!(manager.admit_peer(pubkey, "wrong").is_err());

        // Correct phrase should succeed
        assert!(manager.admit_peer(pubkey, "secret-phrase").is_ok());
        assert!(manager.is_admitted(&pubkey));
    }

    #[test]
    fn test_ban() {
        let manager = MembershipManager::new("phrase", 100);
        let pubkey = [2; 32];

        manager.admit_peer(pubkey, "phrase").unwrap();
        assert!(manager.is_admitted(&pubkey));

        manager.ban_peer(&pubkey);
        assert!(!manager.is_admitted(&pubkey));

        // Re-admission should fail
        assert!(manager.admit_peer(pubkey, "phrase").is_err());
    }

    #[test]
    fn test_rate_limiting() {
        let manager = MembershipManager::new("phrase", 3); // 3 requests per minute
        let pubkey = [3; 32];

        manager.admit_peer(pubkey, "phrase").unwrap();

        // First 3 should succeed
        assert!(manager.check_authorized(&pubkey).is_ok());
        assert!(manager.check_authorized(&pubkey).is_ok());
        assert!(manager.check_authorized(&pubkey).is_ok());

        // 4th should be rate limited
        assert!(matches!(
            manager.check_authorized(&pubkey),
            Err(MembershipError::RateLimited)
        ));
    }
}
