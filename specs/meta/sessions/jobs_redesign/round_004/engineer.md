## Gap Resolution: GAP-DATA-001

**Confidence:** HIGH

### Proposed Solution

The TUI should learn about job data via **database polling**, not event-driven IPC. This aligns with existing patterns in the codebase and leverages infrastructure that already exists.

#### Architecture: Database as Event Store

```
                                         ┌──────────────────────┐
┌─────────────┐                          │    cf_processing_    │
│   Sentinel  │───────────┐              │        queue         │
│  (control   │           │              │  ┌────────────────┐  │
│   plane)    │           │              │  │ id             │  │
└─────────────┘           │              │  │ status         │  │
       │                  │              │  │ plugin_name    │  │
       │ ZMQ              ▼              │  │ retry_count    │  │
       │              ┌───────┐         │  │ progress_pct   │◄─┼──── NEW COLUMN
┌──────▼──────┐       │Worker │         │  │ items_processed│◄─┼──── NEW COLUMN
│   Worker 1  │───────┤Process│──────────│ │ items_total    │◄─┼──── NEW COLUMN
└─────────────┘       └───────┘          │  │ output_path    │◄─┼──── NEW COLUMN
┌─────────────┐                          │  │ output_size    │◄─┼──── NEW COLUMN
│   Worker 2  │──────────────────────────│  │ error_message  │  │
└─────────────┘                          │  │ started_at     │◄─┼──── NEW COLUMN
                                         │  │ completed_at   │  │
                                         │  └────────────────┘  │
                                         └──────────────────────┘
                                                    ▲
                                                    │
                                        ┌───────────┴───────────┐
                                        │  Database Polling     │
                                        │  (500ms interval)     │
                                        └───────────┬───────────┘
                                                    │
                                         ┌──────────▼──────────┐
                                         │       TUI           │
                                         │   (Jobs View)       │
                                         └─────────────────────┘
```

#### Why Database Polling (Not Event-Driven)

1. **Already exists**: Workers already update `cf_processing_queue` table via Sentinel (CONCLUDE messages update status)
2. **No new IPC**: Adding ZMQ channels between TUI and workers would add complexity
3. **Survives disconnects**: TUI can reconnect and immediately have current state
4. **Multi-TUI safe**: Multiple TUI instances see the same state
5. **Matches Discover pattern**: DiscoverState already polls Scout DB for files/sources

#### Database Schema Changes

Add columns to existing `cf_processing_queue` table:

```sql
-- Migration: Add progress tracking columns
ALTER TABLE cf_processing_queue ADD COLUMN progress_pct INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN items_processed INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN items_total INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN output_path TEXT;
ALTER TABLE cf_processing_queue ADD COLUMN output_size_bytes INTEGER DEFAULT 0;
ALTER TABLE cf_processing_queue ADD COLUMN started_at TEXT;  -- ISO8601

-- Index for TUI queries (most recent first, failed first)
CREATE INDEX IF NOT EXISTS idx_processing_queue_tui
ON cf_processing_queue(status, started_at DESC);
```

**Note per CLAUDE.md**: "Alpha app: change schema directly, users delete DB if needed" - no migration system required.

#### Data Flow: Worker -> Database -> TUI

**Step 1: Worker Progress Updates**

Workers already send `JobReceipt` on completion. Extend protocol to include progress updates:

```rust
// In casparian_protocol/src/types.rs

/// Progress update (Worker -> Sentinel, streamed during job execution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressUpdate {
    pub job_id: u64,
    pub items_processed: u32,
    pub items_total: u32,
    pub progress_pct: u8,  // 0-100
}

// New OpCode in protocol (add to existing enum)
OpCode::Progress = 0x07,
```

**Step 2: Sentinel Writes Progress to DB**

```rust
// In sentinel.rs, handle_message()
OpCode::Progress => {
    let update: ProgressUpdate = serde_json::from_slice(&msg.payload)?;
    sqlx::query(
        "UPDATE cf_processing_queue SET
         progress_pct = ?, items_processed = ?, items_total = ?
         WHERE id = ?"
    )
    .bind(update.progress_pct)
    .bind(update.items_processed)
    .bind(update.items_total)
    .bind(update.job_id as i64)
    .execute(&self.pool)
    .await?;
}
```

**Step 3: TUI Polls Database**

```rust
// In tui/app.rs - add to App struct
pub struct App {
    // ... existing fields ...
    jobs_refresh_interval: tokio::time::Interval,
    db_pool: sqlx::Pool<sqlx::Sqlite>,
}

impl App {
    pub fn new() -> Self {
        Self {
            // ... existing init ...
            jobs_refresh_interval: tokio::time::interval(Duration::from_millis(500)),
            db_pool: /* connect to ~/.casparian_flow/casparian_flow.sqlite3 */,
        }
    }

    /// Refresh jobs from database (called every 500ms when Jobs view active)
    async fn refresh_jobs(&mut self) -> Result<()> {
        let jobs: Vec<JobRow> = sqlx::query_as(
            r#"
            SELECT
                pq.id, pq.plugin_name, pq.status, pq.retry_count,
                pq.progress_pct, pq.items_processed, pq.items_total,
                pq.output_path, pq.output_size_bytes,
                pq.error_message, pq.started_at, pq.end_time,
                COALESCE(sr.path || '/' || fl.rel_path, pq.input_file) as file_path
            FROM cf_processing_queue pq
            LEFT JOIN cf_file_version fv ON pq.file_version_id = fv.id
            LEFT JOIN cf_file_location fl ON fv.location_id = fl.id
            LEFT JOIN cf_source_root sr ON fl.source_root_id = sr.id
            ORDER BY
                CASE pq.status WHEN 'FAILED' THEN 0 ELSE 1 END,  -- Failed first
                pq.started_at DESC
            LIMIT 100
            "#
        )
        .fetch_all(&self.db_pool)
        .await?;

        self.jobs_state.jobs = jobs.into_iter().map(JobInfo::from).collect();
        Ok(())
    }
}
```

#### Update Frequency

| Context | Polling Interval | Rationale |
|---------|------------------|-----------|
| Jobs view active | 500ms | Smooth progress bar updates |
| Jobs view inactive | 0 (paused) | No wasted queries |
| Status bar (other views) | 2s | Just need counts, not details |

#### How TUI Learns About New Jobs

Jobs are created by:
1. **CLI (`casparian run`)**: Writes directly to `cf_processing_queue`
2. **Sentinel dispatch**: Updates status from QUEUED -> RUNNING
3. **Tagging rule triggers**: External process writes to queue

TUI discovers new jobs on next poll cycle. With 500ms polling, new jobs appear within 500ms - fast enough for human perception.

#### Handling Job Completion Updates

When a job completes, Sentinel already updates `cf_processing_queue`:

```rust
// Existing code in sentinel.rs handle_conclude()
JobStatus::Success => {
    self.queue.complete_job(job_id, "Success").await?;
    // ^^^ This sets status='COMPLETED', end_time=now()
}
```

Extend to include output metadata:

```rust
// Enhanced complete_job
pub async fn complete_job(&self, job_id: i64, message: &str, receipt: &JobReceipt) -> Result<()> {
    // Extract output path from artifacts
    let output_path = receipt.artifacts.first()
        .and_then(|a| a.get("uri"))
        .map(|u| u.strip_prefix("file://").unwrap_or(u));

    let output_size = receipt.metrics.get("size_bytes").copied().unwrap_or(0);

    sqlx::query(
        "UPDATE cf_processing_queue SET
         status = 'COMPLETED', end_time = datetime('now'),
         output_path = ?, output_size_bytes = ?,
         progress_pct = 100, items_processed = items_total
         WHERE id = ?"
    )
    .bind(output_path)
    .bind(output_size)
    .bind(job_id)
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

### Examples

#### SQL Query for Jobs List

```sql
-- Query used by TUI to fetch job list
SELECT
    pq.id,
    pq.plugin_name,
    pq.status,
    pq.progress_pct,
    pq.items_processed,
    pq.items_total,
    pq.output_path,
    pq.output_size_bytes,
    pq.error_message,
    pq.started_at,
    pq.end_time,
    pq.retry_count,
    COALESCE(sr.path || '/' || fl.rel_path, pq.input_file) as file_path
FROM cf_processing_queue pq
LEFT JOIN cf_file_version fv ON pq.file_version_id = fv.id
LEFT JOIN cf_file_location fl ON fv.location_id = fl.id
LEFT JOIN cf_source_root sr ON fl.source_root_id = sr.id
WHERE
    pq.status IN ('RUNNING', 'COMPLETED', 'FAILED', 'QUEUED')
ORDER BY
    CASE pq.status WHEN 'FAILED' THEN 0 ELSE 1 END,  -- Failed jobs first
    pq.started_at DESC
LIMIT 100;
```

#### SQL Query for Status Bar Summary

```sql
-- Aggregate query for status bar (efficient, runs every 2s)
SELECT
    SUM(CASE WHEN status = 'RUNNING' THEN 1 ELSE 0 END) as running,
    SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END) as completed,
    SUM(CASE WHEN status = 'FAILED' THEN 1 ELSE 0 END) as failed,
    SUM(CASE WHEN status = 'RUNNING' THEN items_processed ELSE 0 END) as items_processed,
    SUM(CASE WHEN status = 'RUNNING' THEN items_total ELSE 0 END) as items_total,
    SUM(output_size_bytes) as total_output_bytes
FROM cf_processing_queue
WHERE started_at > datetime('now', '-24 hours');
```

#### Rust Data Model (TUI side)

```rust
/// Row from cf_processing_queue (maps to JobInfo in spec)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct JobRow {
    pub id: i64,
    pub plugin_name: String,
    pub status: String,
    pub progress_pct: i32,
    pub items_processed: i32,
    pub items_total: i32,
    pub output_path: Option<String>,
    pub output_size_bytes: i64,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub end_time: Option<String>,
    pub retry_count: i32,
    pub file_path: Option<String>,
}

impl From<JobRow> for JobInfo {
    fn from(row: JobRow) -> Self {
        JobInfo {
            id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &row.id.to_le_bytes()),
            job_type: JobType::Parse,  // Infer from plugin or add column
            name: row.plugin_name,
            version: None,  // Could add parser version column
            status: match row.status.as_str() {
                "RUNNING" => JobStatus::Running,
                "COMPLETED" => JobStatus::Complete,
                "FAILED" => JobStatus::Failed,
                "CANCELLED" => JobStatus::Cancelled,
                _ => JobStatus::Running,
            },
            started_at: row.started_at.map(|s| DateTime::parse_from_rfc3339(&s).ok()).flatten(),
            completed_at: row.end_time.map(|s| DateTime::parse_from_rfc3339(&s).ok()).flatten(),
            items_total: row.items_total as u32,
            items_processed: row.items_processed as u32,
            items_failed: if row.status == "FAILED" { row.items_total as u32 } else { 0 },
            output_path: row.output_path.map(PathBuf::from),
            output_size_bytes: Some(row.output_size_bytes as u64),
            failures: vec![],  // Load on demand when detail panel opens
        }
    }
}
```

### Trade-offs

**Pros:**
1. **Simple**: No new IPC channels, no message queues, no pub/sub
2. **Resilient**: TUI crash/restart instantly recovers state from DB
3. **Consistent**: Single source of truth (database) for all consumers
4. **Existing pattern**: Matches how Discover mode already loads files/sources
5. **Debuggable**: Can query DB directly to see job state (`sqlite3 ~/.casparian_flow/casparian_flow.sqlite3`)

**Cons:**
1. **Polling latency**: 500ms worst-case delay for updates (acceptable for UX)
2. **DB load**: One query every 500ms per TUI instance (negligible for SQLite)
3. **Progress granularity**: Workers must emit progress updates (requires worker change)

**Alternatives Considered:**

| Approach | Rejected Because |
|----------|------------------|
| ZMQ subscription (TUI subscribes to Sentinel) | Adds new IPC channel, TUI must run Sentinel |
| Shared memory | Platform-specific, complex synchronization |
| File watching | Unreliable across platforms, no schema |
| WebSocket | Adds HTTP server, overkill for local CLI |

### New Gaps Introduced

1. **GAP-PROGRESS-001**: Workers currently don't emit progress updates during execution. The Python bridge_shim needs to be extended to emit `OpCode::Progress` messages as it processes rows. This is a separate implementation task.

2. **GAP-JOBTYPE-001**: The spec shows `JobType::Scan`, `JobType::Parse`, `JobType::Export` but `cf_processing_queue` doesn't have a job_type column. Either:
   - Add `job_type TEXT` column, OR
   - Infer from context (Scan jobs come from Scout, Parse from Sentinel, Export from CLI)
