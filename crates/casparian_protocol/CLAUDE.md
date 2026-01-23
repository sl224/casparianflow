# Claude Code Instructions for casparian_protocol

## Quick Reference

```bash
cargo test -p casparian_protocol              # All tests
cargo test -p casparian_protocol -- http      # HTTP types tests
```

---

## Overview

`casparian_protocol` is the **Protocol crate** containing:
1. **Binary Protocol** - Wire format for Sentinel ↔ Worker communication (ZMQ)
2. **HTTP Types** - Types for the Control Plane API (used by MCP)
3. **Shared Types** - Canonical enums and data types used across crates

---

## Module Structure

```
crates/casparian_protocol/
├── CLAUDE.md                 # This file
├── Cargo.toml
├── src/
│   ├── lib.rs                # Crate root with re-exports
│   ├── types.rs              # Core types (JobId, DataType, enums)
│   ├── http_types.rs         # Control Plane API types
│   ├── error.rs              # Protocol errors
│   └── idempotency.rs        # Hash functions for deduplication
```

---

## Binary Protocol (Sentinel ↔ Worker)

### Wire Format

Header: 16 bytes, Network Byte Order (Big Endian)
```
[VER:1][OP:1][RES:2][JOB_ID:8][LEN:4]
```

| Field | Size | Description |
|-------|------|-------------|
| VER | 1 byte | Protocol version (0x04) |
| OP | 1 byte | OpCode |
| RES | 2 bytes | Reserved |
| JOB_ID | 8 bytes | Job ID (u64) |
| LEN | 4 bytes | Payload length |

### OpCodes

```rust
pub enum OpCode {
    Unknown = 0,
    Identify = 1,    // Worker → Sentinel: "I am here"
    Dispatch = 2,    // Sentinel → Worker: "Process this file"
    Abort = 3,       // Sentinel → Worker: "Cancel this job"
    Heartbeat = 4,   // Worker → Sentinel: "Still alive"
    Conclude = 5,    // Worker → Sentinel: "Job finished"
    Err = 6,         // Bidirectional: "Error occurred"
    Reload = 7,      // Sentinel → Worker: "Reload config"
    Deploy = 10,     // Sentinel → Worker: "Deploy artifact"
    Ack = 11,        // Generic acknowledgment
}
```

### Usage

```rust
use casparian_protocol::{Header, Message, OpCode, JobId, HEADER_SIZE};

// Create message
let payload = serde_json::to_vec(&my_data)?;
let msg = Message::new(OpCode::Dispatch, JobId::new(12345), payload)?;

// Pack for ZMQ
let (header_bytes, payload_bytes) = msg.pack()?;
socket.send_multipart(&[header_bytes, payload_bytes])?;

// Unpack from ZMQ
let frames = socket.recv_multipart()?;
let msg = Message::unpack(&frames)?;
assert_eq!(msg.header.opcode, OpCode::Conclude);
```

---

## HTTP Types (Control Plane API)

Located in `src/http_types.rs`. These types are used by `casparian_mcp` and `casparian_sentinel::ApiStorage`.

### Job Types

```rust
/// Job type for the HTTP API
pub enum HttpJobType {
    Run,       // Parser execution
    Backtest,  // Multi-file validation
    Preview,   // Preview (no output)
}

/// Job status
pub enum HttpJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Full job record
pub struct Job {
    pub job_id: JobId,
    pub job_type: HttpJobType,
    pub status: HttpJobStatus,
    pub plugin_name: String,
    pub plugin_version: Option<String>,
    pub input_dir: String,
    pub output: Option<String>,
    pub created_at: String,           // RFC3339
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_message: Option<String>,
    pub approval_id: Option<String>,
    pub progress: Option<JobProgress>,
    pub result: Option<JobResult>,
}
```

### Event Types

```rust
/// Event ID (monotonic per job)
pub type EventId = u64;

/// Event types emitted during job execution
pub enum EventType {
    JobStarted,
    Phase { name: String },
    Progress {
        items_done: u64,
        items_total: Option<u64>,
        message: Option<String>,
    },
    Violation { violations: Vec<ViolationSummary> },
    Output {
        output_name: String,
        sink_uri: String,
        rows: u64,
        bytes: Option<u64>,
    },
    JobFinished {
        status: HttpJobStatus,
        error_message: Option<String>,
    },
    ApprovalRequired { approval_id: String },
}

/// Event record
pub struct Event {
    pub event_id: EventId,
    pub job_id: JobId,
    pub timestamp: String,  // RFC3339
    pub event_type: EventType,
}
```

### Approval Types

```rust
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

pub enum ApprovalOperation {
    Run {
        plugin_name: String,
        plugin_version: Option<String>,
        input_dir: String,
        file_count: u64,
        output: Option<String>,
    },
    SchemaPromote {
        plugin_name: String,
        output_name: String,
        schema: SchemaSpec,
    },
}

pub struct Approval {
    pub approval_id: String,
    pub status: ApprovalStatus,
    pub operation: ApprovalOperation,
    pub summary: String,
    pub created_at: String,
    pub expires_at: String,
    pub decided_at: Option<String>,
    pub decided_by: Option<String>,
    pub rejection_reason: Option<String>,
    pub job_id: Option<JobId>,
}
```

### Redaction Types

```rust
pub enum RedactionMode {
    None,      // Raw values (explicit opt-in)
    Truncate,  // First N chars
    Hash,      // SHA256 prefix (default)
}

pub struct RedactionPolicy {
    pub mode: RedactionMode,
    pub max_sample_count: usize,    // Default: 5
    pub max_value_length: usize,    // Default: 100
}
```

### Query Types

```rust
pub struct QueryRequest {
    pub sql: String,
    pub limit: usize,           // Default: 1000
    pub timeout_ms: u64,        // Default: 30000
    pub redaction: RedactionPolicy,
}

pub struct QueryResponse {
    pub columns: Vec<String>,
    pub types: Vec<DataType>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub truncated: bool,
    pub execution_ms: u64,
}
```

---

## Core Types

Located in `src/types.rs`:

### JobId

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(u64);

impl JobId {
    pub fn new(id: u64) -> Self { Self(id) }
    pub fn as_u64(&self) -> u64 { self.0 }
}
```

### DataType

```rust
pub enum DataType {
    Null,
    Boolean,
    Int8, Int16, Int32, Int64,
    UInt8, UInt16, UInt32, UInt64,
    Float32, Float64,
    String,
    Binary,
    Date,
    Time,
    Timestamp,
    Duration,
    List(Box<DataType>),
    Struct(Vec<(String, DataType)>),
}
```

### Status Enums

```rust
/// Processing status (internal)
pub enum ProcessingStatus {
    Pending,
    Queued,
    Staged,
    Running,
    Completed,
    Failed,
    Skipped,
}

/// Worker status
pub enum WorkerStatus {
    Idle,
    Busy,
    Draining,
    Offline,
}

/// Plugin status
pub enum PluginStatus {
    Active,
    Deprecated,
    Disabled,
}
```

---

## Serde Conventions

All types use strict serde tagging:

```rust
// Enums use snake_case
#[serde(rename_all = "snake_case")]
pub enum HttpJobStatus {
    Queued,     // "queued"
    Running,    // "running"
    Completed,  // "completed"
}

// Tagged enums use "type" field
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventType {
    JobStarted,                    // {"type": "job_started"}
    Progress { items_done: u64 },  // {"type": "progress", "items_done": 100}
}

// Optional fields skip if None
#[serde(skip_serializing_if = "Option::is_none")]
pub plugin_version: Option<String>,
```

---

## Testing

```bash
# All tests
cargo test -p casparian_protocol

# Specific tests
cargo test -p casparian_protocol -- header
cargo test -p casparian_protocol -- event_type
cargo test -p casparian_protocol -- redaction
```

### Test Examples

```rust
#[test]
fn test_event_type_serialization() {
    let event = EventType::Progress {
        items_done: 100,
        items_total: Some(1000),
        message: Some("Processing".to_string()),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"progress\""));
    assert!(json.contains("\"items_done\":100"));
}

#[test]
fn test_header_roundtrip() {
    let header = Header::new(OpCode::Dispatch, JobId::new(12345), 1024);
    let packed = header.pack().unwrap();
    let unpacked = Header::unpack(&packed).unwrap();
    assert_eq!(header, unpacked);
}
```

---

## Common Tasks

### Add a New Event Type

1. Add variant to `EventType`:
   ```rust
   pub enum EventType {
       // ...
       MyNewEvent {
           field1: String,
           field2: Option<u64>,
       },
   }
   ```

2. Update consumers:
   - `casparian_sentinel/src/db/api_storage.rs` - `event_type_to_str()`
   - DDL in `init_schema()` - add to CHECK constraint

### Add a New Approval Operation

1. Add variant to `ApprovalOperation`:
   ```rust
   pub enum ApprovalOperation {
       // ...
       MyNewOp {
           field1: String,
       },
   }
   ```

2. Update `casparian_sentinel/src/db/api_storage.rs`:
   - `create_approval()` - handle new operation type

### Add a New HTTP Response Type

1. Add struct in `http_types.rs`:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct MyNewResponse {
       pub field: String,
   }
   ```

2. Re-export in `lib.rs`:
   ```rust
   pub use http_types::MyNewResponse;
   ```

---

## Key Principles

1. **Strict typing** - Use enums over strings, newtypes over primitives
2. **Serde conventions** - snake_case, tagged enums, skip None fields
3. **Single source of truth** - Protocol types are canonical
4. **Backward compatibility** - Not required pre-v1, but type changes should update all consumers
