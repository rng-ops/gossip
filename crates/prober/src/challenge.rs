//! Probe challenge generation

use blake3::Hasher;
use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Challenge errors
#[derive(Debug, Error)]
pub enum ChallengeError {
    #[error("Invalid challenge format")]
    InvalidFormat,
    #[error("Challenge expired")]
    Expired,
    #[error("Invalid response")]
    InvalidResponse,
}

/// A probe challenge
#[derive(Debug, Clone)]
pub struct Challenge {
    /// Unique challenge ID
    pub id: [u8; 32],
    /// Challenge nonce
    pub nonce: [u8; 32],
    /// Target provider ID
    pub target_provider: [u8; 32],
    /// Challenge prompt tokens
    pub prompt_tokens: Vec<String>,
    /// Expected token count in response
    pub expected_token_count: usize,
    /// Challenge timestamp (Unix epoch seconds)
    pub timestamp: u64,
    /// Expiry timestamp
    pub expires_at: u64,
}

impl Challenge {
    /// Generate a new challenge for a provider
    pub fn generate(
        target_provider: [u8; 32],
        token_count: usize,
        ttl_secs: u64,
    ) -> Self {
        let mut rng = rand::thread_rng();

        // Generate nonce
        let mut nonce = [0u8; 32];
        rng.fill(&mut nonce);

        // Generate challenge ID
        let mut hasher = Hasher::new();
        hasher.update(&target_provider);
        hasher.update(&nonce);
        let id = *hasher.finalize().as_bytes();

        // Generate prompt tokens (simple token generation)
        let prompt_tokens: Vec<String> = (0..token_count)
            .map(|i| format!("probe_token_{:04x}", rng.gen::<u16>()))
            .collect();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id,
            nonce,
            target_provider,
            prompt_tokens,
            expected_token_count: token_count * 2, // Expect roughly 2x tokens back
            timestamp: now,
            expires_at: now + ttl_secs,
        }
    }

    /// Check if challenge is expired
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.expires_at
    }

    /// Get challenge as a prompt string
    pub fn as_prompt(&self) -> String {
        format!(
            "Complete this sequence: {}",
            self.prompt_tokens.join(" ")
        )
    }

    /// Compute challenge hash for verification
    pub fn challenge_hash(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(&self.id);
        hasher.update(&self.nonce);
        hasher.update(&self.target_provider);
        for token in &self.prompt_tokens {
            hasher.update(token.as_bytes());
        }
        *hasher.finalize().as_bytes()
    }
}

/// Challenge response from a provider
#[derive(Debug, Clone)]
pub struct ChallengeResponse {
    /// Challenge ID being responded to
    pub challenge_id: [u8; 32],
    /// Provider's response tokens
    pub response_tokens: Vec<String>,
    /// Response timestamp
    pub timestamp: u64,
    /// Provider's signature
    pub signature: Vec<u8>,
}

impl ChallengeResponse {
    /// Create a new response
    pub fn new(
        challenge_id: [u8; 32],
        response_tokens: Vec<String>,
        signature: Vec<u8>,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            challenge_id,
            response_tokens,
            timestamp,
            signature,
        }
    }

    /// Compute response hash for verification
    pub fn response_hash(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(&self.challenge_id);
        for token in &self.response_tokens {
            hasher.update(token.as_bytes());
        }
        hasher.update(&self.timestamp.to_le_bytes());
        *hasher.finalize().as_bytes()
    }
}

/// Challenge verifier
pub struct ChallengeVerifier {
    /// Minimum acceptable token count ratio
    min_token_ratio: f64,
    /// Maximum response latency (seconds)
    max_latency_secs: u64,
}

impl ChallengeVerifier {
    pub fn new(min_token_ratio: f64, max_latency_secs: u64) -> Self {
        Self {
            min_token_ratio,
            max_latency_secs,
        }
    }

    /// Verify a challenge response
    pub fn verify(
        &self,
        challenge: &Challenge,
        response: &ChallengeResponse,
    ) -> Result<VerificationResult, ChallengeError> {
        // Check challenge ID matches
        if challenge.id != response.challenge_id {
            return Err(ChallengeError::InvalidFormat);
        }

        // Check expiry
        if challenge.is_expired() {
            return Err(ChallengeError::Expired);
        }

        // Check response latency
        let latency_secs = response.timestamp.saturating_sub(challenge.timestamp);
        if latency_secs > self.max_latency_secs {
            return Err(ChallengeError::Expired);
        }

        // Check token count
        let token_ratio = response.response_tokens.len() as f64
            / challenge.expected_token_count as f64;

        let passed = token_ratio >= self.min_token_ratio;

        Ok(VerificationResult {
            passed,
            token_ratio,
            latency_secs,
            challenge_hash: challenge.challenge_hash(),
            response_hash: response.response_hash(),
        })
    }
}

impl Default for ChallengeVerifier {
    fn default() -> Self {
        Self::new(0.5, 60)
    }
}

/// Result of challenge verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether the challenge passed
    pub passed: bool,
    /// Token count ratio achieved
    pub token_ratio: f64,
    /// Response latency in seconds
    pub latency_secs: u64,
    /// Challenge hash
    pub challenge_hash: [u8; 32],
    /// Response hash
    pub response_hash: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_challenge_generation() {
        let provider = [1u8; 32];
        let challenge = Challenge::generate(provider, 5, 300);

        assert_eq!(challenge.target_provider, provider);
        assert_eq!(challenge.prompt_tokens.len(), 5);
        assert!(!challenge.is_expired());
    }

    #[test]
    fn test_challenge_prompt() {
        let challenge = Challenge::generate([0u8; 32], 3, 60);
        let prompt = challenge.as_prompt();

        assert!(prompt.starts_with("Complete this sequence:"));
    }

    #[test]
    fn test_verification() {
        let provider = [1u8; 32];
        let challenge = Challenge::generate(provider, 5, 300);
        
        let response = ChallengeResponse::new(
            challenge.id,
            vec!["a".to_string(); 10],
            vec![],
        );

        let verifier = ChallengeVerifier::default();
        let result = verifier.verify(&challenge, &response).unwrap();

        assert!(result.passed);
    }
}
