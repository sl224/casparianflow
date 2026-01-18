# Jobs Progress - Multi-Worker Live Updates Spec

**Status:** Draft
**Owner:** TUI + Worker Platform
**Last Updated:** 2026-01-18
**Related:** `specs/views/jobs.md`, `specs/tui.md`, `STRATEGY.md`

---

## 1. Problem Statement

Today, users start a scan or parse and do not see live progress in the Jobs view.
This breaks a core workflow across target verticals: "Is my data ready, if not,
what is happening and why?" The TUI has progress data for some workflows (e.g.,
scan progress events), but that data does not reliably flow into Jobs state.
This is especially problematic for multi-worker deployments, where the UI cannot
depend on in-process channels to observe progress.

We need a durable, cross-worker progress pipeline that:
- Shows live progress for all job types in the Jobs view.
- Survives process restarts and supports multiple worker nodes.
- Preserves local-first/offline operation.

---

## 2. Product/Customer Fit Context

From `STRATEGY.md` and vertical strategy docs, the product promise is:
"Query your dark data locally with full audit trails." Users care about:
- Readiness: Is output ready to query?
- Trust: Schema contract status, parser version, audit trail.
- Actionability: Why did it fail and what should I do next?
- Predictability: ETA and throughput for large workloads.

Regulated and offline environments (finance, healthcare, defense) require:
durable progress reporting and clear audit trails without cloud dependency.
Live progress is not a nice-to-have; it is required to reduce operational risk.

---

## 3. Goals

- Provide live, structured progress for Scan, Parse, Backtest, Schema Eval.
- Make Jobs view updates reliable in single-node and multi-worker mode.
- Keep worker heartbeats lightweight and bounded in size.
- Persist progress in the database for durability and audit.
- Preserve existing CLI/TUI behavior; no regressions in local-only mode.

---

## 4. Non-Goals

- Do not add a real-time streaming UI layer (no WebSocket requirement).
- Do not require new infrastructure beyond the existing DB.
- Do not refactor all worker task scheduling in this iteration.

---

## 5. Proposed Design

### 5.1 Progress Envelope (Structured, Bounded)

All job progress is represented as a small, fixed "envelope" plus an optional
JSON payload for job-specific details.

```
JobProgressEnvelope:
  items_done: u64
  items_total: Option<u64>
  unit: ProgressUnit            # Files, Rows, Bytes, Records, Chunks, Unknown
  throughput_per_sec: Option<f64>
  eta_seconds: Option<u64>
  updated_at: DateTime
  payload: JSON (optional)      # bounded; small key/value details
```

Guideline: `payload` is optional and must be small (no large strings or lists).
Use it for secondary details (schema_ok, current_path, parse_mode) only when needed.

### 5.2 Transport: Heartbeat + Progress Updates

We preserve a lightweight heartbeat and optionally piggyback a small progress
envelope if message size remains bounded. If/when the payload grows, workers
send a separate ProgressUpdate message.

Recommended policy:
- Heartbeat contains only the envelope (no large strings).
- Rich details (current file path, warnings) are log data, not heartbeat data.

### 5.3 Persistence: Database as Source of Truth

The coordinator (or worker host) writes progress into the DB on each update:
- Table: `cf_processing_queue` (extend) or `cf_job_progress` (new).
- Jobs view polls DB and renders progress from the envelope.

DB persistence enables:
- Multi-worker compatibility.
- Crash/restart durability.
- Auditability of progress history.

**Database constraints (explicit):**
- **SQLite** and **DuckDB** are embedded, local-first databases. They are suitable
  for **single-node** deployments only (including multiple processes on the same
  host with WAL + throttling).
- **Distributed multi-worker** deployments across hosts require a client/server
  database (e.g., Postgres) or a coordinator service to serialize updates.

---

## 6. UI Behavior

### Jobs View
- Actionable/Ready layout (per `specs/views/jobs.md`).
- Progress line uses envelope fields:
  - Scan: `files_done/files_total`, throughput, ETA.
  - Parse: `rows_done/rows_total` or `bytes_done` depending on unit.
  - Backtest: iterations done and pass rate.
  - Schema Eval: paths analyzed.

### Detail Panel
- Shows envelope plus any payload fields (if present).
- Continues to show output path, schema status, parser version, and failure detail.

---

## 7. Data Model Options

### Option A: Extend `cf_processing_queue` (Not Recommended for Multi-Worker)
Add columns:
- `progress_unit` (TEXT)
- `progress_done` (INTEGER)
- `progress_total` (INTEGER)
- `progress_throughput` (REAL)
- `progress_eta_seconds` (INTEGER)
- `progress_payload_json` (TEXT)
- `progress_updated_at` (TEXT)

Pros: fewer joins, existing Jobs query unchanged.
Cons: table grows and risks lock contention under frequent writes.

### Option B: New `cf_job_progress` Table (Recommended)
Schema:
- `job_id` (FK)
- `worker_id` (TEXT)    # Supports N workers per job
- `unit`, `done`, `total`, `throughput`, `eta_seconds`, `payload_json`, `updated_at`

Pros: separation of concerns, reduces lock contention, enables aggregation.
Cons: extra join and more query logic.

Recommendation: **Option B** for safety under multi-worker updates.

---

## 8. Deployment Matrix

| Deployment | DB | Progress Updates | Notes |
|------------|----|------------------|-------|
| Single-node (local-first) | SQLite or DuckDB | Worker writes directly (throttled) | WAL recommended |
| Multi-process same host | SQLite (WAL) | Worker writes directly | Throttle to avoid contention |
| Distributed workers | Postgres (or coordinator service) | Worker or coordinator writes | SQLite/DuckDB not supported |

---

## 9. Wire Protocol Changes

Add optional progress to heartbeat:
```
Heartbeat {
  job_id,
  status,
  progress: Option<JobProgressEnvelope>,
  updated_at,
}
```

If the progress payload must be larger, add:
```
ProgressUpdate { job_id, progress: JobProgressEnvelope }
```

Backward compatibility: progress is optional and ignored by older clients.

---

## 10. Operational Considerations

- Update frequency: 1-2s is sufficient for TUI; avoid per-item updates.
- Size limit: payload must be bounded (<1-4KB recommended).
- Offline: DB updates continue locally; no network required.
- Multi-worker: updates are keyed by `(job_id, worker_id)`; UI aggregates `SUM(done)` and `MAX(updated_at)`.
- Liveness: if `(now - updated_at) > 30s`, render progress as **stalled**.

---

## 11. Risks and Mitigations

- **Heartbeat bloat**: Keep payload small; move rich details to logs.
- **DB write load**: Throttle updates (e.g., every 1s or on material change).
- **Inconsistent UI**: Use `updated_at` for stale indicators.
- **Payload misuse**: Payload is metrics-only; no logs or large strings allowed.

---

## 12. Success Metrics

- Jobs view shows live progress for scan and parse within 1-2s.
- TUI remains responsive under active scans with 10k+ files.
- Multi-worker Jobs view remains consistent after worker restarts.
- Stalled workers are visually distinct within 30s.

---

## 13. Open Questions

- Confirm Option B schema and aggregation queries.
- For distributed deployments, what is the target DB (Postgres vs other)?
- Maximum allowed payload size for heartbeat?
