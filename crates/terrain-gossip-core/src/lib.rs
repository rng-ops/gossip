//! TerrainGossip Core Library
//!
//! This crate provides the core types, canonical encoding, and cryptographic
//! primitives for the TerrainGossip LLM Mesh Protocol (RFC-0001).
//!
//! # Modules
//!
//! - [`types`]: Core protocol types (WorldId, TargetRef, ProbeReceipt, etc.)
//! - [`canonical`]: Deterministic serialization for hashing/signing
//! - [`crypto`]: Hash derivations and signature utilities
//! - [`error`]: Error types

pub mod canonical;
pub mod crypto;
pub mod error;
pub mod types;

#[cfg(test)]
mod test_vectors;

pub use error::{Error, Result};
pub use types::*;
