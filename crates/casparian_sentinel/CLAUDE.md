# Claude Code Instructions for casparian_sentinel

## Quick Reference

```bash
cargo test -p casparian_sentinel              # All tests
cargo test -p casparian_sentinel -- api       # ApiStorage tests
```

---

## Overview

`casparian_sentinel` is the **Sentinel crate** containing:
1. **Job Queue** - Manages parser job dispatch to workers
2. **API Storage** - DuckDB-backed storage for Control Plane API
3. **Sentinel Service** - ZMQ-based orchestration (Sentinel → Workers)
4. **Metrics** - Prometheus metrics for monitoring

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    casparian_sentinel                            │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Sentinel Service                       │   │
│  │  • ZMQ router for worker communication                   │   │
│  │  • Job dispatch and lifecycle management                 │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Database Layer                         │   │
│  │                                                          │   │
│  │  ┌────────────────┐    ┌─────────────────────────────┐  │   │
│  │  │   JobQueue     │    │      ApiStorage              │  │   │
│  │  │  (legacy jobs) │    │  (MCP jobs/events/approvals) │  │   │
│  │  └────────────────┘    └─────────────────────────────┘  │   │
│  │                              │                           │   │
│  │                              ▼                           │   │
│  │                    DuckDB (casparian_flow.duckdb)       │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Module Structure

```
crates/casparian_sentinel/
├── CLAUDE.md                 # This file
├── Cargo.toml
├── src/
│   ├── lib.rs                # Crate root with re-exports
│   ├── sentinel.rs           # Sentinel service (ZMQ router)
│   ├── metrics.rs            # Prometheus metrics
│   └── db/
│       ├── mod.rs            # Database module root
│       ├── queue.rs          # JobQueue (legacy job management)
│       └── api_storage.rs    # ApiStorage (MCP Control Plane API)
```

---

## ApiStorage (Control Plane API)

The `ApiStorage` struct is the primary interface for MCP to manage jobs, events, and approvals.

### Location

`src/db/api_storage.rs`

### Tables

```sql
-- API Jobs table
CREATE TABLE cf_api_jobs (
    job_id BIGINT PRIMARY KEY,
    job_type TEXT NOT NULL,           -- 'run', 'backtest', 'preview'
    status TEXT NOT NULL,             -- 'queued', 'running', 'completed', 'failed', 'cancelled'
    plugin_name TEXT NOT NULL,
    plugin_version TEXT,
    input_dir TEXT NOT NULL,
    output_sink TEXT,
    approval_id TEXT,
    created_at TIMESTAMP,
    started_at TIMESTAMP,
    finished_at TIMESTAMP,
    error_message TEXT,
    progress_phase TEXT,
    progress_items_done BIGINT,
    progress_items_total BIGINT,
    progress_message TEXT,
    result_rows_processed BIGINT,
    result_bytes_written BIGINT,
    result_outputs_json TEXT,
    result_metrics_json TEXT
);

-- API Events table (monotonic event_id per job)
CREATE TABLE cf_api_events (
    id BIGINT PRIMARY KEY,
    event_id BIGINT NOT NULL,         -- Monotonic per job
    job_id BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    timestamp TIMESTAMP,
    payload_json TEXT NOT NULL,
    UNIQUE(job_id, event_id)
);

-- API Approvals table
CREATE TABLE cf_api_approvals (
    approval_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,             -- 'pending', 'approved', 'rejected', 'expired'
    operation_type TEXT NOT NULL,
    operation_json TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TIMESTAMP,
    expires_at TIMESTAMP NOT NULL,
    decided_at TIMESTAMP,
    decided_by TEXT,
    rejection_reason TEXT,
    job_id BIGINT
);
```

### Usage

```rust
use casparian_sentinel::ApiStorage;
use casparian_protocol::{HttpJobType, HttpJobStatus, EventType, ApprovalOperation};

// Open storage
let storage = ApiStorage::open("duckdb:casparian_flow.duckdb")?;
storage.init_schema()?;

// Create a job
let job_id = storage.create_job(
    HttpJobType::Run,
    "my_parser",
    Some("1.0.0"),
    "/data/input",
    Some("parquet://./output"),
    None,  // approval_id
)?;

// Update job status
storage.update_job_status(job_id, HttpJobStatus::Running)?;

// Emit events (monotonic ordering)
let event_id = storage.insert_event(job_id, &EventType::JobStarted)?;
assert_eq!(event_id, 1);

let event_id = storage.insert_event(job_id, &EventType::Progress {
    items_done: 50,
    items_total: Some(100),
    message: Some("Processing".to_string()),
})?;
assert_eq!(event_id, 2);

// Poll events (for long-polling)
let events = storage.list_events(job_id, Some(1))?; // After event_id 1
assert_eq!(events.len(), 1);  // Only event 2

// Create approval
let approval_id = ApiStorage::generate_approval_id();
storage.create_approval(
    &approval_id,
    &ApprovalOperation::Run {
        plugin_name: "my_parser".to_string(),
        plugin_version: None,
        input_dir: "/data".to_string(),
        file_count: 100,
        output: None,
    },
    "Run my_parser on 100 files",
    chrono::Duration::hours(1),
)?;

// Approve or reject
storage.approve(&approval_id, Some("user@example.com"))?;
// OR
storage.reject(&approval_id, Some("admin"), Some("Too risky"))?;
```

### Key Methods

| Method | Purpose |
|--------|---------|
| `init_schema()` | Create tables (idempotent) |
| `create_job()` | Create new job record |
| `get_job()` | Get job by ID |
| `list_jobs()` | List jobs with optional status filter |
| `update_job_status()` | Update job status (sets timestamps) |
| `update_job_progress()` | Update progress fields |
| `update_job_result()` | Set result (rows, outputs, metrics) |
| `cancel_job()` | Cancel job (if not terminal) |
| `next_event_id()` | Get next monotonic event ID for job |
| `insert_event()` | Insert event (auto-assigns ID) |
| `list_events()` | List events, optionally after an ID |
| `create_approval()` | Create approval request |
| `get_approval()` | Get approval by ID |
| `list_approvals()` | List approvals with optional status filter |
| `approve()` | Approve a pending request |
| `reject()` | Reject a pending request |
| `expire_approvals()` | Mark expired approvals |
| `link_approval_to_job()` | Link job to approval after creation |
| `cleanup_old_data()` | TTL enforcement for jobs/events |

---

## Event Ordering

Events use monotonic `event_id` per job:

```rust
/// Get the next event ID for a job (monotonically increasing per job).
pub fn next_event_id(&self, job_id: JobId) -> Result<EventId> {
    let sql = r#"
        SELECT COALESCE(MAX(event_id), 0) + 1
        FROM cf_api_events
        WHERE job_id = ?
    "#;
    // ...
}
```

**Guarantees:**
- Events are strictly ordered within a job
- Polling with `after_event_id` returns only new events
- No gaps in event IDs (1, 2, 3, ...)

---

## Approval Workflow

```
1. Tool requests write operation
   └─> create_approval() → status: pending

2. Human reviews via CLI
   └─> casparian mcp list
   └─> casparian mcp approve <id>

3. Approval decided
   └─> approve() or reject() → status: approved/rejected

4. If approved, job created
   └─> create_job() with approval_id
   └─> link_approval_to_job()

5. Job executes and emits events
```

---

## JobQueue (Legacy)

The `JobQueue` struct manages the legacy job queue (separate from API jobs).

Located in `src/db/queue.rs`:

```rust
use casparian_sentinel::JobQueue;

let queue = JobQueue::new(conn);
queue.init()?;

// Enqueue a job
let job_id = queue.enqueue(&JobDetails {
    plugin_name: "my_parser".to_string(),
    // ...
})?;

// Dequeue next job
let job = queue.dequeue()?;
```

---

## Sentinel Service

The `Sentinel` struct manages worker communication via ZMQ.

Located in `src/sentinel.rs`:

```rust
use casparian_sentinel::{Sentinel, SentinelConfig};

let config = SentinelConfig {
    bind_address: "tcp://127.0.0.1:5555".to_string(),
    // ...
};

let sentinel = Sentinel::new(config)?;
sentinel.run()?;  // Blocks, handling worker messages
```

---

## Testing

### Unit Tests

```bash
# All tests
cargo test -p casparian_sentinel

# ApiStorage tests only
cargo test -p casparian_sentinel -- api_storage
```

### Test Patterns

Tests use in-memory DuckDB:

```rust
fn setup_storage() -> ApiStorage {
    let conn = DbConnection::open_duckdb_memory().unwrap();
    let storage = ApiStorage::new(conn);
    storage.init_schema().unwrap();
    storage
}

#[test]
fn test_create_and_get_job() {
    let storage = setup_storage();
    let job_id = storage.create_job(...).unwrap();
    let job = storage.get_job(job_id).unwrap().unwrap();
    assert_eq!(job.plugin_name, "test_parser");
}
```

---

## Common Tasks

### Add a New Event Type

1. Add variant to `EventType` in `casparian_protocol/src/http_types.rs`:
   ```rust
   pub enum EventType {
       // ...
       MyNewEvent { field: String },
   }
   ```

2. Update `event_type_to_str()` in `api_storage.rs`:
   ```rust
   fn event_type_to_str(event_type: &EventType) -> &'static str {
       match event_type {
           // ...
           EventType::MyNewEvent { .. } => "my_new_event",
       }
   }
   ```

3. Update DDL in `init_schema()`:
   ```rust
   let event_type_values = "'job_started','phase','progress','violation','output','job_finished','approval_required','my_new_event'";
   ```

### Add a New Job Field

1. Add column to DDL in `init_schema()`
2. Update `create_job()` parameters
3. Update `row_to_job()` extraction
4. Add update method if needed (e.g., `update_job_my_field()`)

**Note:** Per pre-v1 rules, delete the database file when schema changes. No migrations.

---

## Key Principles

1. **Single source of truth** - All API state in DuckDB
2. **Monotonic events** - Strict ordering within each job
3. **No migrations** - Drop/recreate tables on schema change
4. **Direct crate calls** - MCP uses ApiStorage directly (no HTTP)
5. **Terminal states are final** - Completed/Failed/Cancelled jobs can't be modified
