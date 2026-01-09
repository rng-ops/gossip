//! Networking primitives for TerrainGossip protocol
//!
//! This crate provides:
//! - Transport keypair management
//! - QUIC-based secure transport
//! - Onion circuit construction and relay
//! - Message framing and encryption

pub mod circuit;
pub mod crypto;
pub mod framing;
pub mod peer;
pub mod transport;

pub use circuit::{Circuit, CircuitBuilder, CircuitHop};
pub use crypto::{KeyPair, SessionKeys};
pub use framing::{Frame, FrameCodec};
pub use peer::{PeerId, PeerInfo};
pub use transport::Transport;
