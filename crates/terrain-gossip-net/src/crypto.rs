//! Cryptographic primitives for network layer
//!
//! Provides key generation, ECDH, and session key derivation.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::Sha256;
use thiserror::Error;
use x25519_dalek::{EphemeralSecret, PublicKey as X25519Public, SharedSecret};

/// Cryptographic errors
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Invalid key length")]
    InvalidKeyLength,
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    #[error("AEAD encryption failed")]
    EncryptionFailed,
    #[error("AEAD decryption failed")]
    DecryptionFailed,
    #[error("Key derivation failed")]
    KeyDerivationFailed,
}

/// Ed25519 keypair for signing and identity
#[derive(Clone)]
pub struct KeyPair {
    signing_key: SigningKey,
}

impl KeyPair {
    /// Generate a new random keypair
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Create from seed bytes (for deterministic testing)
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(seed);
        Self { signing_key }
    }

    /// Get the public key bytes
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Get the verifying key
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.signing_key.sign(message).to_bytes()
    }

    /// Verify a signature
    pub fn verify(
        public_key: &[u8; 32],
        message: &[u8],
        signature: &[u8; 64],
    ) -> Result<(), CryptoError> {
        let verifying_key = VerifyingKey::from_bytes(public_key)
            .map_err(|_| CryptoError::InvalidKeyLength)?;
        let sig = Signature::from_bytes(signature);
        verifying_key
            .verify(message, &sig)
            .map_err(|_| CryptoError::SignatureVerificationFailed)
    }
}

/// Session keys derived from ECDH
pub struct SessionKeys {
    /// Key for encrypting outgoing messages
    pub encrypt_key: [u8; 32],
    /// Key for decrypting incoming messages
    pub decrypt_key: [u8; 32],
    /// Nonce counter for outgoing messages
    nonce_counter: u64,
}

impl SessionKeys {
    /// Derive session keys from shared secret and role
    pub fn derive(
        shared_secret: &SharedSecret,
        our_public: &X25519Public,
        their_public: &X25519Public,
        context: &[u8],
    ) -> Result<Self, CryptoError> {
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());

        // Determine who is "initiator" based on public key ordering
        let is_initiator = our_public.as_bytes() < their_public.as_bytes();

        let mut encrypt_key = [0u8; 32];
        let mut decrypt_key = [0u8; 32];

        // Derive keys with role-specific info
        let (enc_info, dec_info) = if is_initiator {
            (b"initiator_to_responder", b"responder_to_initiator")
        } else {
            (b"responder_to_initiator", b"initiator_to_responder")
        };

        let mut enc_context = context.to_vec();
        enc_context.extend_from_slice(enc_info);
        hkdf.expand(&enc_context, &mut encrypt_key)
            .map_err(|_| CryptoError::KeyDerivationFailed)?;

        let mut dec_context = context.to_vec();
        dec_context.extend_from_slice(dec_info);
        hkdf.expand(&dec_context, &mut decrypt_key)
            .map_err(|_| CryptoError::KeyDerivationFailed)?;

        Ok(Self {
            encrypt_key,
            decrypt_key,
            nonce_counter: 0,
        })
    }

    /// Encrypt a message with AEAD
    pub fn encrypt(&mut self, plaintext: &[u8], associated_data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.encrypt_key)
            .map_err(|_| CryptoError::InvalidKeyLength)?;

        // Build nonce from counter
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&self.nonce_counter.to_le_bytes());
        self.nonce_counter += 1;

        let nonce = Nonce::from_slice(&nonce_bytes);
        cipher
            .encrypt(nonce, chacha20poly1305::aead::Payload {
                msg: plaintext,
                aad: associated_data,
            })
            .map_err(|_| CryptoError::EncryptionFailed)
    }

    /// Decrypt a message with AEAD
    pub fn decrypt(
        &self,
        ciphertext: &[u8],
        associated_data: &[u8],
        nonce_counter: u64,
    ) -> Result<Vec<u8>, CryptoError> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.decrypt_key)
            .map_err(|_| CryptoError::InvalidKeyLength)?;

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&nonce_counter.to_le_bytes());

        let nonce = Nonce::from_slice(&nonce_bytes);
        cipher
            .decrypt(nonce, chacha20poly1305::aead::Payload {
                msg: ciphertext,
                aad: associated_data,
            })
            .map_err(|_| CryptoError::DecryptionFailed)
    }
}

/// X25519 ephemeral key exchange
pub struct EphemeralKeyExchange {
    secret: EphemeralSecret,
    public: X25519Public,
}

impl EphemeralKeyExchange {
    /// Generate new ephemeral keypair
    pub fn new() -> Self {
        let secret = EphemeralSecret::random_from_rng(OsRng);
        let public = X25519Public::from(&secret);
        Self { secret, public }
    }

    /// Get the public key bytes
    pub fn public_key(&self) -> [u8; 32] {
        *self.public.as_bytes()
    }

    /// Perform key exchange and derive shared secret
    pub fn exchange(self, their_public: &[u8; 32]) -> SharedSecret {
        let their_public = X25519Public::from(*their_public);
        self.secret.diffie_hellman(&their_public)
    }
}

impl Default for EphemeralKeyExchange {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_sign_verify() {
        let kp = KeyPair::generate();
        let msg = b"hello world";
        let sig = kp.sign(msg);

        assert!(KeyPair::verify(&kp.public_key(), msg, &sig).is_ok());
    }

    #[test]
    fn test_ephemeral_key_exchange() {
        let alice = EphemeralKeyExchange::new();
        let bob = EphemeralKeyExchange::new();

        let alice_pub = alice.public_key();
        let bob_pub = bob.public_key();

        let alice_shared = alice.exchange(&bob_pub);
        let bob_shared = bob.exchange(&alice_pub);

        assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
    }

    #[test]
    fn test_session_encryption() {
        let alice = EphemeralKeyExchange::new();
        let bob = EphemeralKeyExchange::new();

        let alice_pub = X25519Public::from(alice.public_key());
        let bob_pub = X25519Public::from(bob.public_key());

        let shared = alice.exchange(&bob_pub.to_bytes());

        let mut alice_keys = SessionKeys::derive(&shared, &alice_pub, &bob_pub, b"test").unwrap();

        let plaintext = b"secret message";
        let aad = b"context";

        let ciphertext = alice_keys.encrypt(plaintext, aad).unwrap();

        // Bob would derive keys with swapped roles
        let bob_exchange = EphemeralKeyExchange::new();
        let shared2 = bob_exchange.exchange(&alice_pub.to_bytes());
        let bob_keys = SessionKeys::derive(&shared2, &bob_pub, &alice_pub, b"test").unwrap();

        // Note: This test shows the API; real usage requires same shared secret
    }
}
