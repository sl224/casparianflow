//! Protocol error types

use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, ProtocolError>;

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid OpCode: {0}")]
    InvalidOpCode(u8),

    #[error("Header too short: expected {expected} bytes, got {got}")]
    HeaderTooShort { expected: usize, got: usize },

    #[error("Protocol version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: u8, got: u8 },

    #[error("Invalid frame count: expected {expected}, got {got}")]
    InvalidFrameCount { expected: usize, got: usize },

    #[error("Payload length mismatch: expected {expected} bytes, got {got}")]
    PayloadLengthMismatch { expected: usize, got: usize },

    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
}
