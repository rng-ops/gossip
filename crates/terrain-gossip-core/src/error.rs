//! Error types for TerrainGossip

use thiserror::Error;

/// Result type alias using our Error
pub type Result<T> = std::result::Result<T, Error>;

/// TerrainGossip error types
#[derive(Debug, Error)]
pub enum Error {
    /// Serialization/deserialization error
    #[error("serialization error: {0}")]
    Serialization(#[from] postcard::Error),

    /// Hash mismatch (computed != transmitted ID)
    #[error("hash mismatch: computed {computed} != transmitted {transmitted}")]
    HashMismatch { computed: String, transmitted: String },

    /// Invalid signature
    #[error("invalid signature")]
    InvalidSignature,

    /// Invalid public key
    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),

    /// Float normalization error (NaN, Inf, or -0.0)
    #[error("float normalization error: {0}")]
    FloatNormalization(String),

    /// Repeated field ordering violation
    #[error("repeated field not sorted/deduped: {field}")]
    UnsortedRepeatedField { field: String },

    /// Missing required field
    #[error("missing required field: {0}")]
    MissingField(String),
}
