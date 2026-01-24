# Casparian Flow Architecture

## Product Intent

**Casparian Flow** is a **local-first ingestion and governance runtime** that turns messy file corpuses into **typed, queryable tables** with:
- **Incremental ingestion** (don't redo work; safe reruns)
- **Lineage** (row → file → job → parser version → contract)
- **Quarantine** (bad rows are preserved, not dropped)
- **Schema contracts + approvals** (auditability and controlled evolution)

### Core Promise
> If you can point Casparian at a directory of files and a parser, you can reliably produce tables you can trust—and you can prove how you got them.

### Target Customers

**Primary:** DFIR / Incident Response
- Air-gapped / sensitive environments
- Inputs: EVTX, registry hives, browser artifacts, logs, forensics bundles
- Pain: extracting structured evidence repeatably; proving provenance

**Broader:**
- Security / compliance analytics teams
- Data teams ingesting semi-structured files
- Regulated industries (redaction, reproducibility)

---

## Target Architecture

### Component Decomposition

```
┌─────────────────────────────────────────────────────────────────┐
│                    FRONTENDS (Clients)                          │
│              CLI / TUI / Tauri UI / MCP Server                  │
│         • Mutations via Control API (IPC/RPC)                   │
│         • Read-only DB for queries                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    CONTROL PLANE (Sentinel)                     │
│         • Single mutation authority for control-plane state     │
│         • Jobs, approvals, sessions, output catalog             │
│         • Materializations, schema contract registry            │
│         • Exposes Control API for all mutations                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    EXECUTION PLANE (Worker)                     │
│         • Stateless executor                                    │
│         • True cancellation (kill subprocess, prevent commit)   │
│         • Stage → promote atomic writes                         │
│         • Emits receipts with stable identities                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    PERSISTENCE                                  │
│         • Control-plane DB (DuckDB): jobs, catalog, contracts   │
│         • Output stores: DuckDB, Parquet, CSV sinks             │
└─────────────────────────────────────────────────────────────────┘
```

### Control Plane (Sentinel)
Single mutation authority for:
- Job queue + job state machine
- Approvals
- Sessions
- Output catalog + materializations
- Schema contract registry (deploy + promote)

Exposes **Control API** (IPC/RPC) for all mutations:
- enqueue/cancel/retry jobs
- approve/reject
- session create/advance

**Rationale:** Prevent split-brain and exclusive-lock dead ends. Enable multi-client UX without distributed systems complexity.

### Execution Plane (Worker)
Stateless executor:
- Accepts dispatch commands
- Runs parser plugin (Python shim + native)
- Validates schema
- Writes outputs via sinks
- Emits receipts with stable IDs (including `source_hash`)

Supports **true cancellation**:
- Abort kills subprocess
- Prevents commit (or records partial side effects explicitly)

### Frontends
CLI/TUI/Tauri/MCP are **clients** of the Control API.
Query console uses **read-only** DB connections.

---

## Domain Entities

| Entity | Identity | Description |
|--------|----------|-------------|
| **InputFile** | `source_hash` (blake3) + optional path hash | A file to be processed |
| **ParserArtifact** | `artifact_hash` + `env_hash` + parser version | Deployed parser with environment |
| **Job** | job_id | State: Queued → Dispatched → Running → {Completed \| Failed \| Aborted \| Rejected} |
| **OutputTarget** | `output_target_key` | sink URI hash + table name + schema hash + sink_mode |
| **Materialization** | `materialization_key` | output_target + source_hash + parser artifact |
| **Contract** | contract_id | Schema constraints for outputs; approval gating |

### Identity Functions

```rust
// crates/casparian_protocol/src/idempotency.rs
output_target_key(sink_uri, table_name, schema_hash, sink_mode) -> String
materialization_key(output_target_key, source_hash, artifact_hash) -> String
```

---

## Core Invariants ("Bad States Impossible")

These invariants MUST hold. Violations are bugs.

| # | Invariant | Description |
|---|-----------|-------------|
| 1 | **No output collisions** | File sink artifact names are globally unique |
| 2 | **Atomic commits** | Staged output is promoted only on success |
| 3 | **Cancel means stop** | Aborted job cannot commit outputs |
| 4 | **SinkMode enforced** | Replace/Error/Append semantics consistent with idempotency keys |
| 5 | **Lineage deterministic** | Reserved `_cf_*` namespace cannot silently break lineage injection |
| 6 | **Incremental decisions deterministic** | Default sink configs do not cause "silent skip" when changed |
| 7 | **UI truthful** | Cancel button and job statuses reflect reality |

---

## Core User Flows

### Flow A: "Turn a directory into tables" (critical path)
1. Choose a source (directory or bundle)
2. Scan / discover (build catalog; optional)
3. Choose parser(s) and define selection rules
4. Run ingestion
5. Inspect results:
   - Outputs produced (tables/files)
   - Quarantine summary and examples
   - Lineage details
6. Iterate: adjust rules/contracts/parser; rerun incrementally

**Success definition:** Outputs are committed atomically and recorded as materializations. Quarantine is visible and actionable.

### Flow B: Incremental ingestion / backfill
1. Update selection window (since watermark, tags)
2. Update sink destination or output topics
3. Rerun
4. System enqueues exactly what's needed (conservative, deterministic)

---

## Data Flow: Complete Pipeline

```
1. DISCOVERY
   casparian scan /data → file catalog with hashes

2. SELECTION
   casparian pipeline select --tag evtx → file set

3. ENQUEUE
   Check materializations → skip already-processed
   Enqueue jobs for remaining files

4. DISPATCH (Sentinel)
   Assign job to worker
   Record job state transitions

5. EXECUTE (Worker)
   Run parser in isolated subprocess
   Validate schema, split valid/quarantine
   Inject lineage columns
   Write to sinks (staged)

6. COMMIT (Worker → Sentinel)
   Promote staged outputs
   Record materializations
   Update job status

7. QUERY
   SQL on DuckDB outputs
```

---

## Protocol (OpCodes)

Binary header: `!BBHQI` (16 bytes)

| OpCode | Name | Direction | Purpose |
|--------|------|-----------|---------|
| 1 | IDENTIFY | Worker → Sentinel | Handshake |
| 2 | DISPATCH | Sentinel → Worker | Job command |
| 3 | ABORT | Sentinel → Worker | Cancel |
| 4 | HEARTBEAT | Worker → Sentinel | Status |
| 5 | CONCLUDE | Worker → Sentinel | Job done |
| 6 | ERR | Both | Error |
| 8 | PREPARE_ENV | Sentinel → Worker | Setup venv |
| 9 | ENV_READY | Worker → Sentinel | Venv ready |
| 10 | DEPLOY | Publisher → Sentinel | Deploy artifact |

---

## Crate Map

| Crate | Purpose |
|-------|---------|
| `casparian` | Unified CLI binary, TUI |
| `casparian_sentinel` | Control plane: job queue, dispatch, Control API |
| `casparian_worker` | Execution plane: parser execution, schema validation |
| `casparian_sinks` | Output persistence (DuckDB, Parquet, CSV) + lineage |
| `casparian_protocol` | Binary protocol, types, idempotency keys |
| `casparian_scout` | File discovery, tagging |
| `casparian_db` | Database abstraction, locking |
| `casparian_tape` | Event recording for replay/debugging |
| `casparian_backtest` | Multi-file validation, fail-fast |

---

## Database Architecture

### Single Database
Everything uses: `~/.casparian_flow/casparian_flow.duckdb`

### Table Prefixes

| Prefix | Purpose |
|--------|---------|
| `cf_processing_queue` | Canonical job queue (execution) |
| `cf_materializations` | Incremental ingestion tracking |
| `cf_expected_outputs` | Expected outputs per parser/topic |
| `scout_*` | File discovery, tagging rules |
| `schema_*` | Schema contracts, amendments |

### Access Pattern
- **Sentinel**: Exclusive write access via Control API
- **UI/CLI mutations**: Via Control API (IPC)
- **Queries**: Read-only DB connections allowed

---

## Security Architecture

### Bridge Mode Execution
```
Worker (Host)              Guest Process
     │                          │
     │ AF_UNIX Socket           │
     │ ─────────────────────    │
     │                          │
     │ Credentials, secrets     │ Plugin code only
     │ Heavy drivers            │ pandas, pyarrow
     │ Sink writers             │ No credentials
```

### Trust Model
- **Python plugins**: `allow_unsigned_python` config (default: false; explicit opt-in)
- **Native plugins**: Signature verification
- **Path traversal**: `validate_entrypoint()` blocks `..` and absolute paths

---

## Key Decisions (ADRs)

| ADR | Decision | Consequence |
|-----|----------|-------------|
| 001 | Parser is top-level entity | Direct file → parser → output flow |
| 002 | Tags, not routes | Scout discovers+tags; Sentinel processes |
| 003 | Constraint-based type inference | Elimination, not voting |
| 004 | Schema as contract | Hard failures on violation |
| 005 | Fail-fast backtest | Test high-failure files first |
| 006 | MCP-first integration | AI-assisted workflow with human approval gates |
| 007 | CLI-first architecture | CLI + TUI primary; Tauri optional |
| 008 | Parser as tuple yielder | `(sink_name, data)` tuples |
| 009 | Content-based parser identity | blake3(content), not path |
| 010 | Partitioned output by job | `{output}_{job_id}.parquet` |
| 016 | Split runtime architecture | Control plane + execution plane separate |

---

## Evidence Index

### Protocol / Identity
- `crates/casparian_protocol/src/lib.rs::OpCode`
- `crates/casparian_protocol/src/types.rs::DispatchCommand`
- `crates/casparian_protocol/src/types.rs::JobReceipt`
- `crates/casparian_protocol/src/idempotency.rs::output_target_key`
- `crates/casparian_protocol/src/idempotency.rs::materialization_key`

### Control Plane
- `crates/casparian_sentinel/src/sentinel.rs::dispatch_loop`
- `crates/casparian_sentinel/src/sentinel.rs::record_materializations_for_job`
- `crates/casparian_sentinel/src/control.rs` - Control API types
- `crates/casparian_sentinel/src/db/queue.rs::Job`

### Execution Plane
- `crates/casparian_worker/src/worker.rs::execute_job_inner`
- `crates/casparian_worker/src/worker.rs::compute_source_hash`
- `crates/casparian_worker/src/worker.rs::validate_lineage_columns`
- `crates/casparian_worker/src/cancel.rs::CancellationToken`

### Sinks
- `crates/casparian_sinks/src/lib.rs::output_filename`
- `crates/casparian_sinks/src/lib.rs::inject_lineage_columns`
- `crates/casparian_sinks/src/lib.rs::DuckDbSink::write_batch`

### UI
- `tauri-ui/src-tauri/src/commands/jobs.rs`
- `tauri-ui/src-tauri/src/state.rs::try_control_client`
