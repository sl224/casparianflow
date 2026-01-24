# Local Control Plane API (Pre-v1) - Implementation Status

This document describes the Control Plane API architecture and implementation status.

## Architecture Decision: MCP Direct Crate Calls (No HTTP Server)

**Decision:** MCP server calls Rust crates directly as a library. No HTTP server.

**Rationale:**
- Simplest architecture for pre-v1
- No additional service to manage
- Direct function calls are faster and easier to debug
- Avoids DuckDB single-writer conflicts (all writes go through one process)

**Implementation:**
- `casparian_mcp` calls `casparian_sentinel`'s `ApiStorage` directly
- All job/event/approval state stored in DuckDB tables
- No `control_plane.json` discovery file needed
- No bearer token authentication needed

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
│  │                    db_store.rs                            │   │
│  │  Bridges MCP types ←→ ApiStorage (sentinel)              │   │
│  └──────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              │ Direct library calls              │
│                              ▼                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │            casparian_sentinel::db::ApiStorage             │   │
│  │  DuckDB tables: cf_api_jobs, cf_api_events, cf_api_approvals │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Implementation Status

### Phase 1: Protocol types (casparian_protocol) - COMPLETE

Types defined in `crates/casparian_protocol/src/http_types.rs`:

| Type | Status | Notes |
|------|--------|-------|
| `Job`, `HttpJobStatus`, `HttpJobType` | Done | Job lifecycle types |
| `JobSpec`, `JobProgress`, `JobResult` | Done | Job request/response |
| `Event`, `EventId`, `EventType` | Done | Monotonic events per job |
| `Approval`, `ApprovalStatus`, `ApprovalOperation` | Done | Approval workflow |
| `RedactionPolicy`, `RedactionMode` | Done | Sensitive data handling |
| `QueryRequest`, `QueryResponse` | Done | Read-only query types |
| `ViolationSummary`, `ViolationType` | Done | Schema violation reporting |

### Phase 2: Storage Layer (casparian_sentinel) - COMPLETE

Implemented in `crates/casparian_sentinel/src/db/api_storage.rs`:

| Component | Status | Notes |
|-----------|--------|-------|
| `ApiStorage` struct | Done | Main storage interface |
| DDL: `cf_api_jobs` | Done | Job tracking with progress/result |
| DDL: `cf_api_events` | Done | Monotonic event_id per job |
| DDL: `cf_api_approvals` | Done | Approval workflow state |
| `create_job()` | Done | Create new job record |
| `get_job()`, `list_jobs()` | Done | Query jobs |
| `update_job_status/progress/result` | Done | Job lifecycle updates |
| `cancel_job()` | Done | Cancel with event emission |
| `next_event_id()` | Done | Monotonic per-job IDs |
| `insert_event()`, `list_events()` | Done | Event persistence and polling |
| `create_approval()` | Done | Create approval request |
| `approve()`, `reject()` | Done | Decide on approvals |
| `expire_approvals()` | Done | TTL enforcement |
| `cleanup_old_data()` | Done | Job/event retention |

### Phase 3: MCP Integration - COMPLETE

Bridge layer in `crates/casparian_mcp/src/db_store.rs`:

| Component | Status | Notes |
|-----------|--------|-------|
| `DbJobStore` | Done | Adapts MCP job operations to ApiStorage |
| `DbApprovalStore` | Done | Adapts MCP approval operations to ApiStorage |
| Type conversion | Done | `PluginRef` ↔ plugin_name/version |

### Phase 4: Redaction Module - COMPLETE

Implemented in `crates/casparian_mcp/src/redaction.rs`:

| Feature | Status | Notes |
|---------|--------|-------|
| Hash mode | Done | SHA256 prefix (8 chars) |
| Truncate mode | Done | First N chars + "..." |
| None mode | Done | Raw values (explicit opt-in) |
| `redact_rows()` | Done | Apply to query results |
| `is_sensitive_column()` | Done | Heuristic detection |

### Phase 5: Query Endpoint Hardening - COMPLETE

Implemented in `crates/casparian_mcp/src/tools/query.rs`:

| Feature | Status | Notes |
|---------|--------|-------|
| Read-only DuckDB | Done | Opens with read-only mode |
| SQL allowlist | Done | SELECT, WITH, EXPLAIN only |
| Forbidden keywords | Done | INSERT, UPDATE, DELETE, DROP, etc. |
| Row limit enforcement | Done | max_rows from security config |
| Redaction applied | Done | Uses redaction module |
| Timeout enforcement | Done | Configurable timeout_ms |

### Phase 6: E2E Tests - COMPLETE

Test infrastructure in `tests/e2e/mcp/`:

| Component | Status | Notes |
|-----------|--------|-------|
| `test_mcp_server.sh` | Done | Smoke test (server starts, tools/list works) |
| `run_with_claude.sh` | Done | **Authoritative E2E tests** via Claude Code CLI |
| `claude_prompt.md` | Done | Backtest flow test instructions |
| `claude_prompt_approval.md` | Done | Approval flow test instructions |
| `.mcp.json` | Done | Project-scoped MCP server config |

**Important:** Unit tests in `crates/casparian_mcp/src/` use real DuckDB (in-memory). E2E tests use real Claude Code CLI calls - no mocking.

---

## Key Design Decisions

### Event Ordering

Events use monotonic `event_id` per job:
```sql
SELECT COALESCE(MAX(event_id), 0) + 1
FROM cf_api_events
WHERE job_id = ?
```

This ensures:
- Events are always ordered within a job
- Polling with `after_event_id` returns only new events
- No race conditions in event ordering

### Approval Workflow

1. Tool requests write operation → creates approval (status: pending)
2. Human approves via CLI: `casparian mcp approve <id>`
3. Approval status changes to `approved`
4. Job is created and linked to approval
5. Job executes and emits events

### Pre-v1 Data Handling

Per project rules:
- No migrations: drop/recreate tables on schema change
- Delete `~/.casparian_flow/casparian_flow.duckdb` if needed
- No backward compatibility concerns

---

## What Was NOT Implemented (Intentionally)

The original plan included an HTTP server. This was **rejected** in favor of direct crate calls:

- ~~API versioning~~ - Not needed without HTTP
- ~~Bearer token auth~~ - Not needed without HTTP
- ~~`control_plane.json` discovery~~ - Not needed without HTTP
- ~~Response size budgets~~ - Handled by MCP output budgets instead
- ~~Loopback-only binding~~ - Not applicable

---

## Running Tests

```bash
# Unit tests (uses in-memory DuckDB)
cargo test -p casparian_mcp
cargo test -p casparian_sentinel

# E2E smoke test
./tests/e2e/mcp/test_mcp_server.sh

# E2E with Claude Code (authoritative)
./tests/e2e/mcp/run_with_claude.sh          # Backtest flow
./tests/e2e/mcp/run_with_claude.sh approval # Approval flow
```

---

## Related Documentation

- `docs/execution_plan_mcp.md` - Full MCP implementation plan
- `crates/casparian_mcp/CLAUDE.md` - MCP crate guide
- `crates/casparian_sentinel/CLAUDE.md` - Sentinel crate guide
- `crates/casparian_protocol/CLAUDE.md` - Protocol types guide
