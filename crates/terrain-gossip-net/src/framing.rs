//! Message framing for network transport
//!
//! Provides length-prefixed framing and optional padding to fixed sizes.

use bytes::{Buf, BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::codec::{Decoder, Encoder};

/// Maximum frame size (16 MB)
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Framing errors
#[derive(Debug, Error)]
pub enum FrameError {
    #[error("Frame too large: {0} bytes (max {MAX_FRAME_SIZE})")]
    TooLarge(usize),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// A framed message
#[derive(Clone, Debug)]
pub struct Frame {
    /// Frame type
    pub frame_type: FrameType,
    /// Payload bytes
    pub payload: Vec<u8>,
}

/// Frame types
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum FrameType {
    /// Ping for keepalive
    Ping = 0,
    /// Pong response
    Pong = 1,
    /// Delta sync request
    DeltaSyncRequest = 10,
    /// Delta sync response
    DeltaSyncResponse = 11,
    /// Event broadcast
    EventBroadcast = 12,
    /// Descriptor query
    DescriptorQuery = 20,
    /// Descriptor response
    DescriptorResponse = 21,
    /// Circuit create
    CircuitCreate = 30,
    /// Circuit extend
    CircuitExtend = 31,
    /// Circuit data cell
    CircuitCell = 32,
    /// Circuit destroy
    CircuitDestroy = 33,
    /// Inference request (inside circuit)
    InferenceRequest = 40,
    /// Inference response (inside circuit)
    InferenceResponse = 41,
}

impl TryFrom<u8> for FrameType {
    type Error = FrameError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Ping),
            1 => Ok(Self::Pong),
            10 => Ok(Self::DeltaSyncRequest),
            11 => Ok(Self::DeltaSyncResponse),
            12 => Ok(Self::EventBroadcast),
            20 => Ok(Self::DescriptorQuery),
            21 => Ok(Self::DescriptorResponse),
            30 => Ok(Self::CircuitCreate),
            31 => Ok(Self::CircuitExtend),
            32 => Ok(Self::CircuitCell),
            33 => Ok(Self::CircuitDestroy),
            40 => Ok(Self::InferenceRequest),
            41 => Ok(Self::InferenceResponse),
            _ => Err(FrameError::Serialization(format!("Unknown frame type: {}", value))),
        }
    }
}

/// Codec for length-prefixed frames
///
/// Wire format:
/// - 4 bytes: length (big-endian, includes type byte)
/// - 1 byte: frame type
/// - N bytes: payload
pub struct FrameCodec {
    /// Fixed cell size (0 = no padding)
    fixed_cell_bytes: usize,
}

impl FrameCodec {
    /// Create a new codec
    pub fn new() -> Self {
        Self { fixed_cell_bytes: 0 }
    }

    /// Create codec with fixed-size cells
    pub fn with_fixed_cells(cell_size: usize) -> Self {
        Self {
            fixed_cell_bytes: cell_size,
        }
    }
}

impl Default for FrameCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = FrameError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Need at least 5 bytes (4 length + 1 type)
        if src.len() < 5 {
            return Ok(None);
        }

        // Peek at length
        let length = u32::from_be_bytes([src[0], src[1], src[2], src[3]]) as usize;

        if length > MAX_FRAME_SIZE {
            return Err(FrameError::TooLarge(length));
        }

        // Need full frame
        if src.len() < 4 + length {
            return Ok(None);
        }

        // Consume length prefix
        src.advance(4);

        // Read frame type
        let frame_type = FrameType::try_from(src[0])?;
        src.advance(1);

        // Read payload
        let payload_len = length - 1;
        let payload = src.split_to(payload_len).to_vec();

        Ok(Some(Frame { frame_type, payload }))
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = FrameError;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut payload = item.payload;

        // Apply padding if fixed cell size is set
        if self.fixed_cell_bytes > 0 && item.frame_type == FrameType::CircuitCell {
            if payload.len() > self.fixed_cell_bytes {
                return Err(FrameError::TooLarge(payload.len()));
            }
            payload.resize(self.fixed_cell_bytes, 0);
        }

        let length = 1 + payload.len();
        if length > MAX_FRAME_SIZE {
            return Err(FrameError::TooLarge(length));
        }

        dst.put_u32(length as u32);
        dst.put_u8(item.frame_type as u8);
        dst.put_slice(&payload);

        Ok(())
    }
}

impl Frame {
    /// Create a new frame
    pub fn new(frame_type: FrameType, payload: Vec<u8>) -> Self {
        Self { frame_type, payload }
    }

    /// Create a ping frame
    pub fn ping() -> Self {
        Self::new(FrameType::Ping, vec![])
    }

    /// Create a pong frame
    pub fn pong() -> Self {
        Self::new(FrameType::Pong, vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_roundtrip() {
        let mut codec = FrameCodec::new();
        let frame = Frame::new(FrameType::EventBroadcast, vec![1, 2, 3, 4, 5]);

        let mut buf = BytesMut::new();
        codec.encode(frame.clone(), &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.frame_type, frame.frame_type);
        assert_eq!(decoded.payload, frame.payload);
    }

    #[test]
    fn test_fixed_cell_padding() {
        let mut codec = FrameCodec::with_fixed_cells(512);
        let frame = Frame::new(FrameType::CircuitCell, vec![1, 2, 3]);

        let mut buf = BytesMut::new();
        codec.encode(frame, &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.payload.len(), 512);
        assert_eq!(&decoded.payload[..3], &[1, 2, 3]);
    }
}
