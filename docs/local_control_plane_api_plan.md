# Local Control Plane API (Pre-v1) - Evaluation + Execution Plan

This doc evaluates the proposal and outlines a concrete, pre-v1 execution plan that fits the repo constraints (no migrations, no backward-compat, no API versioning).

## Evaluation (Validity, Merit, Gaps)

### Merit / What is strong
- Clear primitives: Jobs, Events, Manifests, Datasets, Quarantine summaries map well to Sentinel/Worker roles.
- Job-first API avoids streaming deadlocks for MCP and keeps integrations stable.
- Local-only HTTP with per-session token and output budgets is sensible for local-first security.
- Redaction policy and bounded samples keep agent integrations safer by default.
- Cross-platform transport + discovery file is pragmatic and simple to implement.
- Event model (progress, violations, output materialization) is exactly the right surface for UI/agent consumption.

### Gaps / Adjustments needed
- API versioning conflicts with pre-v1 rule: remove `/v0.1` base path. Keep protocol version in headers and `/version` payload only.
- Connection file permissions: must be written atomically and with restrictive permissions on Unix; Windows handling should be best-effort but documented.
- Event ordering: needs a single source of truth for monotonic `event_id` per job to avoid out-of-order event streams.
- Read-only query endpoint: enforce read-only at the DuckDB connection level AND with a SQL allowlist to reduce risk.
- Output budgets: must apply to both query responses and event payloads; violations must be sampled/aggregated on the server.
- Typo/ambiguity: approval endpoint path should be `/approvals/{approval_id}/decide` (spec contains a stray non-ASCII token).
- Retention: define and enforce a TTL for job/events; a cleanup task should be part of Sentinel startup.
- Pre-v1 data handling: schema changes must drop and recreate tables or delete `~/.casparian_flow/casparian_flow.duckdb`.

## Execution Plan (Pre-v1, No Versioned Paths)

### Phase 1: Protocol types (casparian_protocol)
1. Define core structs in `crates/casparian_protocol`:
   - `Job`, `JobStatus`, `JobType`, `JobSpec`, `JobResult`, `JobError`
   - `Event`, `EventType`, `ProgressPayload`, `ViolationPayload`, `ApprovalPayload`, `OutputPayload`
   - `RedactionPolicy`, `QueryRequest`, `QueryResponse`
2. Provide serde tags and strict enums to avoid stringly-typed usage.
3. Add helper builders for event creation to avoid illegal combinations.

### Phase 2: Sentinel storage + DDL
1. Add new DuckDB tables for jobs, events, approvals, and (optionally) manifests/datasets:
   - `cf_jobs`, `cf_job_events`, `cf_approvals`, `cf_manifests`, `cf_datasets`
2. Ensure table creation is destructive on schema changes:
   - If columns change, drop and recreate or delete `~/.casparian_flow/casparian_flow.duckdb` (per pre-v1 rules).
3. Add an internal `next_event_id(job_id)` function to guarantee monotonic per-job IDs.

### Phase 3: Sentinel API server
1. Add `sentinel_api` module in `crates/casparian_sentinel` using `axum`.
2. Bind to loopback only, choose ephemeral port, write `~/.casparian_flow/control_plane.json` atomically.
3. Middleware:
   - Require `Authorization: Bearer <token>` on all endpoints except `/health` and `/version`.
   - Enforce response size budgets and redaction defaults.
4. Routes (no versioned base path):
   - `GET /health`, `GET /version`
   - `POST /jobs`, `GET /jobs`, `GET /jobs/{job_id}`, `POST /jobs/{job_id}/cancel`
   - `GET /jobs/{job_id}/events`
   - `GET /approvals`, `GET /approvals/{approval_id}`, `POST /approvals/{approval_id}/decide`
   - `POST /query` (read-only)
   - `GET /datasets`, `GET /manifests/{manifest_id}`, `GET /quarantine/summary`

### Phase 4: Worker integration + event emission
1. Add event emission hooks in Worker -> Sentinel (via existing ZMQ or direct channel).
2. Emit:
   - `job_started`, `phase`, periodic `progress`, `violation`, `output`, `job_finished`.
3. Create aggregation for violations to keep event payloads bounded.
4. Ensure cancellation is respected and reflected in job status.

### Phase 5: Redaction + output budgets
1. Implement a redaction policy module:
   - default `hash` for strings, `max_values`, `max_len`.
2. Apply policy to all sample/preview outputs.
3. Enforce hard response budgets; return `413` on overflow.

### Phase 6: Query endpoint hardening
1. Open DuckDB in read-only mode for `/query`.
2. Also parse/allowlist `SELECT`, `WITH`, `EXPLAIN` and block `ATTACH`, `COPY`, `INSERT`, etc.
3. Apply `max_rows` + `max_bytes` limits and redaction.

### Phase 7: Tests + manual validation
1. Unit tests:
   - auth required, loopback only, invalid token => 401.
   - redaction defaults and output budgets enforced.
2. Integration tests:
   - create job, observe events, finish job.
   - cancel job, status transitions.
   - query endpoint rejects write queries.
3. Manual smoke:
   - start Sentinel, confirm `control_plane.json` discovery file, hit `/health`.

### Phase 8: Documentation
1. Update `docs/` with API reference and local discovery file format.
2. Add example CLI/MCP calls for job polling (bounded long-poll).

## Implementation Notes (Pre-v1 compliance)
- No versioned routes (`/v0.1`); use `X-Casparian-Protocol: 0.1` header + `/version`.
- No migrations: drop/recreate tables or delete the DuckDB file when schema changes.
- No compatibility shims; update all call sites when API changes.

