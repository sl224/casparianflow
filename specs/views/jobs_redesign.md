# Jobs View - Specification

**Status:** Ready for Implementation
**Version:** 2.0
**Last Updated:** January 2026
**Refined via:** spec_refinement_workflow_v2 (2 rounds)

---

## 1. Overview

The Jobs view shows processing activity with three purposes:
1. **Find problems fast** - Failed jobs surface immediately
2. **Know where output is** - Paths are always visible
3. **Monitor throughput** - Live metrics via dedicated panel

---

## 2. User Questions This View Answers

| Question | Answer Location |
|----------|-----------------|
| "What failed?" | Failed jobs pinned to top with error |
| "Where's my output?" | Output path on every completed job |
| "How far along?" | Progress bar on running jobs |
| "How much is done?" | Status bar + Pipeline summary |
| "How fast is it going?" | Monitoring panel (rows/sec, sink stats) |
| "Is my parser ready?" | Backtest job shows pass rate |

---

## 3. Layout

### 3.1 Default View (Pipeline Collapsed)

```
┌─ JOBS ─────────────────────────────────────────────────────────────────────────┐
│                                                                                │
│  ↻ 2 running   ✓ 3 done   ✗ 1 failed     1,235/1,247 files • 847 MB output    │
│                                                                                │
│  ══════════════════════════════════════════════════════════════════════════════ │
│                                                                                │
│▸ ✗ PARSE    fix_parser v1.2                                        2m ago     │
│             12 files failed • SchemaViolation at row 42                       │
│             First failure: venue_nyse_20240115.log                            │
│                                                                                │
│  ↻ BACKTEST fix_parser v1.3 (iter 3)                   ████████░░░░  87%     │
│             Pass: 108/124 files • 5 high-failure passed                       │
│                                                                                │
│  ↻ EXPORT   concordance → ./production_001/          ████████░░░░  67%       │
│             30,521/45,782 records • ETA 6m                                    │
│                                                                                │
│  ✓ PARSE    fix_parser v1.2                                        2m ago     │
│             1,235 files → ~/.casparian_flow/output/fix_orders/ (847 MB)      │
│                                                                                │
│  ✓ SCAN     /data/fix_logs • 1,247 files                          15m ago     │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│  [j/k] Navigate  [Enter] Details  [P] Pipeline  [m] Monitor  [?] Help         │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 With Pipeline Summary (Toggle `P`)

```
┌─ JOBS ─────────────────────────────────────────────────────────────────────────┐
│                                                                                │
│  ┌─ PIPELINE ────────────────────────────────────────────────────────────────┐ │
│  │   SOURCE              PARSED               OUTPUT                         │ │
│  │   ┌────────┐         ┌────────┐          ┌────────┐                       │ │
│  │   │ 1,247  │  ────▶  │ 1,235  │  ────▶   │ 2 ready│                       │ │
│  │   │ files  │   @12   │ files  │    @1    │ 1 run  │                       │ │
│  │   └────────┘         └────────┘          └────────┘                       │ │
│  │   fix_parser v1.2: 1,235 processed • 847 MB output                        │ │
│  └───────────────────────────────────────────────────────────────────────────┘ │
│                                                                                │
│  ↻ 2 running   ✓ 3 done   ✗ 1 failed                                          │
│  ══════════════════════════════════════════════════════════════════════════════ │
│                                                                                │
│▸ ✗ PARSE    fix_parser v1.2                                        2m ago     │
│             ...                                                                │
```

---

## 4. Job Type Rendering

### 4.1 Scan Job

```
✓ SCAN     /data/fix_logs • 1,247 files                          15m ago
```

One line. Path scanned, file count, timestamp.

### 4.2 Parse Job (Complete)

```
✓ PARSE    fix_parser v1.2                                        2m ago
           1,235 files → ~/.casparian_flow/output/fix_orders/ (847 MB)
```

Two lines. Parser name/version, file count, **output path**, size.

### 4.3 Parse Job (Failed)

```
✗ PARSE    fix_parser v1.2                                        2m ago
           12 files failed • SchemaViolation at row 42
           First failure: venue_nyse_20240115.log
```

Three lines. Error summary, **first failing file**.

### 4.4 Parse Job (Running)

```
↻ PARSE    fix_parser v1.2                            ████████░░░░  67%
           892/1,247 files • ETA 4m
```

### 4.5 Export Job (Complete)

```
✓ EXPORT   concordance → ./production_001/ (156 MB)               5m ago
           45,782 records • Bates SMITH000001-045782
```

### 4.6 Export Job (Running)

```
↻ EXPORT   concordance → ./production_001/          ████████░░░░  67%
           30,521/45,782 records • ETA 6m
```

### 4.7 Backtest Job (Running)

```
↻ BACKTEST fix_parser v1.3 (iter 3)                   ████████░░░░  87%
           Pass: 108/124 files • 5 high-failure passed
```

Two lines. Iteration number, pass rate as progress, high-failure status.

### 4.8 Backtest Job (Complete)

```
✓ BACKTEST fix_parser v1.3                                        1h ago
           Pass rate: 99.2% (496/500) • All high-failure resolved
```

### 4.9 Backtest Job (Failed/Stopped)

```
✗ BACKTEST broken_parser v0.1                                    30m ago
           Pass rate: 23% (12/52) • Early stop: high-failure failing
           First failure: corrupt_file_001.csv
```

---

## 5. Monitoring Panel

Press `m` to open monitoring panel (sub-state of Jobs view).

```
┌─ MONITORING ───────────────────────────────────────────────────────────────────┐
│                                                                                │
│  QUEUE                              THROUGHPUT (5m)                            │
│  ┌─────────────────────────────┐    ┌─────────────────────────────────────┐    │
│  │  Pending:    45             │    │         ▄                           │    │
│  │  Running:     3             │    │     ▃▅ ▆█    ▄                      │    │
│  │  Done:    1,247             │    │    ▅███ ██  ███                     │    │
│  │  Failed:     12             │    │   ████████ █████ ▃▄                 │    │
│  │                             │    │  ███████████████████ ▅▆             │    │
│  │  Depth: ▁▂▃▄█████▆▄▃▂       │    │ ███████████████████████▆▇           │    │
│  └─────────────────────────────┘    │ 2.4k rows/s avg      3.1k now ▲     │    │
│                                     └─────────────────────────────────────┘    │
│                                                                                │
│  SINKS                                                                         │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │  parquet://output/                    847 MB total    45 errors         │   │
│  │    └─ fix_orders                      623 MB   1,847,234 rows           │   │
│  │    └─ venue_data                      224 MB     523,891 rows           │   │
│  │                                                                          │   │
│  │  sqlite:///data.db                    1.2 GB total     0 errors         │   │
│  │    └─ transactions                    45,782 rows                       │   │
│  │                                                                          │   │
│  │  Write latency: 12ms (p50)  45ms (p99)                                  │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│  [Esc] Back  [p] Pause updates  [r] Reset stats                               │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 5.1 Monitoring Metrics

| Panel | Metrics Shown |
|-------|---------------|
| Queue | Pending/Running/Done/Failed counts, queue depth sparkline |
| Throughput | Rows/sec over 5 minutes, current vs average |
| Sinks | Per-sink totals (rows, bytes, errors), write latency |

---

## 6. State Machine

```
                         JOB_LIST
                        (default)
                            │
         ┌──────────┬───────┼───────┬──────────┬──────────┐
         │          │       │       │          │          │
     Enter│       'l'│    'f'│    'm'│       'P'│          │
         ▼          ▼       ▼       ▼          │          │
    ┌─────────┐ ┌───────┐ ┌──────┐ ┌─────────┐ │          │
    │ DETAIL  │ │ LOGS  │ │FILTER│ │MONITORING│ │          │
    │ PANEL   │ │VIEWER │ │DIALOG│ │ PANEL   │ │          │
    └────┬────┘ └───┬───┘ └──┬───┘ └────┬────┘ │          │
         │          │        │          │      │          │
       Esc│       Esc│   Esc/Enter    Esc│     │(toggle)  │
         │          │        │          │      │          │
         └──────────┴────────┴──────────┴──────┘          │
                             │                             │
                             ▼                             │
                         JOB_LIST ◄────────────────────────┘
                    (show_pipeline toggled)
```

**States:**

| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| JOB_LIST | Default | - | Browse jobs, j/k navigate |
| DETAIL_PANEL | Enter | Esc | Show full job details |
| LOG_VIEWER | l | Esc | Full-screen logs |
| FILTER_DIALOG | f | Esc/Enter | Filter by type/status |
| MONITORING_PANEL | m | Esc | Live metrics dashboard |

`P` toggles `show_pipeline` flag without changing state.

---

## 7. Keybindings

### 7.1 Job List

| Key | Action | When Available |
|-----|--------|----------------|
| `j` / `↓` | Next job | Always |
| `k` / `↑` | Previous job | Always |
| `g` | Go to first | Always |
| `G` | Go to last | Always |
| `Enter` | Detail panel | Always |
| `l` | View logs | Always |
| `R` | Retry failed | Failed jobs |
| `c` | Cancel job | Running jobs |
| `S` | Stop backtest | Running backtest (with confirm) |
| `y` | Copy output path | Jobs with output |
| `o` | Open folder | Completed exports |
| `f` | Filter dialog | Always |
| `P` | Toggle pipeline | Always |
| `m` | Monitoring panel | Always |
| `x` | Clear completed | When completed exist |
| `?` | Help | Always |

### 7.2 Monitoring Panel

| Key | Action |
|-----|--------|
| `Esc` | Return to job list |
| `p` | Pause/resume updates |
| `r` | Reset statistics |
| `Tab` | Cycle focus between panels |

---

## 8. Data Model

### 8.1 Core Types

```rust
pub struct JobInfo {
    pub id: i64,
    pub file_version_id: Option<i64>,    // For parse jobs; None for scan/backtest
    pub job_type: JobType,
    pub name: String,                     // parser/exporter/source name
    pub version: Option<String>,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,        // Maps to claim_time in DB
    pub completed_at: Option<DateTime<Utc>>,

    // Progress
    pub items_total: u32,
    pub items_processed: u32,
    pub items_failed: u32,

    // Output
    pub output_path: Option<PathBuf>,
    pub output_size_bytes: Option<u64>,

    // Backtest-specific (None for other types)
    pub backtest: Option<BacktestInfo>,

    // Errors
    pub failures: Vec<JobFailure>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobType {
    Scan,
    Parse,
    Export,
    Backtest,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Complete,
    Failed,
    Cancelled,
}

pub struct BacktestInfo {
    pub pass_rate: f64,                   // 0.0 - 1.0
    pub iteration: u32,
    pub high_failure_tested: u32,
    pub high_failure_passed: u32,
    pub termination_reason: Option<TerminationReason>,
}

#[derive(Clone, Copy)]
pub enum TerminationReason {
    PassRateAchieved,
    MaxIterations,
    PlateauDetected,
    HighFailureEarlyStop,
    UserStopped,
}

pub struct JobFailure {
    pub file_path: PathBuf,
    pub error: String,
    pub line: Option<u32>,
}
```

### 8.2 Monitoring Types

```rust
pub struct MonitoringState {
    pub queue: QueueStats,
    pub throughput_history: VecDeque<ThroughputSample>,  // Last 5 min
    pub sinks: Vec<SinkStats>,
    pub paused: bool,
}

pub struct QueueStats {
    pub pending: u32,
    pub running: u32,
    pub completed: u32,
    pub failed: u32,
    pub depth_history: VecDeque<u32>,  // For sparkline
}

pub struct ThroughputSample {
    pub timestamp: DateTime<Utc>,
    pub rows_per_second: f64,
}

pub struct SinkStats {
    pub uri: String,
    pub total_rows: u64,
    pub total_bytes: u64,
    pub error_count: u32,
    pub latency_p50_ms: u32,
    pub latency_p99_ms: u32,
    pub outputs: Vec<SinkOutput>,
}

pub struct SinkOutput {
    pub name: String,
    pub rows: u64,
    pub bytes: u64,
}
```

### 8.3 Pipeline Types

```rust
pub struct PipelineState {
    pub source: PipelineStage,
    pub parsed: PipelineStage,
    pub output: PipelineStage,
    pub active_parser: Option<String>,
}

pub struct PipelineStage {
    pub count: u32,
    pub in_progress: u32,
}
```

---

## 9. Data Flow: Workers to TUI

### 9.1 Architecture

TUI polls SQLite database. Workers write progress updates.

```
Workers ──(write)──▶ SQLite ◀──(poll 500ms)── TUI
```

**Why polling over events?**
- Matches existing architecture (Discover polls scout_files)
- TUI survives restarts (state in DB, not memory)
- No new IPC complexity

### 9.2 Database Schema Extensions

```sql
-- Extend cf_processing_queue
ALTER TABLE cf_processing_queue ADD COLUMN job_type TEXT DEFAULT 'PARSE';
ALTER TABLE cf_processing_queue ADD COLUMN progress_pct INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN items_processed INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN items_total INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN backtest_data TEXT;  -- JSON
ALTER TABLE cf_processing_queue ADD COLUMN updated_at TEXT;

CREATE INDEX idx_jobs_updated ON cf_processing_queue(updated_at DESC);

-- Monitoring metrics (time-series, auto-cleaned)
CREATE TABLE cf_job_metrics (
    id INTEGER PRIMARY KEY,
    job_id INTEGER NOT NULL,
    metric_time TEXT DEFAULT (datetime('now')),
    rows_per_second REAL,
    bytes_per_second INTEGER,
    queue_depth INTEGER
);

-- Sink statistics
CREATE TABLE cf_sink_stats (
    id INTEGER PRIMARY KEY,
    sink_uri TEXT NOT NULL,
    recorded_at TEXT DEFAULT (datetime('now')),
    total_rows INTEGER DEFAULT 0,
    total_bytes INTEGER DEFAULT 0,
    write_latency_ms INTEGER,
    error_count INTEGER DEFAULT 0
);
```

### 9.3 TUI Polling

```rust
/// Fetch jobs changed since last poll
pub async fn fetch_job_updates(
    pool: &SqlitePool,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<JobInfo>> {
    sqlx::query_as!(
        JobRow,
        r#"
        SELECT id, job_type, plugin_name, status, progress_pct,
               items_processed, items_total, output_path, error_message,
               claim_time as started_at, end_time as completed_at,
               updated_at, backtest_data
        FROM cf_processing_queue
        WHERE updated_at > ?
        ORDER BY
            CASE status WHEN 'FAILED' THEN 0 WHEN 'RUNNING' THEN 1 ELSE 2 END,
            claim_time DESC
        LIMIT 100
        "#,
        since
    )
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(JobInfo::from).collect())
}

/// Clean old metrics (run every poll cycle)
pub async fn cleanup_old_metrics(pool: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM cf_job_metrics WHERE metric_time < datetime('now', '-5 minutes')")
        .execute(pool)
        .await?;
    Ok(())
}
```

### 9.4 DateTime Parsing

SQLite stores `datetime()` as `YYYY-MM-DD HH:MM:SS`. Parse correctly:

```rust
fn parse_sqlite_datetime(s: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
}
```

---

## 10. Implementation Notes

### 10.1 Sorting

```rust
fn sort_jobs(jobs: &mut Vec<JobInfo>) {
    jobs.sort_by(|a, b| {
        // Failed first
        match (a.status == JobStatus::Failed, b.status == JobStatus::Failed) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        // Then by recency
        b.started_at.cmp(&a.started_at)
    });
}
```

### 10.2 Sparkline Rendering

Using Unicode block characters for visual consistency:

```rust
const BLOCKS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

fn render_sparkline(samples: &[f64], width: usize) -> String {
    if samples.is_empty() { return " ".repeat(width); }

    let max = samples.iter().cloned().fold(f64::MIN, f64::max);
    let min = samples.iter().cloned().fold(f64::MAX, f64::min);
    let range = if max == min { 1.0 } else { max - min };

    samples.iter()
        .map(|&v| {
            let idx = ((v - min) / range * 7.0).round() as usize;
            BLOCKS[idx.min(7)]
        })
        .collect()
}
```

### 10.3 Backtest Pass Rate as Progress

For backtest jobs, use pass rate as progress indicator:

```rust
fn render_backtest_progress(info: &BacktestInfo) -> String {
    let percent = (info.pass_rate * 100.0).round() as u8;
    let bar = render_progress_bar(percent);
    format!("{}  {}%", bar, percent)
}
```

---

## 11. Success Criteria

| Goal | Metric |
|------|--------|
| Find failures | Failed jobs visible without scrolling |
| Copy output path | `y` copies in < 1 second |
| Understand progress | Running jobs show %, ETA |
| Monitor throughput | `m` shows live rows/sec, sink stats |
| Track backtest | Pass rate visible, high-failure status clear |
| See pipeline state | `P` toggles summary showing SOURCE→PARSED→OUTPUT |

---

## 12. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 2.0 | Refinement v2: Added Backtest job type, full monitoring panel, pipeline visualization, data flow architecture |
| 2026-01 | 1.0 | Crystallized spec (later deemed over-simplified) |
| 2026-01 | 0.2 | Added monitoring view |
| 2026-01 | 0.1 | Initial draft with 5 alternatives |
