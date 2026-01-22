# Database Abstraction + Job Events Spec

**Status:** Draft  
**Version:** 0.1  
**Parent:** spec.md  
**Related:** docs/execution_plan.md, specs/job_types.md, specs/pipelines.md  
**Last Updated:** 2026-01-18

---

## 1. Purpose

Define a single database abstraction in `casparian_db` for DuckDB (v1).
Future backends may be added later, but v1 is DuckDB-only. Add an append-only
job event log that complements the mutable job queue, enabling audit trails
without breaking atomic job claiming.

---

## 2. Constraints

- **Single DB only.** No dual DB usage.
- **DuckDB only (v1).** Local-first, air-gapped by default.
- **Data-oriented design.** Explicit rows/values, no ORM, minimal surface.
- **Control plane needs atomic job claiming.** Mutable queue table remains.
- **Audit trail is required.** Append-only job events table required.
- **Single-worker policy (v1).** DuckDB is single-writer; v1 uses one worker
  per DB file. Multi-worker concurrency is deferred to a future server backend.

---

## 3. Architecture Overview

### 3.1 `casparian_db` Interface (Minimal Surface)

```rust
pub trait Dbx: Send + Sync {
    async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, DbxError>;
    async fn query_all(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, DbxError>;
    async fn query_one(&self, sql: &str, params: &[DbValue]) -> Result<DbRow, DbxError>;
    async fn transaction(&self) -> Result<Box<dyn DbxTxn>, DbxError>;
}
```

`DbConnection` is the concrete implementation and **the single entry point**
for all database access. Pool-based APIs are not used in application layers.

### 3.2 Backend Implementations

- **DuckDB (v1):** single-writer enforced by db connection owner.
- **Future:** Postgres/SQL Server can be added later behind the same interface.

### 3.3 Write Discipline

- **Sentinel is the single writer.**
- Workers emit events to Sentinel; Sentinel batches DB writes.
- TUI/CLI are readers only.

### 3.4 Implementation Approach (Simplified)

Ship a single, concrete `DbConnection` that owns the async boundary via a
single DB actor thread. Avoid trait-object adapters until a second backend is
actually added.

Key points:
- `DbConnection` is cheap-cloneable (internal `Arc`).
- All async methods send requests to the actor and await a `oneshot` response.
- The actor owns the DB connection (thread affinity).
- Only one transaction API: closure-based, to keep txn state on the actor thread.

If we add a server backend later, either:
- Add `DbConnection::open_postgres()` and internally store an enum backend, or
- Introduce a separate concrete type (no API break for existing callers).

Proposed shape:

```rust
pub struct DbConnection {
    backend: Arc<Backend>,
}

enum Backend {
    Actor(ActorBackend),
    // Future: Postgres(PostgresBackend)
}

impl DbConnection {
    pub async fn execute(&self, sql: &str, params: &[DbValue]) -> Result<u64, DbError>;
    pub async fn query_all(&self, sql: &str, params: &[DbValue]) -> Result<Vec<DbRow>, DbError>;
    pub async fn transaction<F, Fut, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&mut DbTxnHandle) -> Fut + Send,
        Fut: Future<Output = Result<T, DbError>> + Send;
}
```

Notes:
- `DbError` should include `retryable()` or a `Transient` vs `Permanent` class.
- `DbOptions` should include timeouts for queue wait and execution.

---

## 4. Job Queue + Event Log (Dual-Table Model)

### 4.1 Mutable Queue (Atomic Claiming)

`cf_processing_queue` remains the authoritative queue for claiming.
Updates are limited to state transitions (queued → running → completed/failed).

### 4.2 Append-Only Events

```sql
CREATE TABLE cf_job_events (
  event_id BIGINT PRIMARY KEY,
  job_id TEXT NOT NULL,
  event_type TEXT NOT NULL,    -- CREATED, STARTED, PROGRESS, COMPLETED, FAILED
  payload_json TEXT,           -- JSON metadata (progress, error, etc.)
  occurred_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### 4.3 Derived View (Read-Only Convenience)

```sql
CREATE VIEW cf_jobs_current AS
SELECT
  job_id,
  arg_max(event_type, occurred_at) AS status,
  arg_max(payload_json, occurred_at) AS details,
  min(occurred_at) AS created_at,
  max(occurred_at) AS updated_at
FROM cf_job_events
GROUP BY job_id;
```

This view is **not** used for job claiming.

### 4.4 Columnar-Friendly Write Strategy (Proposal)

DuckDB is columnar and performs best with append-only inserts and scans, not
frequent random updates. To reconcile the "single DB" constraint with DuckDB's
strengths:

- Use the append-only `cf_job_events` table for auditability and most state
  transitions (bulk INSERTs, read via `cf_jobs_current` view).
- Keep the mutable queue (`cf_processing_queue`) narrowly scoped to atomic job
  claiming and minimal status changes only.
- Prefer batch INSERTs for event writes; avoid wide UPDATEs on hot tables.

This keeps the control-plane mostly append-only while preserving the required
atomic claiming semantics.

---

## 5. Transaction + Batch Requirements

`casparian_db` must support:
- Explicit transactions
- Batch inserts for job events
- Bulk inserts for snapshots and queue entries

---

## 6. Retention and Housekeeping

Events are append-only and can grow indefinitely. Add retention policy:
- Keep all events for active jobs.
- For completed jobs, archive or truncate after N days.
- Optional export to Parquet for compliance.

---

## 7. Migration Plan (Phased)

### Phase 1: Consolidate on `casparian_db`
- Remove `create_pool()` usage in application layers.
- Ensure all DB access uses `DbConnection`.

### Phase 2: Sentinel Refactor
- Replace pool usage with `DbConnection::open_duckdb()`.
- Port job queue SQL calls to `DbConnection` methods.

### Phase 3: Storage Layer Refactor
- Replace `storage/sqlite.rs` with `storage/db.rs` backed by `DbConnection` (DuckDB-first).
- Centralize schema creation/migrations in one place.

### Phase 4: Scout Integration
- Migrate scout DB wrapper to `DbConnection` or add adapter.

### Phase 5: Job Events
- Add `cf_job_events` and batch insert path in Sentinel.
- Update TUI to read events for audit detail.

---

## 8. Open Questions

- Should event payload be JSON or structured columns for hot fields?
- How aggressive should event retention be by vertical?
- Do we need separate read-only connections for TUI in DuckDB mode?
