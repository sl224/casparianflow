# Engineer Response: Round 002

**Date:** 2026-01-13
**Engineer:** Claude (Opus 4.5)
**Context:** Incorporating user decisions from Round 1 to revise Jobs view specification

---

## User Decisions Applied

1. **Reopen pipeline/data-state visualization** - v1.0 was over-simplified
2. **Backtest in Jobs view** - Unified view with Scan/Parse/Export
3. **Full monitoring panel** - Sink metrics, queue depth, trends (not just inline throughput)

---

## Gap Resolution: GAP-DATA-001

**CRITICAL: How does job data flow from workers to TUI?**

**Confidence:** HIGH

### Proposed Solution

Use **database polling with change detection**. This matches the existing architectural pattern (see `CLAUDE.md` - "Single Database Rule") and avoids adding new IPC complexity.

#### Data Flow Architecture

```
+-----------------------------------------------------------------------------+
|                               WORKERS                                        |
|   +-----------+  +-----------+  +-----------+                               |
|   |  Worker   |  |  Worker   |  |  Worker   |                               |
|   +-----+-----+  +-----+-----+  +-----+-----+                               |
|         |              |              |                                      |
|         +------+-------+-------+------+                                      |
|                |               |                                             |
|                v               v                                             |
|              +---------------------+                                         |
|              |       SQLite        |  cf_processing_queue                    |
|              |  (Single Source     |  cf_job_metrics (NEW)                   |
|              |    of Truth)        |  cf_sink_stats (NEW)                    |
|              +---------+-----------+                                         |
|                        |                                                     |
|         +--------------+--------------+                                      |
|         v              v              v                                      |
|   +-----------+  +-----------+  +-----------+                               |
|   |    TUI    |  |    CLI    |  |    MCP    |                               |
|   | (polling) |  |  (query)  |  |  (tools)  |                               |
|   +-----------+  +-----------+  +-----------+                               |
+-----------------------------------------------------------------------------+
```

#### TUI Polling Strategy

```rust
/// Configuration for TUI job polling
pub struct JobPollingConfig {
    /// How often to poll for job updates (default: 500ms)
    pub poll_interval: Duration,
    /// How often to refresh metrics (default: 1s)
    pub metrics_interval: Duration,
    /// Only fetch jobs modified after this timestamp
    pub last_seen_timestamp: Option<DateTime<Utc>>,
}

/// TUI polls using this query for efficient updates
pub async fn fetch_job_updates(
    pool: &SqlitePool,
    config: &JobPollingConfig,
) -> Result<JobUpdateBatch> {
    // Use timestamp-based change detection
    let jobs = sqlx::query_as!(
        JobRow,
        r#"
        SELECT id, job_type, plugin_name, status, progress_pct, items_processed,
               items_total, output_path, error_message, started_at, completed_at,
               updated_at
        FROM cf_processing_queue
        WHERE updated_at > ?
        ORDER BY
            CASE status
                WHEN 'FAILED' THEN 0
                WHEN 'RUNNING' THEN 1
                ELSE 2
            END,
            started_at DESC
        LIMIT 100
        "#,
        config.last_seen_timestamp
    )
    .fetch_all(pool)
    .await?;

    Ok(JobUpdateBatch {
        jobs,
        server_timestamp: Utc::now(),
    })
}
```

#### New Database Schema Additions

```sql
-- Extend cf_processing_queue with progress tracking
ALTER TABLE cf_processing_queue ADD COLUMN job_type TEXT DEFAULT 'PARSE';
ALTER TABLE cf_processing_queue ADD COLUMN progress_pct INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN items_processed INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN items_total INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN updated_at TEXT DEFAULT (datetime('now'));

-- Index for efficient polling
CREATE INDEX IF NOT EXISTS idx_cf_processing_queue_updated
ON cf_processing_queue(updated_at DESC);

-- NEW: Job-level metrics (for monitoring panel)
CREATE TABLE IF NOT EXISTS cf_job_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL,
    metric_time TEXT NOT NULL DEFAULT (datetime('now')),
    rows_per_second REAL,
    bytes_per_second INTEGER,
    queue_depth INTEGER,
    FOREIGN KEY (job_id) REFERENCES cf_processing_queue(id)
);

-- Retention: keep only last 5 minutes of metrics per job
CREATE INDEX IF NOT EXISTS idx_cf_job_metrics_time
ON cf_job_metrics(job_id, metric_time DESC);

-- NEW: Sink statistics (for monitoring panel)
CREATE TABLE IF NOT EXISTS cf_sink_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sink_uri TEXT NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
    total_rows INTEGER DEFAULT 0,
    total_bytes INTEGER DEFAULT 0,
    write_latency_ms INTEGER,
    error_count INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_cf_sink_stats_time
ON cf_sink_stats(recorded_at DESC);
```

#### Worker Updates Metrics

Workers write to `cf_job_metrics` during processing:

```rust
// In worker's processing loop (every batch or every N seconds)
pub async fn update_job_metrics(
    pool: &SqlitePool,
    job_id: i64,
    progress_pct: i32,
    items_processed: i32,
    rows_per_second: f64,
    bytes_per_second: i64,
) -> Result<()> {
    // Update progress on main job record
    sqlx::query!(
        r#"
        UPDATE cf_processing_queue
        SET progress_pct = ?, items_processed = ?, updated_at = datetime('now')
        WHERE id = ?
        "#,
        progress_pct,
        items_processed,
        job_id
    )
    .execute(pool)
    .await?;

    // Insert metrics for time-series (monitoring panel)
    sqlx::query!(
        r#"
        INSERT INTO cf_job_metrics (job_id, rows_per_second, bytes_per_second, queue_depth)
        VALUES (?, ?, ?, (SELECT COUNT(*) FROM cf_processing_queue WHERE status = 'QUEUED'))
        "#,
        job_id,
        rows_per_second,
        bytes_per_second
    )
    .execute(pool)
    .await?;

    Ok(())
}
```

### Trade-offs

| Aspect | Benefit | Cost |
|--------|---------|------|
| Database polling | Simple, works with existing architecture | 500ms latency, extra DB queries |
| Timestamp-based change detection | Only fetches changed rows | Requires new `updated_at` column |
| Metrics in SQLite | Self-managing, queryable | Need retention cleanup |
| Single DB | No new dependencies | Potential contention under high load |

### New Gaps Introduced

- **GAP-RETENTION-001**: Need background task to clean up old `cf_job_metrics` rows (keep last 5 minutes)

---

## Gap Resolution: GAP-BACKTEST-001

**Add Backtest as a job type with appropriate rendering**

**Confidence:** HIGH

### Proposed Solution

Extend `JobType` enum and add backtest-specific rendering. Backtest jobs are validation jobs that test a parser against multiple files.

#### Extended Job Type

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JobType {
    Scan,       // File discovery
    Parse,      // Parser execution
    Export,     // Data export
    Backtest,   // Parser validation (NEW)
}

impl JobType {
    pub fn symbol(&self) -> &'static str {
        match self {
            JobType::Scan => "S",
            JobType::Parse => "P",
            JobType::Export => "E",
            JobType::Backtest => "B",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            JobType::Scan => "SCAN",
            JobType::Parse => "PARSE",
            JobType::Export => "EXPORT",
            JobType::Backtest => "BACKTEST",
        }
    }
}
```

#### Extended JobInfo for Backtest

```rust
pub struct JobInfo {
    pub id: Uuid,
    pub job_type: JobType,
    pub name: String,                    // parser name, exporter name, source path
    pub version: Option<String>,         // parser/exporter version
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,

    // Progress (common)
    pub items_total: u32,
    pub items_processed: u32,
    pub items_failed: u32,

    // Output (for Parse/Export)
    pub output_path: Option<PathBuf>,
    pub output_size_bytes: Option<u64>,

    // Backtest-specific fields
    pub backtest_info: Option<BacktestJobInfo>,

    // Errors (for failed jobs)
    pub failures: Vec<JobFailure>,
}

/// Backtest-specific job information
pub struct BacktestJobInfo {
    pub parser_name: String,
    pub parser_version: String,
    pub pass_rate: f64,                    // 0.0 - 1.0
    pub high_failure_tested: u32,          // How many high-failure files tested
    pub high_failure_passed: u32,          // How many of those passed
    pub iteration: u32,                    // Which iteration of backtest loop
    pub termination_reason: Option<TerminationReason>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    PassRateAchieved,     // Hit target (e.g., 95%)
    MaxIterations,        // Reached max iterations
    PlateauDetected,      // No improvement for N iterations
    Timeout,              // Time limit exceeded
    UserStopped,          // Manual cancellation
    HighFailureEarlyStop, // High-failure files still failing
}
```

#### Backtest Job Rendering in Jobs List

```
+----------------------------------------------------------------------------------+
| JOBS                                                                              |
|                                                                                   |
|  Running: 2   Done: 3   Failed: 1        1,235/1,247 files   847 MB output       |
|                                                                                   |
|  ============================================================================    |
|                                                                                   |
|  @ BACKTEST  fix_parser v1.2 (iter 3)                     ########....  67%     |
|              Pass rate: 87% (108/124) * 5 high-failure passed                    |
|                                                                                   |
|  + BACKTEST  venue_parser v2.0                                     1h ago        |
|              Pass rate: 99.2% (496/500) * All high-failure resolved              |
|                                                                                   |
|  X BACKTEST  broken_parser v0.1                                   30m ago        |
|              Pass rate: 23% (12/52) * Early stop: high-failure failing           |
|              First failure: corrupt_file_001.csv                                  |
|                                                                                   |
|  + PARSE     fix_parser v1.2                                        2m ago       |
|              1,235 files -> ~/.casparian_flow/output/fix_orders/ (847 MB)        |
|                                                                                   |
|  + SCAN      /data/fix_logs * 1,247 files                          15m ago       |
|                                                                                   |
+----------------------------------------------------------------------------------+

Legend: @ = running, + = done, X = failed
```

#### Backtest Detail Panel

```
+------------------------------------------------------------+
| BACKTEST DETAILS                                            |
|                                                             |
|  Parser:      fix_parser v1.2                               |
|  Iteration:   3 of 10                                       |
|  Status:      Running                                       |
|  Started:     10:38:15                                      |
|  Duration:    2m 34s                                        |
|                                                             |
|  PASS RATE                                                  |
|  +========================================+                 |
|  |####################..........   87%   |                 |
|  +========================================+                 |
|  108/124 files passed                                       |
|                                                             |
|  HIGH-FAILURE FILES                                         |
|  -------------------                                        |
|  Tested: 8    Passed: 5    Still failing: 3                 |
|                                                             |
|  FAILURE BREAKDOWN                                          |
|  -------------------                                        |
|  TypeMismatch:     9 files                                  |
|  SchemaViolation:  5 files                                  |
|  ParseError:       2 files                                  |
|                                                             |
|  RECENT FAILURES (3)                                        |
|  -------------------                                        |
|  venue_nyse_20240115.log                                    |
|    TypeMismatch: expected Integer at row 42                 |
|  venue_lse_20240115.log                                     |
|    SchemaViolation: column 'price' not found                |
|  ... (1 more)                                               |
|                                                             |
+-------------------------------------------------------------+
|  [S] Stop backtest  [l] Logs  [Esc] Close                   |
+-------------------------------------------------------------+
```

#### Database Schema for Backtest Jobs

```sql
-- Backtest-specific data stored in JSON in new column
ALTER TABLE cf_processing_queue ADD COLUMN backtest_data TEXT;
-- JSON example: {"pass_rate": 0.87, "iteration": 3, "high_failure_tested": 8,
--                "high_failure_passed": 5, "termination_reason": null}
```

### Trade-offs

| Aspect | Benefit | Cost |
|--------|---------|------|
| Unified job list | Single view for all job types | More complex rendering logic |
| Backtest as first-class job | Consistent UX | Need to store backtest state |
| Pass rate visualization | At-a-glance status | Takes vertical space |
| JSON for backtest data | Flexible schema | Slightly harder to query |

### New Gaps Introduced

- None (backtest state already tracked in `casparian_backtest` crate)

---

## Gap Resolution: GAP-MONITOR-001

**Design the full monitoring panel**

**Confidence:** HIGH

### Proposed Solution

Add a dedicated monitoring panel accessible via `M` key, showing real-time metrics including sink stats, queue depth, and throughput trends.

#### Monitoring Panel Layout

```
+-------------------------------------------------------------------------------+
| MONITORING                                                                     |
|                                                                                |
|  QUEUE                              THROUGHPUT (last 5m)                       |
|  +-----------------------------+    +-------------------------------------+    |
|  |  Pending:    45             |    |         #                           |    |
|  |  Running:     3             |    |     ## ##    #                      |    |
|  |  Done:    1,247             |    |    #### ##  ###                     |    |
|  |  Failed:     12             |    |   ######## ##### ##                 |    |
|  |                             |    |  ################ #### #            |    |
|  |  Queue Depth Trend          |    | ####################### ###         |    |
|  |  ______##########____       |    | 2.4k rows/s (avg)      ^ 3.1k now   |    |
|  +-----------------------------+    +-------------------------------------+    |
|                                                                                |
|  ACTIVE WORKERS                     SINKS                                      |
|  +-----------------------------+    +-------------------------------------+    |
|  |  worker-001  @ fix_parser   |    |  parquet://output/   847 MB  45 err |    |
|  |              job #1234      |    |    +- fix_orders     623 MB         |    |
|  |              12m runtime    |    |    +- venue_data     224 MB         |    |
|  |                             |    |                                     |    |
|  |  worker-002  @ venue_parser |    |  sqlite:///data.db  1.2 GB   0 err  |    |
|  |              job #1235      |    |    +- transactions  45,782 rows     |    |
|  |              3m runtime     |    |                                     |    |
|  |                             |    |  Write Latency: 12ms (p50) 45ms (p99)|   |
|  |  worker-003  o idle         |    +-------------------------------------+    |
|  +-----------------------------+                                               |
|                                                                                |
+--------------------------------------------------------------------------------+
|  [Esc] Back  [p] Pause updates  [r] Reset stats  [1-4] Focus panel             |
+--------------------------------------------------------------------------------+

Legend: @ = busy, o = idle
```

#### Monitoring Panel State

```rust
/// State for the monitoring panel
#[derive(Debug, Clone, Default)]
pub struct MonitoringState {
    /// Queue statistics over time
    pub queue_history: VecDeque<QueueSnapshot>,
    /// Throughput samples (rows/sec over time)
    pub throughput_history: VecDeque<ThroughputSample>,
    /// Active worker information
    pub workers: Vec<WorkerInfo>,
    /// Sink statistics
    pub sinks: Vec<SinkStats>,
    /// Whether updates are paused
    pub paused: bool,
    /// Last update timestamp
    pub last_update: DateTime<Utc>,
    /// Which sub-panel is focused (for keyboard navigation)
    pub focused_panel: MonitoringPanel,
}

#[derive(Debug, Clone, Default)]
pub struct QueueSnapshot {
    pub timestamp: DateTime<Utc>,
    pub pending: u32,
    pub running: u32,
    pub completed: u32,
    pub failed: u32,
}

#[derive(Debug, Clone)]
pub struct ThroughputSample {
    pub timestamp: DateTime<Utc>,
    pub rows_per_second: f64,
    pub bytes_per_second: u64,
}

#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub status: WorkerStatus,
    pub current_job_id: Option<i64>,
    pub current_job_name: Option<String>,
    pub runtime: Option<Duration>,
    pub last_heartbeat: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SinkStats {
    pub uri: String,
    pub total_bytes: u64,
    pub total_rows: u64,
    pub error_count: u32,
    pub write_latency_p50_ms: u32,
    pub write_latency_p99_ms: u32,
    /// Nested outputs for this sink (e.g., different topics)
    pub outputs: Vec<SinkOutput>,
}

#[derive(Debug, Clone)]
pub struct SinkOutput {
    pub name: String,
    pub bytes: u64,
    pub rows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MonitoringPanel {
    #[default]
    Queue,
    Throughput,
    Workers,
    Sinks,
}
```

#### Monitoring Panel Keybindings

| Key | Action | Description |
|-----|--------|-------------|
| `Esc` | Close panel | Return to Jobs view |
| `p` | Pause/resume | Toggle real-time updates |
| `r` | Reset stats | Clear history, start fresh |
| `1` | Focus Queue | Expand queue panel |
| `2` | Focus Throughput | Expand throughput panel |
| `3` | Focus Workers | Expand workers panel |
| `4` | Focus Sinks | Expand sinks panel |
| `Tab` | Next panel | Cycle through panels |
| `j`/`k` | Scroll | Scroll within focused panel |

#### Sparkline Rendering

```rust
/// Render a sparkline from samples (using ASCII fallback for compatibility)
pub fn render_sparkline(samples: &[f64], width: usize) -> String {
    // ASCII blocks: . _ - = # for 5 levels
    const BLOCKS: [char; 5] = ['.', '_', '-', '=', '#'];

    if samples.is_empty() {
        return " ".repeat(width);
    }

    let max = samples.iter().cloned().fold(f64::MIN, f64::max);
    let min = samples.iter().cloned().fold(f64::MAX, f64::min);
    let range = max - min;

    samples
        .iter()
        .map(|&v| {
            if range == 0.0 {
                BLOCKS[2] // Middle block for flat line
            } else {
                let normalized = (v - min) / range;
                let idx = (normalized * 4.0).round() as usize;
                BLOCKS[idx.min(4)]
            }
        })
        .collect()
}
```

#### Data Fetching for Monitoring

```rust
/// Fetch all monitoring data in one query batch
pub async fn fetch_monitoring_data(pool: &SqlitePool) -> Result<MonitoringData> {
    // Queue stats
    let queue_stats: (i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            SUM(CASE WHEN status IN ('QUEUED', 'PENDING') THEN 1 ELSE 0 END),
            SUM(CASE WHEN status = 'RUNNING' THEN 1 ELSE 0 END),
            SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END),
            SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END)
        FROM cf_processing_queue
        "#
    )
    .fetch_one(pool)
    .await?;

    let queue_snapshot = QueueSnapshot {
        timestamp: Utc::now(),
        pending: queue_stats.0 as u32,
        running: queue_stats.1 as u32,
        completed: queue_stats.2 as u32,
        failed: queue_stats.3 as u32,
    };

    // Throughput from recent metrics
    let throughput: Vec<(String, f64, i64)> = sqlx::query_as(
        r#"
        SELECT
            metric_time,
            AVG(rows_per_second),
            AVG(bytes_per_second)
        FROM cf_job_metrics
        WHERE metric_time > datetime('now', '-5 minutes')
        GROUP BY strftime('%S', metric_time)
        ORDER BY metric_time ASC
        "#
    )
    .fetch_all(pool)
    .await?;

    let throughput_samples: Vec<ThroughputSample> = throughput
        .into_iter()
        .filter_map(|(time_str, rps, bps)| {
            DateTime::parse_from_rfc3339(&time_str)
                .ok()
                .map(|ts| ThroughputSample {
                    timestamp: ts.with_timezone(&Utc),
                    rows_per_second: rps,
                    bytes_per_second: bps as u64,
                })
        })
        .collect();

    // Sink statistics
    let sink_rows: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
        r#"
        SELECT
            sink_uri,
            SUM(total_rows),
            SUM(total_bytes),
            SUM(error_count),
            AVG(write_latency_ms)
        FROM cf_sink_stats
        WHERE recorded_at > datetime('now', '-5 minutes')
        GROUP BY sink_uri
        "#
    )
    .fetch_all(pool)
    .await?;

    let sinks: Vec<SinkStats> = sink_rows
        .into_iter()
        .map(|(uri, rows, bytes, errors, latency)| SinkStats {
            uri,
            total_rows: rows as u64,
            total_bytes: bytes as u64,
            error_count: errors as u32,
            write_latency_p50_ms: latency as u32,
            write_latency_p99_ms: (latency * 3) as u32, // Estimate, proper implementation later
            outputs: vec![],
        })
        .collect();

    Ok(MonitoringData {
        queue: queue_snapshot,
        throughput: throughput_samples,
        sinks,
        workers: vec![], // TODO: fetch from heartbeat data
    })
}
```

### State Machine Extension

```
                    JOB_LIST
                   (default)
                       |
         +-------------+-------------+-----------+
         |             |             |           |
     Enter|          'l'|          'f'|        'M'|
         v             v             v           v
    +---------+  +---------+  +---------+  +-----------+
    | DETAIL  |  |  LOGS   |  | FILTER  |  | MONITORING|
    | PANEL   |  | VIEWER  |  | DIALOG  |  |   PANEL   |
    +----+----+  +----+----+  +----+----+  +-----+-----+
         |            |            |             |
       Esc|         Esc|      Esc/Enter        Esc|
         |            |            |             |
         +------------+------------+-------------+
                      |
                      v
                  JOB_LIST
```

### Trade-offs

| Aspect | Benefit | Cost |
|--------|---------|------|
| Full monitoring panel | Comprehensive observability | More UI complexity |
| Real-time updates | Live feedback | CPU/DB load |
| ASCII sparklines | Works in all terminals | Less visual fidelity than Unicode blocks |
| Panel focus mode | Detailed view | More key bindings to learn |

### New Gaps Introduced

- **GAP-WORKER-001**: Need to track worker heartbeats to show worker status in monitoring

---

## Gap Resolution: GAP-PIPELINE-001

**Add pipeline/data-state visualization that user wants**

**Confidence:** MEDIUM

### Proposed Solution

Add a **Pipeline Summary** header to the Jobs view that shows data flow state. This is NOT a separate view, but an optional header that provides context without replacing the job list.

#### Pipeline Summary Design

```
+----------------------------------------------------------------------------------+
| JOBS                                                                              |
|                                                                                   |
|  +- PIPELINE STATUS -------------------------------------------------------+     |
|  |                                                                          |     |
|  |   SOURCE                PARSED                 OUTPUT                    |     |
|  |   +----------+         +----------+          +----------+                |     |
|  |   | 1,247    |  ---->  | 1,235    |  ---->   |  2 ready |                |     |
|  |   | files    |   @12   | files    |    @1    |  1 active|                |     |
|  |   +----------+         +----------+          +----------+                |     |
|  |                                                                          |     |
|  |   fix_parser v1.2: 1,235 files processed * 847 MB output                 |     |
|  +--------------------------------------------------------------------------+     |
|                                                                                   |
|  ---------------------------------------------------------------------------------  |
|                                                                                   |
|  Running: 1   Done: 3   Failed: 1                                                 |
|                                                                                   |
|> X PARSE    fix_parser v1.2                                        2m ago        |
|             12 files failed * SchemaViolation at row 42                           |
|  ...                                                                              |
|                                                                                   |
+----------------------------------------------------------------------------------+
|  [j/k] Navigate  [Enter] Details  [P] Toggle pipeline  [M] Monitoring            |
+----------------------------------------------------------------------------------+

Legend: @ = in progress count, > = selected, X = failed
```

#### Pipeline State Data Model

```rust
/// Pipeline state visualization data
#[derive(Debug, Clone, Default)]
pub struct PipelineState {
    /// Source files discovered
    pub source: PipelineStage,
    /// Files that have been parsed
    pub parsed: PipelineStage,
    /// Output exports (ready/in-progress)
    pub output: PipelineStage,
    /// Currently active parser (if any)
    pub active_parser: Option<ParserSummary>,
}

#[derive(Debug, Clone, Default)]
pub struct PipelineStage {
    pub count: u32,
    pub label: String,  // "files", "parquet", etc.
    pub in_progress: u32,  // Number currently being processed
}

#[derive(Debug, Clone)]
pub struct ParserSummary {
    pub name: String,
    pub version: String,
    pub files_processed: u32,
    pub output_size_bytes: u64,
}

/// Fetch pipeline state from database
pub async fn fetch_pipeline_state(pool: &SqlitePool) -> Result<PipelineState> {
    // Count source files
    let source_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cf_file_version"
    )
    .fetch_one(pool)
    .await?;

    // Count parsed files and running parse jobs
    let parsed_result: (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(DISTINCT file_version_id) FROM cf_processing_queue
             WHERE job_type = 'PARSE' AND status = 'COMPLETED'),
            (SELECT COUNT(*) FROM cf_processing_queue
             WHERE job_type = 'PARSE' AND status = 'RUNNING')
        "#
    )
    .fetch_one(pool)
    .await?;

    // Count output exports
    let output_result: (i64, i64) = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM cf_processing_queue
             WHERE job_type = 'EXPORT' AND status = 'COMPLETED'),
            (SELECT COUNT(*) FROM cf_processing_queue
             WHERE job_type = 'EXPORT' AND status = 'RUNNING')
        "#
    )
    .fetch_one(pool)
    .await?;

    // Get active parser summary
    let active_parser: Option<(String, i64, i64)> = sqlx::query_as(
        r#"
        SELECT
            plugin_name,
            COUNT(*),
            COALESCE(SUM(CASE WHEN output_path IS NOT NULL THEN 1 ELSE 0 END), 0)
        FROM cf_processing_queue
        WHERE job_type = 'PARSE' AND status = 'COMPLETED'
        GROUP BY plugin_name
        ORDER BY COUNT(*) DESC
        LIMIT 1
        "#
    )
    .fetch_optional(pool)
    .await?;

    Ok(PipelineState {
        source: PipelineStage {
            count: source_count as u32,
            label: "files".to_string(),
            in_progress: 0,
        },
        parsed: PipelineStage {
            count: parsed_result.0 as u32,
            label: "files".to_string(),
            in_progress: parsed_result.1 as u32,
        },
        output: PipelineStage {
            count: output_result.0 as u32,
            label: "ready".to_string(),
            in_progress: output_result.1 as u32,
        },
        active_parser: active_parser.map(|(name, processed, _)| ParserSummary {
            name,
            version: "1.0".to_string(), // TODO: get from manifest
            files_processed: processed as u32,
            output_size_bytes: 0, // TODO: calculate
        }),
    })
}
```

#### Pipeline Toggle Behavior

- `P` key toggles pipeline summary visibility
- Pipeline summary is **collapsed by default** (jobs-first philosophy)
- When expanded, takes 8 lines at top of Jobs view
- State persists during session (not persisted to disk)

```rust
pub struct JobsViewState {
    pub jobs_state: JobsState,
    pub show_pipeline: bool,  // Toggle with 'P'
    pub pipeline: PipelineState,
}
```

#### Edge Rendering

The arrows between stages indicate flow with status:

```rust
fn render_edge(in_progress: u32) -> String {
    if in_progress > 0 {
        format!("---->  @{}", in_progress)  // With spinner and count
    } else {
        "---->".to_string()  // Static arrow
    }
}
```

### Trade-offs

| Aspect | Benefit | Cost |
|--------|---------|------|
| Optional toggle | Jobs-first by default | Extra key binding |
| Inline in Jobs view | Single view, no context switch | Takes vertical space |
| Simple 3-stage model | Easy to understand | Does not show branching/multiple parsers |
| Aggregated counts | At-a-glance status | Loses per-parser detail |

### Alternative Considered: Full DAG Visualization

A full DAG showing SOURCE -> PARSER1 -> SINK1, SOURCE -> PARSER2 -> SINK2 was considered but rejected because:

1. Complexity: Multiple parsers create branching that is hard to render in terminal
2. Space: Would require dedicated view, not inline
3. Value: Most users have 1-3 parsers, summary is sufficient

For complex multi-parser setups, users can:
1. Filter jobs by parser name (`f` key)
2. View monitoring panel for detailed metrics
3. Use CLI `casparian jobs --topic fix_parser` for specific parser

### New Gaps Introduced

- None (pipeline state derived from existing database tables)

---

## Summary: Updated Spec Changes

### New Sections to Add

| Section | Content |
|---------|---------|
| Section 4.X | Pipeline Summary (optional header) |
| Section 5.5 | Backtest Job Rendering |
| Section 6.X | Monitoring Panel Keybindings |
| Section 10 | Full Monitoring Panel (replaces inline-only approach) |

### New Data Types

```rust
// Jobs view extended types
pub enum JobType {
    Scan, Parse, Export, Backtest,  // Added Backtest
}

pub struct BacktestJobInfo { ... }

// Monitoring types
pub struct MonitoringState { ... }
pub struct QueueSnapshot { ... }
pub struct ThroughputSample { ... }
pub struct WorkerInfo { ... }
pub struct SinkStats { ... }

// Pipeline types
pub struct PipelineState { ... }
pub struct PipelineStage { ... }
```

### New Database Schema

```sql
-- Extended columns on cf_processing_queue
ALTER TABLE cf_processing_queue ADD COLUMN job_type TEXT DEFAULT 'PARSE';
ALTER TABLE cf_processing_queue ADD COLUMN progress_pct INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN items_processed INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN items_total INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN updated_at TEXT;
ALTER TABLE cf_processing_queue ADD COLUMN backtest_data TEXT;

-- New tables for monitoring
CREATE TABLE IF NOT EXISTS cf_job_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL,
    metric_time TEXT NOT NULL DEFAULT (datetime('now')),
    rows_per_second REAL,
    bytes_per_second INTEGER,
    queue_depth INTEGER,
    FOREIGN KEY (job_id) REFERENCES cf_processing_queue(id)
);

CREATE TABLE IF NOT EXISTS cf_sink_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sink_uri TEXT NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
    total_rows INTEGER DEFAULT 0,
    total_bytes INTEGER DEFAULT 0,
    write_latency_ms INTEGER,
    error_count INTEGER DEFAULT 0
);
```

### New Keybindings

| Key | Context | Action |
|-----|---------|--------|
| `M` | Jobs list | Open monitoring panel |
| `P` | Jobs list | Toggle pipeline summary |
| `S` | Backtest detail | Stop backtest |
| `p` | Monitoring | Pause/resume updates |
| `r` | Monitoring | Reset stats |
| `1-4` | Monitoring | Focus sub-panel |

---

## Open Gaps for Future Rounds

| Gap ID | Description | Priority |
|--------|-------------|----------|
| GAP-RETENTION-001 | Background task to clean old cf_job_metrics rows | MEDIUM |
| GAP-WORKER-001 | Track worker heartbeats for monitoring panel worker status | MEDIUM |
