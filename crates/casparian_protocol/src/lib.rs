//! Binary Protocol v4: The Split Plane Protocol
//!
//! Wire format for Sentinel <-> Worker communication.
//! Control Plane only - Data flows directly from Worker to Storage.
//!
//! # Protocol Specification
//!
//! Header Format: !BBHQI (16 bytes, Network Byte Order / Big Endian)
//! ```text
//! [VER:1][OP:1][RES:2][JOB_ID:8][LEN:4]
//! ```
//!
//! - VER (u8): Protocol version (0x04)
//! - OP (u8): OpCode
//! - RES (u16): Reserved for future use
//! - JOB_ID (u64): Job ID (Q = unsigned long long, 8 bytes)
//! - LEN (u32): Payload length in bytes (I = unsigned int, 4 bytes)

pub mod defaults;
pub mod error;
pub mod http_types;
pub mod idempotency;
pub mod metrics;
pub mod telemetry;
pub mod types;

// Re-export types for convenience
pub use types::{
    // Shredder types
    AnalysisResult,
    ArtifactKind,
    ArtifactV1,
    ColumnOrderMismatch,
    // Canonical enums (use these everywhere)
    DataType,
    // Protocol types
    DeployCommand,
    DeployResponse,
    DetectionConfidence,
    DispatchCommand,
    ErrorPayload,
    HeartbeatPayload,
    HeartbeatStatus,
    IdentifyPayload,
    JobDiagnostics,
    JobId,
    JobReceipt,
    JobStatus,
    LineageBlock,
    LineageChain,
    LineageFileType,
    LineageHop,
    LlmConfig,
    LlmProvider,
    ObservedColumn,
    ObservedDataType,
    PipelineRunStatus,
    PluginStatus,
    ProcessingStatus,
    QuarantineConfig,
    RuntimeKind,
    SchemaColumnSpec,
    SchemaDefinition,
    SchemaMismatch,
    ShardMeta,
    ShredConfig,
    ShredResult,
    ShredStrategy,
    SinkConfig,
    SinkMode,
    TypeMismatch,
    WorkerStatus,
};

pub use idempotency::{
    materialization_key, output_target_key, schema_hash, table_name_with_schema,
};

// Re-export HTTP API types
pub use http_types::{
    ApiJobId,
    // Approval types
    Approval,
    // API response types
    ApprovalDecideResponse,
    ApprovalDecision,
    ApprovalDecisionType,
    ApprovalOperation,
    ApprovalStatus,
    ControlPlaneDiscovery,
    CreateJobResponse,
    DatasetSummary,
    ErrorResponse,
    // Event types
    Event,
    EventId,
    EventType,
    HealthResponse,
    // Job types
    HttpJobStatus,
    HttpJobType,
    Job,
    JobProgress,
    JobResult,
    JobSpec,
    ListApprovalsResponse,
    ListDatasetsResponse,
    ListEventsResponse,
    ListJobsResponse,
    OutputInfo,
    QuarantineSummary,
    // Query types
    QueryRequest,
    QueryResponse,
    RedactionMode,
    RedactionPolicy,
    SchemaMode,
    SchemaSpec,
    VersionResponse,
    ViolationSummary,
    ViolationType,
};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use error::{ProtocolError, Result};
use std::io::Cursor;

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 0x04;

/// Header size in bytes
pub const HEADER_SIZE: usize = 16;

/// Split Plane Protocol OpCodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    Unknown = 0,

    // Worker -> Sentinel (Handshake)
    Identify = 1, // "I am here. My capabilities are [A, B, C]."

    // Sentinel -> Worker (Command)
    Dispatch = 2, // "Process this file. Here is your sink configuration."

    // Sentinel -> Worker (Abort)
    Abort = 3, // "Cancel this job."

    // Worker -> Sentinel (Keep-alive)
    Heartbeat = 4, // "Still alive, working on job X."

    // Worker -> Sentinel (Completion)
    Conclude = 5, // "Job finished. Here is the receipt."

    // Bidirectional (Error)
    Err = 6, // "Something went wrong."

    // Sentinel -> Worker (Config Refresh)
    Reload = 7, // "Reload configuration / plugins."

    // v5.0 Bridge Mode: Artifact Deployment
    Deploy = 10, // "Deploy this artifact (source + lockfile + signature)."
    Ack = 11,    // "Generic acknowledgment (used for DeployResponse, etc.)"
}

impl OpCode {
    /// Convert u8 to OpCode
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(OpCode::Unknown),
            1 => Ok(OpCode::Identify),
            2 => Ok(OpCode::Dispatch),
            3 => Ok(OpCode::Abort),
            4 => Ok(OpCode::Heartbeat),
            5 => Ok(OpCode::Conclude),
            6 => Ok(OpCode::Err),
            7 => Ok(OpCode::Reload),
            10 => Ok(OpCode::Deploy),
            11 => Ok(OpCode::Ack),
            _ => Err(ProtocolError::InvalidOpCode(value)),
        }
    }

    /// Convert OpCode to u8
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Protocol header
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: u8,
    pub opcode: OpCode,
    pub reserved: u16,
    pub job_id: JobId,
    pub payload_len: u32,
}

impl Header {
    /// Create a new header
    pub fn new(opcode: OpCode, job_id: JobId, payload_len: u32) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            opcode,
            reserved: 0,
            job_id,
            payload_len,
        }
    }

    /// Pack header into 16-byte buffer
    ///
    /// # Format
    /// Network Byte Order (Big Endian):
    /// - Version (u8): 1 byte
    /// - OpCode (u8): 1 byte
    /// - Reserved (u16): 2 bytes
    /// - Job ID (u64): 8 bytes
    /// - Payload Length (u32): 4 bytes
    ///
    /// Total: 16 bytes
    pub fn pack(&self) -> Result<[u8; HEADER_SIZE]> {
        let mut buf = [0u8; HEADER_SIZE];
        let mut cursor = Cursor::new(&mut buf[..]);

        cursor.write_u8(self.version)?;
        cursor.write_u8(self.opcode.as_u8())?;
        cursor.write_u16::<BigEndian>(self.reserved)?;
        cursor.write_u64::<BigEndian>(self.job_id.as_u64())?;
        cursor.write_u32::<BigEndian>(self.payload_len)?;

        Ok(buf)
    }

    /// Unpack header from 16-byte buffer
    pub fn unpack(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(ProtocolError::HeaderTooShort {
                expected: HEADER_SIZE,
                got: data.len(),
            });
        }

        let mut cursor = Cursor::new(&data[..HEADER_SIZE]);

        let version = cursor.read_u8()?;
        let op_raw = cursor.read_u8()?;
        let reserved = cursor.read_u16::<BigEndian>()?;
        let job_id = JobId::new(cursor.read_u64::<BigEndian>()?);
        let payload_len = cursor.read_u32::<BigEndian>()?;

        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: version,
            });
        }

        let opcode = OpCode::from_u8(op_raw)?;

        Ok(Self {
            version,
            opcode,
            reserved,
            job_id,
            payload_len,
        })
    }
}

/// Protocol message (header + payload)
#[derive(Debug, Clone)]
pub struct Message {
    pub header: Header,
    pub payload: Vec<u8>,
}

/// Maximum payload size (4GB - 1, the max value of u32)
pub const MAX_PAYLOAD_SIZE: usize = u32::MAX as usize;

impl Message {
    /// Create a new message
    ///
    /// Returns an error if payload exceeds MAX_PAYLOAD_SIZE (4GB).
    pub fn new(opcode: OpCode, job_id: JobId, payload: Vec<u8>) -> Result<Self> {
        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge {
                size: payload.len(),
                max: MAX_PAYLOAD_SIZE,
            });
        }
        let header = Header::new(opcode, job_id, payload.len() as u32);
        Ok(Self { header, payload })
    }

    /// Pack message into ZMQ frames (header, payload)
    pub fn pack(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let header_bytes = self.header.pack()?.to_vec();
        Ok((header_bytes, self.payload.clone()))
    }

    /// Unpack message from ZMQ frames
    pub fn unpack(frames: &[Vec<u8>]) -> Result<Self> {
        if frames.len() < 2 {
            return Err(ProtocolError::InvalidFrameCount {
                expected: 2,
                got: frames.len(),
            });
        }

        let header = Header::unpack(&frames[0])?;
        let payload = frames[1].clone();

        // Validate payload length
        if payload.len() != header.payload_len as usize {
            return Err(ProtocolError::PayloadLengthMismatch {
                expected: header.payload_len as usize,
                got: payload.len(),
            });
        }

        Ok(Self { header, payload })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_pack_unpack() {
        let header = Header::new(OpCode::Dispatch, JobId::new(12345), 1024);
        let packed = header.pack().unwrap();

        assert_eq!(packed.len(), HEADER_SIZE);

        let unpacked = Header::unpack(&packed).unwrap();
        assert_eq!(unpacked.version, PROTOCOL_VERSION);
        assert_eq!(unpacked.opcode, OpCode::Dispatch);
        assert_eq!(unpacked.job_id, JobId::new(12345));
        assert_eq!(unpacked.payload_len, 1024);
    }

    #[test]
    fn test_header_roundtrip() {
        for opcode in [
            OpCode::Identify,
            OpCode::Dispatch,
            OpCode::Heartbeat,
            OpCode::Conclude,
        ] {
            let header = Header::new(opcode, JobId::new(9999), 512);
            let packed = header.pack().unwrap();
            let unpacked = Header::unpack(&packed).unwrap();
            assert_eq!(header, unpacked);
        }
    }

    #[test]
    fn test_version_mismatch() {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0] = 0xFF; // Invalid version

        let result = Header::unpack(&buf);
        assert!(matches!(result, Err(ProtocolError::VersionMismatch { .. })));
    }

    #[test]
    fn test_header_too_short() {
        let buf = [0u8; 8]; // Only 8 bytes
        let result = Header::unpack(&buf);
        assert!(matches!(result, Err(ProtocolError::HeaderTooShort { .. })));
    }

    #[test]
    fn test_message_pack_unpack() {
        let payload = b"Hello, Protocol!".to_vec();
        let msg = Message::new(OpCode::Identify, JobId::new(42), payload.clone()).unwrap();

        let (header_bytes, payload_bytes) = msg.pack().unwrap();
        let frames = vec![header_bytes, payload_bytes];

        let unpacked = Message::unpack(&frames).unwrap();
        assert_eq!(unpacked.header.opcode, OpCode::Identify);
        assert_eq!(unpacked.header.job_id, JobId::new(42));
        assert_eq!(unpacked.payload, payload);
    }
}
