# MCP Server Execution Plan

**Status:** Phase 0 Complete + Control Plane API Complete
**Date:** 2026-01-22 (Updated)
**Owner:** Platform
**Related:** `docs/v1_scope.md`, `docs/decisions/ADR-021-ai-agentic-iteration-workflow.md`, `specs/jobs_progress.md`, `docs/local_control_plane_api_plan.md`

---

## Implementation Status (2026-01-22)

### Phase 0: Foundation - COMPLETE

All Phase 0 components have been implemented in `crates/casparian_mcp/`:

| Component | Status | Implementation Notes |
|-----------|--------|---------------------|
| **MCP Crate Structure** | DONE | Full crate with modules for protocol, server, security, jobs, approvals, tools |
| **Protocol Types** | DONE | JSON-RPC 2.0 types in `protocol.rs` - `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcError`, MCP-specific messages |
| **Stdio Server** | DONE | `McpServer` in `server.rs` - reads from stdin, writes to stdout, tool dispatch |
| **CLI Command** | DONE | `casparian mcp serve` + `casparian mcp approve` + `casparian mcp list` in main crate |
| **Security Subsystem** | DONE | Path allowlist, output budget, audit logging |
| **Job Subsystem** | DONE | `JobManager`, `JobStore`, states: Queued/Running/Completed/Failed/Cancelled/Stalled |
| **Approval Subsystem** | DONE | `ApprovalManager`, `ApprovalStore`, file-based persistence |
| **Core Tools** | DONE | plugins, scan, preview, query (with SQL allowlist + read-only DuckDB) |
| **Job Tools** | DONE | backtest_start, run_request, job_status/cancel/list |
| **Approval Tools** | DONE | approval_status, approval_list, approval_decide |

### Control Plane API Integration - COMPLETE

MCP now integrates with `casparian_sentinel`'s `ApiStorage` for persistent job/event/approval management:

| Component | Status | Implementation Notes |
|-----------|--------|---------------------|
| **Protocol HTTP Types** | DONE | `casparian_protocol/src/http_types.rs` - Job, Event, Approval, Query types |
| **ApiStorage** | DONE | `casparian_sentinel/src/db/api_storage.rs` - DuckDB-backed storage |
| **Bridge Layer** | DONE | `casparian_mcp/src/db_store.rs` - DbJobStore, DbApprovalStore |
| **Redaction Module** | DONE | `casparian_mcp/src/redaction.rs` - hash/truncate/none modes |
| **Query Hardening** | DONE | Read-only DuckDB, SQL allowlist, redaction applied |

**Architecture Decision:** MCP calls Rust crates directly (no HTTP server). See `docs/local_control_plane_api_plan.md`.

### Key Implementation Decisions

1. **No Circular Dependencies**: `casparian_mcp` depends on sub-crates (`casparian_db`, `casparian_schema`, `casparian_sentinel`) but NOT the main `casparian` crate. The main crate depends on `casparian_mcp`.

2. **Direct Crate Calls (No HTTP)**: MCP server calls `casparian_sentinel::ApiStorage` directly for job/event/approval management. No separate HTTP server.

3. **DuckDB Storage**: Jobs, events, and approvals are stored in DuckDB tables (`cf_api_jobs`, `cf_api_events`, `cf_api_approvals`) for persistence and queryability.

4. **Monotonic Event IDs**: Events use per-job monotonic IDs for strict ordering and efficient polling.

5. **Security First**: Path validation, SQL allowlist, read-only query connections, and output budgets are enforced.

### File Structure (Implemented)

```
crates/casparian_mcp/
├── CLAUDE.md                     # Crate-specific Claude Code instructions
├── Cargo.toml                    # Dependencies: tokio, serde, uuid, sha2, walkdir, etc.
├── src/
│   ├── lib.rs                    # Crate root with re-exports
│   ├── protocol.rs               # JSON-RPC 2.0 + MCP message types
│   ├── types.rs                  # PluginRef, DataType, SchemaDefinition, RedactionPolicy
│   ├── server.rs                 # McpServer + McpServerConfig
│   ├── db_store.rs               # Bridge to sentinel's ApiStorage (DbJobStore, DbApprovalStore)
│   ├── redaction.rs              # Value redaction (hash/truncate/none modes)
│   ├── security/
│   │   ├── mod.rs                # SecurityConfig, SecurityError
│   │   ├── path_allowlist.rs     # PathAllowlist with canonicalization + symlink handling
│   │   ├── output_budget.rs      # OutputBudget (max_bytes, max_rows)
│   │   └── audit.rs              # AuditLog (NDJSON to file)
│   ├── jobs/
│   │   ├── mod.rs                # JobId, JobState, JobProgress, Job
│   │   ├── manager.rs            # JobManager lifecycle methods
│   │   └── store.rs              # JobStore JSON file persistence
│   ├── approvals/
│   │   ├── mod.rs                # ApprovalId, ApprovalRequest, ApprovalStatus
│   │   ├── manager.rs            # ApprovalManager lifecycle methods
│   │   └── store.rs              # ApprovalStore JSON file persistence
│   └── tools/
│       ├── mod.rs                # McpTool trait
│       ├── registry.rs           # ToolRegistry for dispatch
│       ├── plugins.rs            # casparian_plugins
│       ├── scan.rs               # casparian_scan
│       ├── preview.rs            # casparian_preview
│       ├── query.rs              # casparian_query (SQL allowlist + read-only DuckDB + redaction)
│       ├── backtest.rs           # casparian_backtest_start
│       ├── run.rs                # casparian_run_request
│       ├── job.rs                # job_status, job_cancel, job_list
│       └── approval.rs           # approval_status, approval_list, approval_decide

crates/casparian_sentinel/
├── CLAUDE.md                     # Crate-specific Claude Code instructions
├── src/
│   └── db/
│       └── api_storage.rs        # ApiStorage - DuckDB storage for Control Plane API

crates/casparian_protocol/
├── CLAUDE.md                     # Crate-specific Claude Code instructions
├── src/
│   └── http_types.rs             # HTTP API types (Job, Event, Approval, Query, Redaction)
```

### E2E Test Infrastructure (2026-01-22)

Comprehensive E2E test infrastructure in `tests/e2e/mcp/`:

| File | Purpose |
|------|---------|
| `test_mcp_server.sh` | Smoke test only - verifies server starts and tools/list works |
| `run_with_claude.sh` | **Authoritative E2E tests** via Claude Code CLI |
| `claude_prompt.md` | Backtest flow test instructions |
| `claude_prompt_approval.md` | Approval flow test instructions |
| `result.schema.json` | JSON schema for test results |

Also created `.mcp.json` at project root for project-scoped MCP server configuration.

**Authentication:**
- Uses Claude CLI session authentication by default (no API key required)
- Run `claude login` to authenticate if needed
- Falls back to `ANTHROPIC_API_KEY` env var if CLI not authenticated

**Running smoke test (quick validation):**
```bash
./tests/e2e/mcp/test_mcp_server.sh
```

**Running Claude Code E2E tests (authoritative):**
```bash
# Backtest flow test
./tests/e2e/mcp/run_with_claude.sh

# Approval flow test
./tests/e2e/mcp/run_with_claude.sh approval

# Dry run (preview)
./tests/e2e/mcp/run_with_claude.sh --dry-run
```

**Test Results:**
- Results saved to `tests/e2e/mcp/results/`
- Each run generates a timestamped JSON result file
- Raw Claude output saved alongside for debugging

### Next Steps

1. **Phase 1**: Implement ephemeral contracts and schema tools for AI iteration
   - `EphemeralSchemaContract` in `casparian_schema`
   - `casparian_schema_propose` and `casparian_schema_promote` tools
   - Enhanced `ViolationContext` with `SuggestedFix` generation

2. **Enhanced Progress Reporting**: Detailed progress with per-output metrics and stall detection

3. **Phase 2 Polish**: Additional tools, advanced security, performance optimization

---

## Overview

This document defines the execution plan for implementing the MCP (Model Context Protocol)
server for Casparian Flow. MCP enables AI assistants (Claude, etc.) to interact with
Casparian programmatically, enabling AI-assisted parser development and data workflows.

**Goal:** Ship a production-ready MCP server that exposes Casparian's core capabilities
as MCP tools, with appropriate human approval gates for write operations.

**Key Design Principles:**

1. **Job-first architecture:** Long-running operations return immediately with a `job_id`; progress is polled via separate tools.
2. **Non-blocking approvals:** Write operations create approval requests; humans approve out-of-band.
3. **Read-only by default:** Query tool uses read-only DuckDB connection; samples are redacted by default.
4. **Security from day one:** Path allowlists, output budgets, and audit logging are P0, not afterthoughts.
5. **Per-output schemas:** Multi-output parsers are first-class; schemas are always keyed by output name.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    AI Assistant (Claude)                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ MCP Protocol (JSON-RPC over stdio)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                 crates/casparian_mcp/                            │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Core Subsystems                        │   │
│  ├──────────────────────────────────────────────────────────┤   │
│  │  Server       │ JSON-RPC stdio, tool dispatch            │   │
│  │  Jobs         │ Async job lifecycle (start/status/cancel)│   │
│  │  Approvals    │ Non-blocking approval requests           │   │
│  │  Security     │ Path allowlist, output budgets, redaction│   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    Tool Implementations                   │   │
│  ├──────────────────────────────────────────────────────────┤   │
│  │  Discovery    │ scan, plugins                            │   │
│  │  Preview      │ preview (read-only)                      │   │
│  │  Jobs         │ backtest_start, run_request, job_*       │   │
│  │  Query        │ query (read-only sandbox)                │   │
│  │  Schema       │ schema_propose, schema_promote           │   │
│  │  Approvals    │ approval_status, approval_list           │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Direct crate calls (NOT CLI shelling)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Existing Casparian Crates                     │
│  • casparian_worker (execution)  • casparian_schema (contracts) │
│  • casparian_scout (discovery)   • casparian_sinks (output)     │
│  • casparian_db (storage)        • casparian_backtest (testing) │
└─────────────────────────────────────────────────────────────────┘
```

---

## Shared Type Definitions

These definitions are referenced throughout the tool specifications.

### PluginRef

Standardized parser/plugin identity for future-proofing.

```json
{
  "$id": "PluginRef",
  "oneOf": [
    {
      "type": "object",
      "properties": {
        "plugin": { "type": "string", "description": "Plugin ID" },
        "version": { "type": "string", "description": "Semver version" }
      },
      "required": ["plugin"]
    },
    {
      "type": "object",
      "properties": {
        "path": { "type": "string", "description": "Local file path (dev only)" }
      },
      "required": ["path"]
    }
  ],
  "examples": [
    { "plugin": "evtx_native", "version": "0.1.0" },
    { "plugin": "fix_parser" },
    { "path": "./parsers/my_parser.py" }
  ]
}
```

### SchemaDefinition

Per-output schema definition.

```json
{
  "$id": "SchemaDefinition",
  "type": "object",
  "properties": {
    "output_name": { "type": "string" },
    "mode": { "type": "string", "enum": ["strict", "allow_extra", "allow_missing_optional"], "default": "strict" },
    "columns": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "type": { "$ref": "#/definitions/DataType" },
          "nullable": { "type": "boolean", "default": true },
          "format": { "type": "string", "description": "Optional format string for temporal types" }
        },
        "required": ["name", "type"]
      }
    }
  },
  "required": ["output_name", "columns"]
}
```

### DataType

```json
{
  "$id": "DataType",
  "oneOf": [
    { "type": "string", "enum": ["string", "int64", "float64", "boolean", "date", "binary"] },
    {
      "type": "object",
      "properties": {
        "kind": { "type": "string", "enum": ["decimal", "timestamp_tz"] },
        "precision": { "type": "integer" },
        "scale": { "type": "integer" },
        "timezone": { "type": "string" }
      },
      "required": ["kind"]
    }
  ],
  "examples": [
    "string",
    "int64",
    { "kind": "decimal", "precision": 18, "scale": 8 },
    { "kind": "timestamp_tz", "timezone": "UTC" }
  ]
}
```

### SchemasMap

Multi-output schema specification.

```json
{
  "$id": "SchemasMap",
  "type": "object",
  "additionalProperties": { "$ref": "#/definitions/SchemaDefinition" },
  "description": "Map of output_name -> SchemaDefinition",
  "example": {
    "evtx_events": { "output_name": "evtx_events", "columns": [...] },
    "evtx_eventdata_kv": { "output_name": "evtx_eventdata_kv", "columns": [...] }
  }
}
```

### RedactionPolicy

Controls sensitive data exposure in tool outputs.

```json
{
  "$id": "RedactionPolicy",
  "type": "object",
  "properties": {
    "mode": {
      "type": "string",
      "enum": ["none", "truncate", "hash"],
      "default": "hash",
      "description": "none=raw values, truncate=first N chars, hash=SHA256 prefix"
    },
    "max_sample_count": { "type": "integer", "default": 5 },
    "max_value_length": { "type": "integer", "default": 100 },
    "hash_prefix_length": { "type": "integer", "default": 8 }
  }
}
```

### ViolationContext

Machine-readable error context for AI learning.

```json
{
  "$id": "ViolationContext",
  "type": "object",
  "properties": {
    "output_name": { "type": "string" },
    "column": { "type": "string" },
    "violation_type": {
      "type": "string",
      "enum": ["type_mismatch", "null_not_allowed", "format_mismatch", "column_name_mismatch", "column_count_mismatch"]
    },
    "count": { "type": "integer" },
    "pct_of_rows": { "type": "number" },
    "samples": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Sample values (redacted per policy)"
    },
    "value_distribution": {
      "type": "object",
      "additionalProperties": { "type": "integer" },
      "description": "Top-K value counts (keys redacted per policy)"
    },
    "suggested_fix": { "$ref": "#/definitions/SuggestedFix" }
  }
}
```

### SuggestedFix

```json
{
  "$id": "SuggestedFix",
  "oneOf": [
    { "type": "object", "properties": { "action": { "const": "change_type" }, "from": { "$ref": "#/definitions/DataType" }, "to": { "$ref": "#/definitions/DataType" } } },
    { "type": "object", "properties": { "action": { "const": "make_nullable" } } },
    { "type": "object", "properties": { "action": { "const": "change_format" }, "suggested": { "type": "string" } } },
    { "type": "object", "properties": { "action": { "const": "add_column" }, "name": { "type": "string" }, "type": { "$ref": "#/definitions/DataType" } } },
    { "type": "object", "properties": { "action": { "const": "remove_column" }, "name": { "type": "string" } } }
  ]
}
```

---

## Phases

### Phase 0: Foundation (P0 - Must Ship)

**Goal:** MCP server with job-first architecture, non-blocking approvals, and security from day one.

#### 0.1 Create MCP Crate Structure

```bash
crates/casparian_mcp/
├── Cargo.toml
├── src/
│   ├── lib.rs                 # Crate root
│   ├── server.rs              # MCP server (stdio JSON-RPC)
│   ├── protocol.rs            # MCP protocol types
│   ├── security/
│   │   ├── mod.rs
│   │   ├── path_allowlist.rs  # Path validation + canonicalization
│   │   ├── output_budget.rs   # Response size limits
│   │   ├── redaction.rs       # Sample value redaction
│   │   └── audit.rs           # Tool invocation logging
│   ├── jobs/
│   │   ├── mod.rs
│   │   ├── manager.rs         # Job lifecycle (start/status/cancel)
│   │   └── store.rs           # Job state persistence
│   ├── approvals/
│   │   ├── mod.rs
│   │   ├── manager.rs         # Approval lifecycle
│   │   └── store.rs           # Approval state persistence
│   ├── tools/
│   │   ├── mod.rs             # Tool registry
│   │   ├── scan.rs            # casparian_scan
│   │   ├── plugins.rs         # casparian_plugins
│   │   ├── preview.rs         # casparian_preview
│   │   ├── backtest.rs        # casparian_backtest_start
│   │   ├── run.rs             # casparian_run_request
│   │   ├── job.rs             # casparian_job_status/cancel
│   │   ├── approval.rs        # casparian_approval_*
│   │   └── query.rs           # casparian_query (read-only)
│   └── types.rs               # Shared types (PluginRef, schemas, etc.)
└── tests/
    ├── protocol_test.rs       # JSON-RPC compliance
    ├── security_test.rs       # Path traversal, redaction
    └── integration_test.rs    # Full tool flows
```

**Tasks:**

- [x] **0.1.1** Create `casparian_mcp` crate with Cargo.toml
- [x] **0.1.2** Implement MCP protocol types (JSON-RPC 2.0)
- [x] **0.1.3** Implement stdio server with tool discovery (`tools/list`)
- [x] **0.1.4** Add `casparian mcp serve` CLI command
- [ ] **0.1.5** Write MCP protocol integration tests

**Dependencies:** None (new crate)

#### 0.2 Security Subsystem (P0 - MUST be in Phase 0)

**Tasks:**

- [x] **0.2.1** Path allowlist + canonicalization
  - Deny `..` traversal
  - Deny symlinks escaping configured roots
  - Default root: current working directory
  - Configurable via `--allow-path` flags

- [x] **0.2.2** Output budget enforcement
  - Max response size: 1MB default
  - Max rows returned: 10,000 default
  - Truncation with `truncated: true` indicator

- [x] **0.2.3** Redaction policy implementation
  - Default: `mode: "hash"`, `max_sample_count: 5`, `max_value_length: 100`
  - Hash uses SHA256 prefix (8 chars)
  - Raw mode requires explicit opt-in

- [x] **0.2.4** Audit logging
  - Log all tool invocations with timestamps
  - Log approval requests/responses
  - Store in `~/.casparian_flow/mcp_audit.log`

**Acceptance Criteria:**
- Path traversal attacks fail with clear error
- Large responses are truncated, not OOM
- Sample values are hashed by default
- All tool calls are logged

#### 0.3 Job Subsystem

**Tasks:**

- [x] **0.3.1** Job manager implementation
  ```rust
  pub struct JobManager {
      jobs: HashMap<JobId, JobState>,
      max_concurrent: usize,  // Default: 1
  }

  pub enum JobState {
      Pending,
      Running { started_at: DateTime<Utc>, progress: JobProgress },
      Completed { result: JobResult },
      Failed { error: String },
      Cancelled,
  }
  ```

- [x] **0.3.2** Job persistence (survives MCP server restart)
  - Store in `~/.casparian_flow/mcp_jobs/{job_id}.json`
  - TTL: 24 hours for completed jobs

- [x] **0.3.3** Concurrency control
  - Default: 1 concurrent job
  - Queue additional requests
  - Timeout: 30 minutes default

**Acceptance Criteria:**
- Jobs persist across server restarts
- Only 1 job runs at a time by default
- Jobs timeout after 30 minutes

#### 0.4 Approval Subsystem

**Tasks:**

- [x] **0.4.1** Approval manager implementation
  ```rust
  pub struct ApprovalRequest {
      pub approval_id: String,
      pub operation: ApprovalOperation,
      pub summary: ApprovalSummary,
      pub created_at: DateTime<Utc>,
      pub expires_at: DateTime<Utc>,
      pub status: ApprovalStatus,
  }

  pub enum ApprovalOperation {
      Run { plugin_ref: PluginRef, input_dir: PathBuf, output: String },
      SchemaPromote { ephemeral_id: String, output_path: PathBuf },
  }

  pub struct ApprovalSummary {
      pub description: String,
      pub file_count: usize,
      pub estimated_rows: Option<u64>,
      pub target_path: String,
  }

  pub enum ApprovalStatus {
      Pending,
      Approved { approved_at: DateTime<Utc> },
      Rejected { reason: Option<String> },
      Expired,
  }
  ```

- [x] **0.4.2** File-based approval mechanism (default)
  - Write to `~/.casparian_flow/mcp_approvals/{id}.json`
  - CLI: `casparian mcp list`
  - CLI: `casparian mcp approve <id>`
  - CLI: `casparian mcp approve <id> --reject`

- [x] **0.4.3** Approval TTL and cleanup
  - Default expiry: 1 hour
  - Auto-cleanup expired approvals

**Acceptance Criteria:**
- Write operations create approval requests, return immediately
- Approvals can be listed and actioned via CLI
- Expired approvals are rejected automatically

#### 0.5 Core Tools (Read-Only)

**Tasks:**

- [x] **0.5.1** `casparian_plugins` - List available parsers/plugins
- [x] **0.5.2** `casparian_scan` - Scan directory (no hash by default)
- [x] **0.5.3** `casparian_preview` - Preview parser output (redacted)
- [x] **0.5.4** `casparian_query` - SQL query (read-only sandbox)

**Implementation note:** Call internal crates directly, not CLI subprocess. Currently returns placeholder responses.

#### 0.6 Job-Based Tools

**Tasks:**

- [x] **0.6.1** `casparian_backtest_start` - Start backtest job, return job_id
- [x] **0.6.2** `casparian_run_request` - Create run approval request
- [x] **0.6.3** `casparian_job_status` - Get job progress/result
- [x] **0.6.4** `casparian_job_cancel` - Cancel running job
- [x] **0.6.5** `casparian_job_list` - List recent jobs

#### 0.7 Approval Tools

**Tasks:**

- [x] **0.7.1** `casparian_approval_status` - Check approval status
- [x] **0.7.2** `casparian_approval_list` - List pending approvals

---

### Phase 1: AI Iteration Support (P1 - Strongly Desired)

**Goal:** Enable fast schema/parser iteration via ephemeral contracts.

#### 1.1 EphemeralSchemaContract

Per ADR-021, ephemeral contracts are for iteration, not system-of-record.

**Tasks:**

- [ ] **1.1.1** Add `EphemeralSchemaContract` struct to `casparian_schema`
  ```rust
  pub struct EphemeralSchemaContract {
      pub ephemeral_id: String,
      pub schemas: HashMap<String, SchemaDefinition>,  // Per-output
      pub schema_hashes: HashMap<String, String>,      // Per-output hashes
      pub source: EphemeralSource,
      pub created_at: DateTime<Utc>,
      pub run_id: Option<String>,
  }
  ```

- [ ] **1.1.2** Implement schema canonicalization (sorted keys, no whitespace)
- [ ] **1.1.3** Add local file persistence
  ```
  ~/.casparian_flow/ai/contracts/{ephemeral_id}/
  ├── schemas.json           # All output schemas
  ├── schema_hashes.json     # Per-output hashes
  └── metadata.json          # run_id, timestamps, source
  ```

#### 1.2 ViolationContext Enhancement

**Tasks:**

- [ ] **1.2.1** Integrate `ViolationContext` into backtest results
- [ ] **1.2.2** Add `SuggestedFix` generation based on violation patterns
- [ ] **1.2.3** Apply redaction policy to violation samples

#### 1.3 Schema Proposal Tools

**Tasks:**

- [ ] **1.3.1** `casparian_schema_propose` - Create ephemeral schema
  - Accepts single schema OR schemas map
  - Returns ephemeral_id + per-output schema_hashes

- [ ] **1.3.2** `casparian_schema_promote` - Generate schema-as-code
  - Creates approval request (gated)
  - After approval: generates AST-extractable Python code

#### 1.4 Enhanced Progress Reporting

**Tasks:**

- [ ] **1.4.1** Add detailed progress to `JobProgress` struct
- [ ] **1.4.2** Include per-output metrics in progress
- [ ] **1.4.3** Add stall detection (30s no progress → stalled status)

---

### Phase 2: Polish and Hardening (P2 - Nice to Have)

**Goal:** Production-ready MCP server with comprehensive tooling.

#### 2.1 Additional Tools

- [ ] **2.1.1** `casparian_files` - List files with tags
- [ ] **2.1.2** `casparian_sources` - List configured sources
- [ ] **2.1.3** `casparian_quarantine_summary` - Detailed quarantine analysis

#### 2.2 Advanced Security

- [ ] **2.2.1** Rate limiting (requests per minute)
- [ ] **2.2.2** Per-tool permission configuration
- [ ] **2.2.3** Audit log rotation and retention

#### 2.3 Performance Optimization

- [ ] **2.3.1** Streaming responses for large results
- [ ] **2.3.2** Query result caching
- [ ] **2.3.3** Parallel job execution (opt-in)

---

## Tool Specifications

### casparian_plugins

**Purpose:** List available parsers/plugins.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "include_dev": { "type": "boolean", "default": false, "description": "Include path-based dev plugins" }
  }
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "plugins": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "version": { "type": "string" },
          "runtime": { "type": "string", "enum": ["python", "native"] },
          "outputs": { "type": "array", "items": { "type": "string" } },
          "topics": { "type": "array", "items": { "type": "string" } }
        }
      }
    }
  }
}
```

**Human Gate:** None

---

### casparian_scan

**Purpose:** Discover files in a directory.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "path": { "type": "string", "description": "Directory to scan (must be within allowed paths)" },
    "pattern": { "type": "string", "description": "Glob pattern (e.g., *.evtx)" },
    "recursive": { "type": "boolean", "default": true },
    "hash_mode": {
      "type": "string",
      "enum": ["none", "fast", "sha256"],
      "default": "none",
      "description": "none=skip hashing, fast=xxhash, sha256=full hash"
    },
    "limit": { "type": "integer", "default": 1000 }
  },
  "required": ["path"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "files": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "path": { "type": "string" },
          "size": { "type": "integer" },
          "modified": { "type": "string", "format": "date-time" },
          "hash": { "type": "string", "description": "Only present if hash_mode != none" }
        }
      }
    },
    "total_size": { "type": "integer" },
    "file_count": { "type": "integer" },
    "truncated": { "type": "boolean" }
  }
}
```

**Human Gate:** None

**Security:** Path must be within allowed roots.

---

### casparian_preview

**Purpose:** Preview parser output on sample files.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "plugin_ref": { "$ref": "#/definitions/PluginRef" },
    "files": { "type": "array", "items": { "type": "string" }, "maxItems": 10 },
    "limit": { "type": "integer", "default": 100, "maximum": 1000 },
    "redaction": { "$ref": "#/definitions/RedactionPolicy" }
  },
  "required": ["plugin_ref", "files"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "outputs": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "properties": {
          "schema": { "$ref": "#/definitions/SchemaDefinition" },
          "schema_hash": { "type": "string" },
          "sample_rows": { "type": "array", "description": "Redacted per policy" },
          "row_count": { "type": "integer" }
        }
      }
    },
    "errors": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "file": { "type": "string" },
          "error": { "type": "string" }
        }
      }
    }
  }
}
```

**Human Gate:** None (read-only, no output written)

---

### casparian_backtest_start

**Purpose:** Start a backtest job (non-blocking).

**Input:**
```json
{
  "type": "object",
  "properties": {
    "plugin_ref": { "$ref": "#/definitions/PluginRef" },
    "input_dir": { "type": "string" },
    "schemas": { "$ref": "#/definitions/SchemasMap", "description": "Optional per-output schemas" },
    "redaction": { "$ref": "#/definitions/RedactionPolicy" }
  },
  "required": ["plugin_ref", "input_dir"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "job_id": { "type": "string" },
    "status": { "type": "string", "enum": ["queued", "running"] }
  }
}
```

**Human Gate:** None (no output written)

**Job Result (via job_status):**
```json
{
  "type": "object",
  "properties": {
    "files_processed": { "type": "integer" },
    "files_total": { "type": "integer" },
    "outputs": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "properties": {
          "rows_emitted": { "type": "integer" },
          "rows_quarantined": { "type": "integer" },
          "pass_rate": { "type": "number" },
          "quarantine_pct": { "type": "number" },
          "schema_hash": { "type": "string" },
          "violations": {
            "type": "array",
            "items": { "$ref": "#/definitions/ViolationContext" }
          }
        }
      }
    },
    "elapsed_ms": { "type": "integer" }
  }
}
```

---

### casparian_run_request

**Purpose:** Request parser execution (creates approval request).

**Input:**
```json
{
  "type": "object",
  "properties": {
    "plugin_ref": { "$ref": "#/definitions/PluginRef" },
    "input_dir": { "type": "string" },
    "output": { "type": "string", "description": "Output path or sink URL" },
    "schemas": { "$ref": "#/definitions/SchemasMap", "description": "Optional schema override" }
  },
  "required": ["plugin_ref", "input_dir"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "approval_id": { "type": "string" },
    "status": { "const": "pending_approval" },
    "summary": {
      "type": "object",
      "properties": {
        "description": { "type": "string" },
        "file_count": { "type": "integer" },
        "estimated_rows": { "type": "integer" },
        "target_path": { "type": "string" }
      }
    },
    "expires_at": { "type": "string", "format": "date-time" },
    "approve_command": { "type": "string", "description": "CLI command to approve" }
  }
}
```

**Human Gate:** YES - Returns approval_id; human must run `casparian approvals approve <id>`

**After Approval:** Job is created and can be tracked via `job_status`.

---

### casparian_job_status

**Purpose:** Get job progress or result.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "job_id": { "type": "string" }
  },
  "required": ["job_id"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "job_id": { "type": "string" },
    "status": { "type": "string", "enum": ["queued", "running", "completed", "failed", "cancelled", "stalled"] },
    "progress": {
      "type": "object",
      "properties": {
        "phase": { "type": "string" },
        "items_done": { "type": "integer" },
        "items_total": { "type": "integer" },
        "elapsed_ms": { "type": "integer" },
        "eta_ms": { "type": "integer" }
      }
    },
    "result": { "description": "Present when status=completed; structure depends on job type" },
    "error": { "type": "string", "description": "Present when status=failed" }
  }
}
```

---

### casparian_job_cancel

**Purpose:** Cancel a running or queued job.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "job_id": { "type": "string" }
  },
  "required": ["job_id"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "job_id": { "type": "string" },
    "status": { "type": "string", "enum": ["cancelled", "already_completed", "not_found"] }
  }
}
```

---

### casparian_job_list

**Purpose:** List recent jobs.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "status": { "type": "string", "enum": ["all", "running", "completed", "failed"], "default": "all" },
    "limit": { "type": "integer", "default": 20 }
  }
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "jobs": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "job_id": { "type": "string" },
          "type": { "type": "string", "enum": ["backtest", "run"] },
          "status": { "type": "string" },
          "created_at": { "type": "string", "format": "date-time" },
          "plugin_ref": { "$ref": "#/definitions/PluginRef" }
        }
      }
    }
  }
}
```

---

### casparian_approval_status

**Purpose:** Check status of an approval request.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "approval_id": { "type": "string" }
  },
  "required": ["approval_id"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "approval_id": { "type": "string" },
    "status": { "type": "string", "enum": ["pending", "approved", "rejected", "expired"] },
    "summary": { "$ref": "#/definitions/ApprovalSummary" },
    "job_id": { "type": "string", "description": "Present if approved and job started" },
    "expires_at": { "type": "string", "format": "date-time" }
  }
}
```

---

### casparian_approval_list

**Purpose:** List pending approval requests.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "status": { "type": "string", "enum": ["pending", "all"], "default": "pending" }
  }
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "approvals": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "approval_id": { "type": "string" },
          "operation": { "type": "string" },
          "summary": { "$ref": "#/definitions/ApprovalSummary" },
          "created_at": { "type": "string", "format": "date-time" },
          "expires_at": { "type": "string", "format": "date-time" }
        }
      }
    }
  }
}
```

---

### casparian_query

**Purpose:** Run SQL query on output data (READ-ONLY).

**Input:**
```json
{
  "type": "object",
  "properties": {
    "sql": { "type": "string" },
    "limit": { "type": "integer", "default": 1000, "maximum": 10000 },
    "timeout_ms": { "type": "integer", "default": 30000, "maximum": 300000 },
    "redaction": { "$ref": "#/definitions/RedactionPolicy" }
  },
  "required": ["sql"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "columns": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "name": { "type": "string" },
          "type": { "type": "string" }
        }
      }
    },
    "rows": { "type": "array", "items": { "type": "array" } },
    "row_count": { "type": "integer" },
    "truncated": { "type": "boolean" },
    "elapsed_ms": { "type": "integer" }
  }
}
```

**Human Gate:** None

**Security:**
- Opens DuckDB in **read-only mode**
- SQL allowlist: `SELECT`, `WITH`, `EXPLAIN` only
- Forbids: `INSERT`, `UPDATE`, `DELETE`, `DROP`, `CREATE`, `ALTER`, `COPY`, `INSTALL`, `LOAD`
- Query timeout enforced
- Row limit enforced

---

### casparian_schema_propose (P1)

**Purpose:** Create ephemeral schema for iteration.

**Input:**
```json
{
  "type": "object",
  "oneOf": [
    {
      "properties": {
        "schema": { "$ref": "#/definitions/SchemaDefinition" }
      },
      "required": ["schema"]
    },
    {
      "properties": {
        "schemas": { "$ref": "#/definitions/SchemasMap" }
      },
      "required": ["schemas"]
    }
  ]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "ephemeral_id": { "type": "string" },
    "schema_hashes": {
      "type": "object",
      "additionalProperties": { "type": "string" }
    },
    "valid": { "type": "boolean" },
    "validation_errors": { "type": "array", "items": { "type": "string" } }
  }
}
```

**Human Gate:** None (ephemeral, not persisted to system-of-record)

---

### casparian_schema_promote (P1)

**Purpose:** Generate schema-as-code from ephemeral contract.

**Input:**
```json
{
  "type": "object",
  "properties": {
    "ephemeral_id": { "type": "string" },
    "output_path": { "type": "string" }
  },
  "required": ["ephemeral_id", "output_path"]
}
```

**Output:**
```json
{
  "type": "object",
  "properties": {
    "approval_id": { "type": "string" },
    "status": { "const": "pending_approval" },
    "preview": { "type": "string", "description": "Generated code preview (first 1000 chars)" },
    "approve_command": { "type": "string" }
  }
}
```

**Human Gate:** YES - Creates approval request

---

## CLI Commands for Approval Management

```bash
# List pending approvals
casparian approvals list

# Show approval details
casparian approvals show <approval_id>

# Approve a request
casparian approvals approve <approval_id>

# Reject a request
casparian approvals reject <approval_id> [--reason "..."]

# List recent jobs
casparian jobs list [--status running|completed|failed]

# Show job details
casparian jobs show <job_id>

# Cancel a job
casparian jobs cancel <job_id>
```

---

## Implementation Dependencies

```
Phase 0 (Foundation) - All P0
├── 0.1 MCP Crate Structure
├── 0.2 Security Subsystem ◄─── CRITICAL: Must be P0
│   ├── Path allowlist
│   ├── Output budgets
│   ├── Redaction policy
│   └── Audit logging
├── 0.3 Job Subsystem
│   ├── Job manager
│   └── Job persistence
├── 0.4 Approval Subsystem
│   ├── Approval manager
│   └── File-based approvals
├── 0.5 Core Tools (Read-Only)
│   ├── casparian_plugins
│   ├── casparian_scan
│   ├── casparian_preview
│   └── casparian_query (read-only sandbox)
├── 0.6 Job-Based Tools
│   ├── casparian_backtest_start
│   ├── casparian_run_request
│   └── casparian_job_*
└── 0.7 Approval Tools
    └── casparian_approval_*

Phase 1 (AI Iteration) - P1
├── 1.1 EphemeralSchemaContract (per-output)
├── 1.2 ViolationContext (with redaction)
├── 1.3 Schema tools (propose/promote)
└── 1.4 Enhanced progress reporting

Phase 2 (Polish) - P2
├── Additional tools
├── Advanced security
└── Performance optimization
```

---

## Testing Strategy

### Unit Tests

- [ ] Path allowlist: traversal attacks, symlink escapes
- [ ] Redaction: hash mode, truncate mode, boundary cases
- [ ] SQL allowlist: blocked commands, edge cases
- [ ] Job lifecycle: state transitions, timeouts
- [ ] Approval lifecycle: expiry, cleanup

### Integration Tests

- [ ] Full MCP request/response cycle
- [ ] Job polling workflow
- [ ] Approval → job execution workflow
- [ ] Error handling and recovery

### E2E Tests

- [ ] Claude Code → MCP Server → Parser execution → Query results
- [ ] AI iteration loop (propose → backtest → iterate → promote)
- [ ] Large file handling (memory, timeouts)
- [ ] Security: attempt path traversal, SQL injection

---

## Success Criteria

### Phase 0

- [ ] All P0 tools respond correctly
- [ ] Path traversal attacks blocked
- [ ] Query tool is truly read-only
- [ ] Approvals are non-blocking
- [ ] Jobs persist across restarts

### Phase 1

- [ ] AI can iterate on schema without human intervention
- [ ] Backtest returns actionable violation context (redacted)
- [ ] Schema promotion requires approval

### Phase 2

- [ ] Production-ready security posture
- [ ] Comprehensive audit logging
- [ ] Performance meets targets (<500ms for typical operations)

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| MCP protocol changes | Medium | Pin to stable MCP spec version |
| Approval UX confusion | Medium | Clear CLI output, approve_command in responses |
| Large result payloads | Medium | Output budgets + truncation |
| Destructive SQL commands | High | Read-only connection + SQL allowlist |
| Path traversal | High | Allowlist + canonicalization in P0 |
| Sensitive data leakage | High | Redaction policy default=hash |
| Job deadlocks | Medium | Timeout + stall detection |

---

## Resolved Design Decisions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Progress streaming | Job-based polling | stderr unreliable in MCP clients |
| Human gate mechanism | Non-blocking file-based approvals | Prevents deadlocks in agent loops |
| Query sandboxing | Read-only DuckDB + SQL allowlist | Prevents accidental destructive commands |
| Security timing | P0 (day one) | Filesystem access + SQL in agentic env is high risk |
| Sample redaction | Hash by default | DFIR data contains sensitive content |
| Parser identity | PluginRef (plugin@version or path) | Future-proof for native plugins |
| Multi-output schemas | Per-output everywhere | Parsers are multi-output by design |
| Hashing in scan | Opt-in (hash_mode) | Hashing large evidence is expensive |
| CLI vs crate calls | Direct crate calls | Avoid CLI refactoring, better error handling |
