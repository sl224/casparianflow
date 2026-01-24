# Jobs Progress - Per-Job Live Updates Spec

**Status:** Draft
**Owner:** TUI + Worker Platform
**Last Updated:** 2026-01-21
**Related:** `specs/views/jobs.md`, `specs/tui.md`, `STRATEGY.md`, `docs/decisions/ADR-021-ai-agentic-iteration-workflow.md`

---

## 1. Problem Statement

Today, users start a scan or parse and do not see live progress in the Jobs view.
This breaks a core workflow across target verticals: "Is my data ready, if not,
what is happening and why?" The TUI has progress data for some workflows (e.g.,
scan progress events), but that data does not reliably flow into Jobs state.
This is especially problematic when the UI cannot depend on in-process channels
to observe progress (e.g., worker restarts or separate processes).

We need a durable, per-job progress pipeline that:
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
- Make Jobs view updates reliable across worker restarts and separate processes.
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
- Cross-process compatibility.
- Crash/restart durability.
- Auditability of progress history.

**Database constraints (explicit):**
- **SQLite** and **DuckDB** are embedded, local-first databases. They are suitable
  for **single-node** deployments only (including multiple processes on the same
  host with WAL + throttling).
- **Distributed workers** across hosts require a client/server
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

### Option A: Extend `cf_processing_queue` (Recommended for v1)
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

### Option B: New `cf_job_progress` Table (Optional, future)
Schema:
- `job_id` (FK)
- `unit`, `done`, `total`, `throughput`, `eta_seconds`, `payload_json`, `updated_at`

Pros: separation of concerns, reduces lock contention.
Cons: extra join and more query logic.

Recommendation: **Option A** for v1; revisit Option B if write contention appears.

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

Progress is optional; clients that don't render it can ignore it.

---

## 10. Operational Considerations

- Update frequency: 1-2s is sufficient for TUI; avoid per-item updates.
- Size limit: payload must be bounded (<1-4KB recommended).
- Offline: DB updates continue locally; no network required.
- Per-job: updates are keyed by `job_id` only; UI reads the latest row.
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
- Jobs view remains consistent after worker restarts.
- Stalled jobs are visually distinct within 30s.

---

## 13. AI Agentic Backtest Progress (NEW)

**Added for AI agent iteration support.** See `docs/decisions/ADR-021-ai-agentic-iteration-workflow.md`.

AI agents iterating on parsers and schemas require richer progress feedback than
human users. This section specifies the extended progress format for agentic loops.

### 13.1 Extended Progress Event Schema

For AI backtest runs, the progress envelope is extended with phase-aware metrics:

```rust
/// Extended progress for AI backtest iteration.
pub struct AiBacktestProgress {
    /// Standard envelope
    pub envelope: JobProgressEnvelope,

    /// Current processing phase
    pub phase: BacktestPhase,

    /// Phase-specific metrics
    pub phase_metrics: PhaseMetrics,

    /// Cumulative quality metrics
    pub quality: QualityMetrics,
}

pub enum BacktestPhase {
    /// Discovering files to process
    Discovery,
    /// Parsing files
    Parsing,
    /// Validating against schema
    Validation,
    /// Writing output (if applicable)
    Writing,
    /// Completed
    Done,
}

pub struct PhaseMetrics {
    /// Phase start time
    pub phase_started_at: DateTime<Utc>,

    /// Items processed in this phase
    pub phase_items_done: u64,

    /// Total items expected in this phase (if known)
    pub phase_items_total: Option<u64>,
}

/// Quality metrics for AI learning feedback.
pub struct QualityMetrics {
    /// Files processed so far
    pub files_processed: u64,

    /// Total files to process
    pub files_total: u64,

    /// Total bytes read
    pub bytes_read: u64,

    /// Rows emitted to valid output
    pub rows_emitted: u64,

    /// Rows sent to quarantine
    pub rows_quarantined: u64,

    /// Quarantine percentage (0.0 to 100.0)
    pub quarantine_pct: f64,

    /// Throughput in rows per second
    pub throughput_rows_per_sec: f64,

    /// Elapsed time in milliseconds
    pub elapsed_ms: u64,

    /// Estimated time remaining in milliseconds (if calculable)
    pub eta_ms: Option<u64>,

    /// Violation summary by type (for early feedback)
    pub violation_summary: Vec<ViolationSummaryEntry>,
}

pub struct ViolationSummaryEntry {
    /// Column name
    pub column: String,

    /// Violation type
    pub violation_type: ViolationType,

    /// Count of violations
    pub count: u64,

    /// Percentage of total rows
    pub pct: f64,
}
```

### 13.2 Progress Event JSON Format

Progress events are emitted as NDJSON (newline-delimited JSON) for streaming consumption:

```json
{
  "job_id": "abc123",
  "phase": "validation",
  "files_processed": 3,
  "files_total": 5,
  "bytes_read": 123456789,
  "rows_emitted": 150000,
  "rows_quarantined": 3450,
  "quarantine_pct": 2.3,
  "throughput_rows_per_sec": 3200,
  "elapsed_ms": 45000,
  "eta_ms": 105000,
  "violation_summary": [
    {"column": "price", "violation_type": "type_mismatch", "count": 2100, "pct": 1.4},
    {"column": "timestamp", "violation_type": "timezone_required", "count": 1350, "pct": 0.9}
  ],
  "updated_at": "2026-01-21T10:30:45.123Z"
}
```

### 13.3 Emit Frequency

Progress events should be emitted at **stable intervals** to balance responsiveness
with overhead:

| Condition | Emit Frequency |
|-----------|---------------|
| Time-based | Every 1 second |
| Row-based | Every 50,000 rows |
| Phase transition | Immediately |
| Completion/failure | Immediately |

Emit on whichever condition is met first. Keep event generation cheap: counters
only in the hot path, no heavy sampling.

### 13.4 Transport Options

#### ZMQ PubSub (Live Agent Feedback)

For live agent loops, progress events are published to a ZMQ PUB socket:

```
Topic: ai.backtest.progress.{job_id}
Message: NDJSON event
```

The agent subscribes to the topic and processes events as they arrive.

#### DuckDB Table (Polling + Postmortem)

For CLI/TUI polling and post-run analysis, events are persisted:

```sql
CREATE TABLE IF NOT EXISTS cf_ai_backtest_progress (
    id INTEGER PRIMARY KEY,
    job_id TEXT NOT NULL,
    phase TEXT NOT NULL,
    files_processed INTEGER NOT NULL,
    files_total INTEGER,
    bytes_read INTEGER NOT NULL,
    rows_emitted INTEGER NOT NULL,
    rows_quarantined INTEGER NOT NULL,
    quarantine_pct REAL NOT NULL,
    throughput_rows_per_sec REAL,
    elapsed_ms INTEGER NOT NULL,
    eta_ms INTEGER,
    violation_summary_json TEXT,  -- JSON array
    updated_at TEXT NOT NULL,
    UNIQUE(job_id, updated_at)
);

-- Index for job queries
CREATE INDEX idx_ai_progress_job ON cf_ai_backtest_progress(job_id);
```

### 13.5 CLI Integration

```bash
# Run AI backtest with progress streaming
casparian ai backtest parser.py --input-dir ./samples/ --follow

# Output (streaming to stderr):
# [00:05] discovery: 10/10 files (100%)
# [00:12] parsing: 3/10 files, 150k rows emitted, 3.2k rows/s
# [00:18] validation: 5/10 files, 2.3% quarantine
# [00:25] validation: 8/10 files, 2.1% quarantine (improving)
# [00:30] done: 10/10 files, 500k rows, 1.8% quarantine

# Non-streaming mode (poll-based)
casparian ai backtest parser.py --input-dir ./samples/
# Shows final summary only
```

### 13.6 Progress Monotonicity

Progress metrics MUST be **monotonic** within a phase:

- `files_processed`: always increases or stays same.
- `rows_emitted`: always increases or stays same.
- `elapsed_ms`: always increases.

This allows agents to reason about "stuck vs slow":
- If `elapsed_ms` increases but `rows_emitted` is constant for >30s → stuck.
- If `rows_emitted` increases slowly but steadily → slow (IO bound).

### 13.7 Stall Detection

For AI agents, stall detection is critical to avoid infinite waits:

```rust
const STALL_THRESHOLD_MS: u64 = 30_000;  // 30 seconds

fn is_stalled(current: &AiBacktestProgress, previous: &AiBacktestProgress) -> bool {
    let time_delta = current.quality.elapsed_ms - previous.quality.elapsed_ms;
    let progress_delta = current.quality.rows_emitted - previous.quality.rows_emitted;

    time_delta >= STALL_THRESHOLD_MS && progress_delta == 0
}
```

If stalled, the agent should:
1. Check for errors in the job status.
2. Consider timeout and retry with modified parameters.
3. Log the stall for debugging.

---

## 14. Open Questions

- Confirm Option A columns vs Option B table for standard progress.
- For distributed deployments, what is the target DB (Postgres vs other)?
- Maximum allowed payload size for heartbeat?
- For AI progress: should violation_summary be capped (e.g., top 5 columns)?
- For AI progress: should we add parser-level errors (syntax, import) to progress?
