Here is the complete, updated `ARCHITECTURE.md` incorporating the ZeroMQ process isolation architecture.

````markdown
# Casparian Flow Architecture Guide

A mental model for understanding the entire codebase.

---

## High-Level Overview

Casparian Flow is a **file-to-database pipeline** that watches directories, detects file changes, and transforms them into structured data.

```mermaid
graph LR
    A[File System] -->|Scout| B[Job Queue]
    B -->|ZmqWorker| C[Sidecar Process]
    C -->|Plugin| D[Output DB/Parquet]
````

**Core principle**: Files flow through the system immutably. Every file version is tracked, every job is auditable.

-----

## Directory Structure

```
src/casparian_flow/
├── config.py          # Pydantic settings from TOML
├── context.py         # EtlContext (git hash, env info)
├── sdk.py             # BasePlugin class for user plugins
├── interface.py       # CaspPlugin protocol definition
├── main.py            # Entry point
├── protocol.py        # Binary wire protocol definitions (OpCodes, Headers) [NEW]
├── sidecar.py         # Isolated plugin execution process [NEW]
├── db/
│   ├── models.py      # SQLAlchemy data models (THE HEART)
│   ├── base_session.py# SessionLocal factory
│   ├── access.py      # Engine creation, bulk upload
│   └── setup.py       # DB initialization, schema fingerprinting
├── engine/
│   ├── worker.py      # Legacy In-Process Job processor
│   ├── zmq_worker.py  # ZMQ-based Router Worker [NEW]
│   ├── queue.py       # JobQueue (pop/complete/fail)
│   ├── context.py     # WorkerContext (lineage injection)
│   ├── sinks.py       # Parquet/SQLite/MSSQL outputs
│   ├── heartbeat.py   # Worker node heartbeat
│   └── config.py      # WorkerConfig dataclass
├── services/
│   ├── scout.py       # File scanner (CORE LOGIC)
│   └── fs_engine.py   # Parallel file walker
├── engine/
│   ├── loader.py      # PluginRegistry discovery (Moved from plugins/)
...
└── builtins/          # Compiled-in System Plugins
    └── system_deployer.py 
plugins/               # User-generated Runtime Plugins
    └── generated_plugin.py
```

-----

## Process Architecture (ZMQ Isolation & Data-Oriented Design)

To ensure system stability and high throughput, Casparian Flow uses a **Split Architecture** that decouples the engine from user code, optimized with **Data-Oriented Design** principles.

### The Topology

```mermaid
sequenceDiagram
    participant Queue as Database Queue
    participant Router as ZmqWorker (Router)
    participant Sidecar as Sidecar (Dealer)
    
    Sidecar->>Router: REG (I can handle 'csv_parser')
    Router->>Queue: Pop Job
    Queue-->>Router: Job #123 (Plugin: 'csv_parser')
    Router->>Sidecar: EXEC (Job #123, File: /data/file.csv)
    Sidecar->>Router: DATA (Arrow IPC Stream)
    Note over Router: ZERO-COPY ROUTING
    Router->>Sink: write(pa.Table)
    Sidecar->>Router: DONE
    Router->>Queue: Complete Job #123
```

### Components

1.  **ZmqWorker (The Router)**

      * Binds to a TCP or IPC socket.
      * **Routing Table**: Caches plugin configurations (`TopicConfig`) in memory to avoid DB lookups in the hot path.
      * **Zero-Copy Routing**: Forwards Arrow IPC buffers from Sidecars directly to Sinks without unpacking/deserializing them into Pandas.
      * **Mechanism over Policy**: separating the plumbing (moving bytes) from policy (DB config).

2.  **Sidecar (The Dealer)**

      * A separate OS process (Python interpreter).
      * Loads **one** specific plugin and runs it in a loop.
      * **Isolation**: If this process dies, the Worker detects the disconnect and marks the job as FAILED.

3.  **The Protocol (v2)**

      * Defined in `src/casparian_flow/protocol.py`.
      * **Header**: Fixed 16-byte binary header `!BBHIQ` (Version, OpCode, Flags, MetaLength, JobID).
      * **Zero-Copy Payload**: Uses Apache Arrow IPC streams.
      * **OpCodes**:
          * `REG (1)`: Registration handshake.
          * `EXEC (2)`: Command to process a file.
          * `DATA (3)`: Stream of Arrow batches.
          * `ERR (4)`: Exception info.
          * `DONE (5)`: Job success signal.
          * `HEARTBEAT (6)`: Keep-alive ping.
          * `DEPLOY (7)`: Hot deployment payload.

-----

## The Data Models (The Heart)

All models live in `src/casparian_flow/db/models.py`. Understanding these is critical.

### Entity Relationship

```mermaid
erDiagram
    SourceRoot ||--o{ FileLocation : contains
    FileLocation ||--o{ FileVersion : has_versions
    FileVersion }o--|| FileHashRegistry : references
    FileVersion ||--o{ ProcessingJob : triggers
    ProcessingJob }o--|| PluginConfig : uses
    PluginConfig ||--o{ TopicConfig : has_topics
    RoutingRule ||--o{ FileVersion : tags
```

### Key Models

| Model | Purpose | Primary Key |
|-------|---------|-------------|
| `SourceRoot` | A watched directory (e.g., `/data/invoices`) | `id` |
| `FileLocation` | A persistent path within a SourceRoot | `id` |
| `FileVersion` | Immutable snapshot of file content | `id` |
| `FileHashRegistry` | Deduplication registry (content-addressed) | `content_hash` |
| `ProcessingJob` | A unit of work in the queue | `id` |
| `PluginConfig` | Plugin settings + subscribed tags | `plugin_name` |
| `TopicConfig` | Output sink configuration per topic | `id` |
| `RoutingRule` | Pattern → Tag mapping (glob-based) | `id` |
| `WorkerNode` | Active worker heartbeat registry | `hostname + pid` |

### The Versioning Pattern

```
FileLocation (persistent path)
    └── FileVersion (immutable snapshot)
            └── content_hash → FileHashRegistry

When file changes:
1. Scout detects new hash
2. Creates NEW FileVersion (old one preserved)
3. Updates FileLocation.current_version_id
```

**Why?** Audit trail. Every job permanently references the exact file content it processed.

-----

## Services Layer

The `src/casparian_flow/services/` directory contains the core logic that operates outside the ZMQ Worker's hot loop.

### 1. The Scout (`scout.py`)
Responsible for **File Discovery and Ingestion**.
- Uses `fs_engine.py` for parallel directory walking.
- Detects file changes (SHA-256).
- Manages `FileHashRegistry` and `FileVersion` immutability.
- **Output**: Pushes `ProcessingJob`s to the Database Queue.

### 2. The Architect (`architect.py`) [NEW in v2]
Responsible for **Plugin Governance and Deployment**.
- Handles `DEPLOY` OpCodes.
- Verifies HMAC signatures of new plugin code.
- Persists valid code to `PluginManifest` table.
- Orchestrates the "Hot Reload" of sidecars.

-----

## Contexts: The Two Truths

The system maintains two distinct types of context, often confused but serving different scopes.

| Context | Class | File | Scope | Purpose |
| :--- | :--- | :--- | :--- | :--- |
| **Environmental** | `EtlContext` | `src/casparian_flow/context.py` | Process | Captures Git Hash, Hostname, User. Used by `Heartbeat` to identify the worker node's version/location. |
| **Execution** | `WorkerContext` | `src/casparian_flow/engine/context.py` | Job | Captures JobID, FileVersionID. Injects lineage into data streams. Manages Sinks. |

-----

## The Queue (Job Management)

`src/casparian_flow/engine/queue.py` provides atomic job claims.

### SQLite Mode (Dev)

Simple query + update (no locking).


### MSSQL Mode (Production)

Atomic CTE with row locks:

```sql
WITH cte AS (
    SELECT TOP(1) * FROM queue 
    WITH (ROWLOCK, READPAST, UPDLOCK)
    WHERE status = 'QUEUED'
    ORDER BY priority DESC, id ASC
)
UPDATE cte SET status = 'RUNNING' ...
OUTPUT inserted.*;
```

-----

## The Sinks (Output Writers)

`src/casparian_flow/engine/sinks.py` handles writing processed data.

### Supported Sinks

| URI Scheme | Destination | Example |
|------------|-------------|---------|
| `parquet://` | Parquet files | `parquet://./output` |
| `sqlite://` | SQLite table | `sqlite://data.db/table` |
| `mssql://` | SQL Server | `mssql://schema.table` |

### Staging Pattern (Blue/Green)

All sinks use staging for atomicity:

```
1. write() → staging table/file
2. promote() → move to production
3. On error: staging is discarded
```

-----

## The Plugin System

### BasePlugin (`src/casparian_flow/sdk.py`)

```python
class BasePlugin:
    def configure(self, ctx, config):
        """System injects dependencies."""
        self._ctx = ctx
        
    def publish(self, topic: str, data: DataFrame):
        """User calls this to output data."""
        self._ctx.register_topic(topic)
        self._ctx.publish(handle, data)  # Lineage injected here
```

### Plugin Discovery (`src/casparian_flow/engine/loader.py`)

1.  Scans `plugins/` directory for `.py` files
2.  Dynamically imports each module
3.  Looks for `Handler` class
4.  Caches in registry

### User Plugin Example

```python
from casparian_flow.sdk import BasePlugin
import pandas as pd

class Handler(BasePlugin):
    def execute(self, file_path: str):
        df = pd.read_csv(file_path)
        self.publish('output', df)
```

-----

## Configuration System

### Global Config (`src/casparian_flow/config.py`)

Uses Pydantic Settings with TOML:

```toml
# global_config.toml
[database]
type = "sqlite3"
db_location = "casparian_flow.sqlite3"
```

### Worker Config (`src/casparian_flow/engine/config.py`)

```python
@dataclass
class WorkerConfig:
    database: DatabaseConfig
    storage: StorageConfig  
    plugins: PluginsConfig
```

-----

## Integrity & Security

### Schema Fingerprinting (`src/casparian_flow/db/setup.py`)

The system uses **database-based schema fingerprinting** (not file hashing):

1.  Query live database structure via SQLAlchemy inspector
2.  Build canonical representation: `table:column:TYPE`
3.  SHA-256 hash of the canonical string

<!-- end list -->

```python
def compute_schema_fingerprint(engine) -> str:
    """Returns stable hash of database structure."""
    # Immune to whitespace/comment changes in code
```

### Database Integrity

Inspector validates all expected tables and columns exist.

-----

## Data Flow Summary

```
┌─────────────────────────────────────────────────────────────────────┐
│                           SCOUT PHASE                               │
├─────────────────────────────────────────────────────────────────────┤
│  1. ParallelFileScanner walks SourceRoot directories               │
│  2. For each file: compute SHA-256 hash                            │
│  3. Check FileHashRegistry for deduplication                       │
│  4. Upsert FileLocation (path) and FileVersion (content)           │
│  5. Apply RoutingRules → assign tags                               │
│  6. Match tags to PluginConfig subscriptions                       │
│  7. Create ProcessingJob with QoS priority                         │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          WORKER PHASE                               │
├─────────────────────────────────────────────────────────────────────┤
│  1. JobQueue.pop_job() atomically claims a QUEUED job              │
│  2. Resolve file path: FileVersion → FileLocation → SourceRoot     │
│  3. ZmqWorker dispatches job via ZMQ (EXEC) to Sidecar             │
│  4. Sidecar processes file, streams DATA (Arrow) back to Worker    │
│  5. Worker writes to Sink (Staging) with injected lineage          │
│  6. On DONE signal: Sink.promote() moves staging → production      │
│  7. JobQueue.complete_job() marks job COMPLETED                    │
└─────────────────────────────────────────────────────────────────────┘
```

-----

## Testing Strategy

| Test File | Focus |
|-----------|-------|
| `test_smoke.py` | End-to-end: Scout → Queue → Worker → Sink |
| `test_queue.py` | Priority ordering, atomic claims, completion |
| `test_zmq_integration.py` | Protocol encoding, message flow |
| `test_zmq_e2e.py` | Full multi-process system test |
| `test_worker.py` | Error handling, job failure marking |

Key fixtures in `conftest.py`:

  - `temp_test_dir`: Isolated test data directory
  - `test_db_engine`: Fresh SQLite per test
  - `test_source_root`: Pre-configured SourceRoot ID

<!-- end list -->

```
```