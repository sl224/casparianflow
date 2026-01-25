# Claude Code Instructions for Casparian Flow

## Quick Orientation

**What is Casparian Flow?** A **local-first ingestion and governance runtime** that turns messy file corpuses into **typed, queryable tables** with incremental ingestion, per-row lineage, quarantine semantics, and schema contracts.

**Core Promise:** If you can point Casparian at a directory of files and a parser, you can reliably produce tables you can trust—and you can prove how you got them.

**Primary Users:** DFIR / Incident Response teams (air-gapped, evidence servers, chain-of-custody requirements).

**Start Here:**
1. This file → architecture + invariants
2. `docs/agent/AGENTS_CHECKLIST.md` → **coding standards and testing**
3. `ARCHITECTURE.md` → detailed system design
4. `docs/index.md` → documentation index (canonical vs plan vs archived)
5. Crate-specific `CLAUDE.md` files → component details

---

## Pre-v1 Development Rules

Until v1 ships: NO production users, NO data to preserve. Do NOT implement:
- Database migrations (just delete `~/.casparian_flow/casparian_flow.duckdb`)
- Backwards compatibility / API versioning / gradual rollouts
- Data preservation during refactors

**Instead:** Change schemas directly, update all call sites, break fast and fix fast.

---

## Engineering Ethos

Follow "make illegal states unrepresentable" (Jon Blow, Casey Muratori, John Carmack style). Prefer compile-time guarantees over runtime patching.

### Core Principles

| Principle | Meaning |
|-----------|---------|
| **Parse, don't validate** | Convert unstructured → structured at boundaries |
| **Data dominates** | Right data structures first, algorithms follow |
| **State is liability** | Minimize state, derive what you can compute |
| **Boundaries do heavy lifting** | Defensive code at edges; core trusts inputs |
| **Boring code > clever code** | Junior-readable in 30 seconds |
| **Fail loud, not silent** | Errors impossible to ignore |
| **Delete unused code** | Dead code misleads and hides bugs |

### Anti-Patterns (Reference by Name)

| Pattern | Problem | Fix |
|---------|---------|-----|
| **Silent Corruption** | `.unwrap_or_default()` hides bad DB data | Use `?` with typed error |
| **Stringly Typed** | `match status.as_str()` misses typos | Use enum matching |
| **Shotgun Validation** | Same check in 10 places | Parse once, use validated type |
| **Zombie Object** | Struct needs `.init()` after `new()` | Valid from construction |
| **Primitive Obsession** | `fn f(file_id: i64, job_id: i64)` swappable | Use newtypes |
| **Dual Source of Truth** | Rust enum vs SQL CHECK diverge | Single authoritative source |
| **Boolean Blindness** | `editing: bool, creating: bool` both true | Use enum for exclusive states |
| **Lossy Cast** | `x as i32` silently truncates | Use `try_from` |
| **Magic String Literals** | `"PENDING"` in 40 files | Centralized constants |

### Pre-Commit Checklist

- [ ] No `.unwrap_or_default()` on parsed enums
- [ ] Status/state checks use enums, not strings
- [ ] Unstructured data converted to types at boundaries
- [ ] No duplicated constants between Rust and SQL
- [ ] Structs valid from construction
- [ ] No `as i32` on potentially large values
- [ ] Multiple bools aren't encoding exclusive states

---

## Target Architecture

The system is decomposed into four planes with clear responsibilities:

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
│         • Job queue + state machine                             │
│         • Approvals, sessions, output catalog                   │
│         • Schema contract registry                              │
│         • Exposes local Control API for all mutations           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    EXECUTION PLANE (Worker)                     │
│         • Stateless executor                                    │
│         • Runs parser plugins (Python/native)                   │
│         • Validates schema, quarantines invalid rows            │
│         • Writes outputs via sinks                              │
│         • Emits receipts with stable identities                 │
│         • True cancellation (kill subprocess, prevent commit)   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    PERSISTENCE                                  │
│         • Control-plane DB (DuckDB): jobs, catalog, contracts   │
│         • Output stores: DuckDB sink, Parquet, CSV              │
│         • Materializations for incremental ingestion            │
└─────────────────────────────────────────────────────────────────┘
```

### Crate Architecture

| Crate | Purpose |
|-------|---------|
| `casparian` | Unified CLI binary (includes `scout` module for file discovery) |
| `casparian_sentinel` | Control plane: job queue, dispatch, materializations |
| `casparian_worker` | Execution plane: parser execution, schema validation |
| `casparian_sinks` | Output persistence abstractions + lineage injection |
| `casparian_sinks_duckdb` | DuckDB-specific sink implementation |
| `casparian_protocol` | Binary protocol, serialization, idempotency keys |
| `casparian_db` | Database abstraction (DuckDB via `DbConnection`) |
| `casparian_tape` | Event recording for replay/debugging |
| `casparian_schema` | Schema contract storage and validation |
| `casparian_ids` | Strongly-typed ID definitions |
| `casparian_security` | Trust config, signing, gatekeeper |
| `casparian_mcp` | Model Context Protocol integration |
| `casparian_profiler` | Performance profiling utilities |
| `casparian_backtest` | Multi-file validation, fail-fast testing |
| `casparian_intent` | Intent handling for AI workflows |

**Note:** Scout is a module within `casparian` at `crates/casparian/src/scout/`, not a separate crate.

---

## Core Concepts

### 1. Schema = Intent, then Contract
```
Discovery → User Reviews → APPROVAL → Contract (Immutable)
```
Before approval: proposal. After: CONTRACT with hard failures on violation.

### 2. Constraint-Based Type Inference
Elimination, not voting: "31/05/24" PROVES DD/MM/YY because 31 > 12.

### 3. Fail-Fast Backtest
Test high-failure files first → early stop if they still fail.

### 4. Tags, Not Routes
Scout: `pattern → TAG`. Sentinel: `TAG → plugin → execute → sink`.

### 5. Bridge Mode Execution
Plugins run in isolated subprocesses. Host holds secrets; guest is sandboxed.
- **Transport:** TCP loopback (`127.0.0.1:0`), port passed via `BRIDGE_PORT` env var
- **Wire format:** Arrow IPC with framing headers
- **Code reference:** `crates/casparian_worker/src/bridge.rs`

### 6. Parser Execution

Parsers are Python modules with a `parse(file_path)` function:

```python
# Single-output parser (most common)
TOPIC = "transactions"  # Optional, defaults to "default"

def parse(file_path: str):
    """Returns pandas DataFrame, polars DataFrame, or pyarrow Table."""
    import pandas as pd
    df = pd.read_csv(file_path)
    return df  # Host wraps with TOPIC name
```

```python
# Multi-output parser
from casparian_types import Output

TOPIC = "orders"

def parse(file_path: str):
    """Returns list[Output] for multiple output tables."""
    return [
        Output("events", events_df),
        Output("metrics", metrics_df, table="metrics_override"),
    ]
```

**Bridge transport:** TCP loopback (`127.0.0.1:0`), not AF_UNIX. Port passed via `BRIDGE_PORT` env var.

**Key features:** Parser versioning, deduplication by (input_hash, parser_name, version), lineage columns (`_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version`), atomic writes.

**Code reference:** `crates/casparian_worker/shim/bridge_shim.py`, `crates/casparian_worker/shim/casparian_types.py`

---

## Core Invariants ("Bad States Impossible")

These invariants MUST hold. Violations are bugs to fix immediately.

| Invariant | Description | Evidence |
|-----------|-------------|----------|
| **No output collisions** | File sink artifact names are globally unique | `casparian_sinks/src/lib.rs::output_filename` |
| **Atomic commits** | Staged output is promoted only on success | Staging directory + rename |
| **Cancel means stop** | Aborted job cannot commit outputs | `CancellationToken` in worker |
| **SinkMode enforced** | Replace/Error/Append semantics are consistent | `casparian_protocol/src/types.rs::SinkMode` |
| **Lineage deterministic** | Reserved `_cf_*` namespace cannot break lineage | `validate_lineage_columns()` |
| **Incremental decisions deterministic** | Default sink configs do not cause "silent skip" | ExpectedOutputs expansion |
| **UI truthful** | Cancel button and job statuses reflect reality | Control API integration |

---

## Domain Entities

| Entity | Identity | Description |
|--------|----------|-------------|
| **InputFile** | `source_hash` (blake3) + optional path hash | A file to be processed |
| **ParserArtifact** | `artifact_hash` + `env_hash` + parser version | A deployed parser with its environment |
| **Job** | job_id | State machine: Queued → Dispatched → Running → {Completed \| Failed \| Aborted} |
| **OutputTarget** | `output_target_key` (sink URI hash, table name, schema hash, sink_mode) | Where outputs go |
| **Materialization** | `materialization_key` (output_target + source_hash + parser artifact) | Record that an input was processed to an output |
| **Contract** | contract_id | Schema constraints for outputs; approval gating |

### Key Identity Functions

```rust
// crates/casparian_protocol/src/idempotency.rs
output_target_key(sink_uri, table_name, schema_hash, sink_mode) -> String
materialization_key(output_target_key, source_hash, artifact_hash) -> String
```

---

## Database Architecture

### Single Database Rule
Everything uses: `~/.casparian_flow/casparian_flow.duckdb`

### Table Prefixes
| Prefix | Purpose | Code Reference |
|--------|---------|----------------|
| `cf_processing_queue` | Job queue entries | `crates/casparian_sentinel/src/db/queue.rs` |
| `cf_output_materializations` | Incremental ingestion tracking | `crates/casparian_sentinel/src/db/queue.rs` |
| `cf_plugin_manifest` | Parser registry with versions | `crates/casparian_sentinel/src/db/queue.rs` |
| `cf_topic_config` | Topic→Parser→Sink routing | `crates/casparian_sentinel/src/db/queue.rs` |
| `cf_api_*` | MCP job/event/approval storage | `crates/casparian_sentinel/src/db/api_storage.rs` |
| `scout_*` | File discovery, tagging rules | `crates/casparian/src/scout/db.rs` |
| `parser_lab_*` | Parser validation | `crates/casparian/src/scout/db.rs` |

### Database Abstraction
- **Database:** DuckDB only (no SQLite, no sqlx)
- **Connection type:** `casparian_db::DbConnection` wraps `duckdb::Connection`
- **Access modes:** Read-write (exclusive lock via `fs2`) or read-only (shared)
- **NEVER** hardcode `anthropic::*` → use `LlmProvider` trait
- **OK** to use concrete types in: CLI entry points, tests

**Code reference:** `crates/casparian_db/src/backend.rs`

---

## Development Workflow

```bash
# After any change
cargo check && cargo build --release && cargo test

# Key commands
casparian run parser.py input.csv [--sink parquet://./] [--force]
casparian backfill my_parser [--execute] [--limit 10]
casparian tui
casparian scan <dir> --tag my_topic
casparian files --tag my_topic
casparian jobs --status pending
```

### Code Quality Requirements

| Requirement | Details |
|-------------|---------|
| Zero warnings | `cargo check` + `cargo clippy` clean |
| Use DuckDB | Via `casparian_db::DbConnection` (no SQLite, no sqlx) |
| No unwrap in lib | Use `?` or `expect()` with context |
| Channels over locks | `tokio::sync::mpsc` or `std::sync::mpsc` |

---

## Documentation Structure

| Master Doc | Sub-Docs | Focus |
|------------|----------|-------|
| `spec.md` | `specs/*.md` | What to build |
| `STRATEGY.md` | `strategies/*.md` | How to win |

**When to create subspec:** TUI mode with complex state, >50 lines of spec, multiple impl phases.

---

## TUI Development

**Use TMux for ALL TUI work.** Scripts in `scripts/`:
- `tui-debug.sh start|stop|restart` - Session management
- `tui-send.sh "key"` - Send keystrokes
- `tui-capture.sh` - Capture screen
- `tui-test.sh all` - Run test scenarios

**Workflow:** Reproduce → Capture after EACH keystroke → Compare to spec → Fix → Verify in fresh session.

---

## Workflow Manager

Meta-workflows for specs, code quality, data models. See `specs/meta/workflow_manager.md`.

| Workflow | Trigger |
|----------|---------|
| `feature_workflow` | "add", "implement", "fix" |
| `spec_refinement_workflow` | "refine spec", "gaps in spec" |
| `spec_maintenance_workflow` | "audit specs" |
| `memory_audit_workflow` | "memory audit", "allocation" |
| `data_model_maintenance_workflow` | "dead types", "type cleanup" |
| `abstraction_audit_workflow` | "abstraction", "db coupling" |
| `tui_testing_workflow` | "test TUI" |

---

## Architecture Decision Records

| ADR | Decision | Consequence |
|-----|----------|-------------|
| 001 | Parser is top-level entity | Direct file → parser → output flow |
| 002 | Tags, not routes | Scout discovers+tags; Sentinel processes |
| 003 | Constraint-based type inference | Elimination, not voting |
| 004 | Schema as contract | Hard failures on violation |
| 005 | Fail-fast backtest | Test high-failure files first |
| 006 | MCP-first integration (v1) | AI-assisted workflow with human approval gates |
| 007 | CLI-first architecture | No Tauri; CLI + TUI only |
| 008 | Parser as tuple yielder | `(sink_name, data)` tuples |
| 009 | Content-based parser identity | blake3(content), not path |
| 010 | Partitioned output by job | `{output}_{job_id}.parquet` |
| 011 | CLI sink override | `--sink` overrides parser defaults |
| 012 | Parser versioning | `name`, `version`, `topics` required; dedup by (hash, name, ver) |
| 013 | Topic subscriptions | Files → Tags → Topics → Parsers chain |
| 014 | Structured error codes | Python bridge emits `error_code` field |
| 015 | Dual parser patterns | `transform(df)` for test; `parse(path)` for run |
| 016 | Split runtime architecture | Control plane + data plane separate |
| 021 | AI agentic iteration (v1) | Ephemeral contracts for iteration; AST for publish |

**ADR-006 details:** See `docs/v1_scope.md` MCP section, `docs/execution_plan_mcp.md`.
**ADR-021 details:** See `docs/decisions/ADR-021-ai-agentic-iteration-workflow.md`.

---

## Glossary

| Term | Definition |
|------|------------|
| **Scout** | File discovery + tagging |
| **Sentinel** | Control plane: job orchestration, single mutation authority |
| **Worker** | Execution plane: stateless parser executor |
| **Control API** | IPC/RPC interface for mutations (CLI/UI → Sentinel) |
| **Schema Contract** | Approved schema parser must conform to |
| **High-Failure File** | Historically failed during backtest |
| **Bridge Mode** | Host/guest isolation model |
| **Lineage Columns** | `_cf_source_hash`, `_cf_job_id`, `_cf_processed_at`, `_cf_parser_version` |
| **Deduplication** | Skip if (input_hash, parser_name, version) seen |
| **Backfill** | Re-process files when parser version changes |
| **Materialization** | Record that (input, parser, output_target) was processed |
| **OutputTarget** | A specific sink + table + schema combination |
| **EphemeralSchemaContract** | Temporary contract for AI iteration |
| **ViolationContext** | Machine-readable error context for AI learning |

---

## Evidence Index (Key Code Locations)

### Protocol / Identity
- `crates/casparian_protocol/src/lib.rs::OpCode` - Semantic protocol boundary
- `crates/casparian_protocol/src/types.rs::DispatchCommand` - Job dispatch
- `crates/casparian_protocol/src/types.rs::JobReceipt` - Job completion
- `crates/casparian_protocol/src/idempotency.rs::output_target_key` - Output identity
- `crates/casparian_protocol/src/idempotency.rs::materialization_key` - Incremental ingestion key

### Control Plane
- `crates/casparian_sentinel/src/sentinel.rs::dispatch_loop` - Main job dispatch
- `crates/casparian_sentinel/src/sentinel.rs::record_materializations_for_job` - Incremental tracking
- `crates/casparian_sentinel/src/control.rs` - Control API request/response types
- `crates/casparian_sentinel/src/db/queue.rs::Job` - Canonical job model

### Execution Plane
- `crates/casparian_worker/src/worker.rs::execute_job_inner` - Job execution
- `crates/casparian_worker/src/worker.rs::compute_source_hash` - Input identity
- `crates/casparian_worker/src/worker.rs::validate_lineage_columns` - Lineage validation
- `crates/casparian_worker/src/schema_validation.rs::validate_record_batch` - Schema enforcement

### Sinks
- `crates/casparian_sinks/src/lib.rs::output_filename` - Output naming
- `crates/casparian_sinks/src/lib.rs::inject_lineage_columns` - Lineage injection
- `crates/casparian_sinks/src/lib.rs::DuckDbSink::write_batch` - DuckDB persistence

### UI
- `tauri-ui/src-tauri/src/commands/jobs.rs` - Job commands
- `tauri-ui/src-tauri/src/state.rs::try_control_client` - Control API client

---

## Getting Help

- Component docs: crate-specific `CLAUDE.md` files
- Architecture: `ARCHITECTURE.md`
- CLI usage: `casparian --help`
