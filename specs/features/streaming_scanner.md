# Streaming Scanner Architecture

**Version:** 1.2
**Status:** DRAFT (Reliability fixes implemented, incremental folder updates pending)
**Created:** 2026-01-14
**Related:** specs/views/discover.md (Glob Explorer), CLAUDE.md (Database)

---

## 1. Overview

This specification defines the streaming architecture for the file scanner, replacing the current batch-and-hold approach with a bounded-memory streaming pipeline.

### 1.1 Problem Statement

The current scanner architecture has a hard memory ceiling:

| Files | Peak Memory | Impact |
|-------|-------------|--------|
| 1M | ~490 MB | Acceptable |
| 10M | ~4.9 GB | OOM on 8GB machines |
| 100M | ~49 GB | Impossible |

**Root cause:** `parallel_walk()` collects ALL files into `Vec<ScannedFile>` before any persistence or UI feedback.

### 1.2 Design Goals

| Goal | Metric |
|------|--------|
| **Bounded memory** | O(batch_size) not O(file_count) |
| **Progressive feedback** | <1 sec to first visible result |
| **Cancellation** | User can abort at any time |
| **Instant navigation** | <5ms folder drill-down |

### 1.3 Non-Goals

- Real-time file watching (separate feature)
- Distributed scanning (single machine)
- Incremental diff scanning (future enhancement)

### 1.4 Implementation Gap: Incremental Folder Updates (2026-01-14)

> **STATUS: SPEC-IMPLEMENTATION MISMATCH**
>
> The spec describes incremental folder updates per batch (Section 3), but the current
> implementation rebuilds folder counts AFTER the scan completes.

**Current Implementation (scanner.rs):**
```rust
// AFTER scan completes - queries ALL files, takes ~10s for 1.2M files
async fn build_folder_counts(&self, source_id: &str) -> Result<()> {
    let paths = query_all("SELECT rel_path FROM scout_files WHERE source_id = ?");
    let deltas = compute_folder_deltas(&paths);  // O(n) CPU
    db.clear_folder_cache(source_id);
    db.batch_upsert_folder_counts(source_id, &deltas);
}
```

**Spec-Compliant Approach (Section 3):**
```rust
// DURING scan - updates folders incrementally per batch, O(batch_size) per call
while let Some(files) = rx.recv().await {
    persist_batch(&batch_buffer, &mut tx_db, source_id).await?;
    batch_update_folder_counts(&batch_buffer, &mut tx_db, source_id).await?;  // <-- Missing!
    tx_db.commit().await?;
}
```

**Impact:**
- TUI must wait for full scan + rebuild (~10s for 1.2M files) before showing accurate folders
- Falls back to slow GROUP BY query when `scout_folders` is empty during scan

**Required Changes:**
1. Move `batch_update_folder_counts()` INTO the persist loop (per batch)
2. Remove post-scan `build_folder_counts()` call
3. Handle UPSERT with `ON CONFLICT` for incremental count updates

### 1.5 Scanner Reliability Fixes (2026-01-16)

> **STATUS: IMPLEMENTED**
>
> All 10 issues from `docs/SCAN_REVIEW.md` have been fixed. These address scanner
> reliability, cross-platform compatibility, and progress reporting accuracy.

**Critical Fixes:**
- **GAP-SCAN-001**: `scan_ok` flag prevents marking files deleted on partial/failed scans
- **GAP-SCAN-002**: Batch upsert now clears `error`/`sentinel_job_id` on file change

**High Priority Fixes:**
- **GAP-SCAN-003**: Cross-platform path normalization (forward slashes)
- **GAP-SCAN-004**: Folder cache truncation tracking for UI indicators

**Medium Priority Fixes:**
- **GAP-SCAN-005**: Separate `files_found` vs `files_persisted` counters
- **GAP-SCAN-006**: Immediate directory counting (not batched)
- **GAP-SCAN-007**: `AtomicU64` for byte counts (32-bit safe)
- **GAP-SCAN-008**: Progress updates per-file, not per-batch
- **GAP-SCAN-009**: Reliable symlink detection via `entry.file_type()`

**Low Priority Fixes:**
- **GAP-SCAN-010**: Correct SQLite bulk insert chunk size (83 rows)

See `docs/SCAN_REVIEW.md` for detailed implementation notes.

---

## 2. Architecture

### 2.1 Current Flow (Blocking)

```
parallel_walk() ──▶ Vec<ScannedFile> ──▶ persist_files() ──▶ build_cache()
     │                    │                   │                  │
     ▼                    ▼                   ▼                  ▼
  All files in        ~490 MB for        Sequential         Load ALL
  memory at once      1M files           one-by-one         paths again

Memory: O(file_count) - unbounded
User sees: NOTHING until complete
```

### 2.2 Streaming Flow (Progressive)

```
parallel_walk()
     │
     ├──▶ mpsc::channel(10) ──▶ Batch Writer ──▶ DB (scout_files + scout_folders)
     │    (bounded backpressure)     │
     │                               └──▶ Progress events to TUI
     │
Memory: O(batch_size) ≈ 10KB
User sees: Live progress, file counts, cancel option
```

### 2.3 Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           STREAMING PIPELINE                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                   │
│  │   Walker     │    │   Batch      │    │   Persist    │                   │
│  │  Threads     │───▶│   Channel    │───▶│   Task       │                   │
│  │  (N cores)   │    │  (bounded)   │    │  (single)    │                   │
│  └──────────────┘    └──────────────┘    └──────────────┘                   │
│         │                   │                   │                            │
│         │                   │                   ├───▶ INSERT scout_files     │
│         │                   │                   ├───▶ UPSERT scout_folders   │
│         │                   │                   └───▶ Progress event         │
│         │                   │                                                │
│         ▼                   ▼                                                │
│  ┌──────────────┐    ┌──────────────┐                                       │
│  │   Cancel     │    │  Backpressure│                                       │
│  │   Flag       │    │  (blocks if  │                                       │
│  │ (AtomicBool) │    │  channel full)│                                       │
│  └──────────────┘    └──────────────┘                                       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Components

### 3.1 ScanStream API

**Type References:**
- `Database`: `sqlx::SqlitePool` wrapper from `casparian_scout::db`
- `Source`: `scout_sources` row from `casparian_scout::Source`

```rust
/// Streaming scan handle - replaces blocking parallel_walk()
pub struct ScanStream {
    /// Receive progress events
    progress_rx: mpsc::Receiver<ScanEvent>,
    /// Cancel flag - set to true to abort
    cancel: Arc<AtomicBool>,
    /// Join handle for completion
    handle: JoinHandle<Result<ScanStats>>,
}

impl ScanStream {
    /// Start streaming scan
    ///
    /// # Parameters
    /// - `source`: Source configuration from scout_sources table
    /// - `db`: Database connection pool (sqlx::SqlitePool wrapper)
    pub fn start(source: &Source, db: &Database) -> Self;

    /// Request cancellation (non-blocking)
    pub fn cancel(&self);

    /// Check if scan is complete
    pub fn is_done(&self) -> bool;

    /// Try to receive next event (non-blocking)
    pub fn try_recv(&mut self) -> Option<ScanEvent>;

    /// Wait for completion
    pub async fn join(self) -> Result<ScanStats>;
}
```

### 3.2 ScanEvent Enum

```rust
pub enum ScanEvent {
    /// Batch persisted to database
    Progress {
        discovered: usize,
        persisted: usize,
        rate: f64,  // files/sec (rolling 5s window)
    },

    /// Folder structure updated
    FolderUpdate {
        prefix: String,
        count: u32,
    },

    /// Non-fatal error
    Error(String),

    /// Scan complete
    Complete(ScanStats),
}

pub struct ScanStats {
    pub total_files: usize,
    pub total_folders: usize,
    pub duration_secs: f64,
    pub avg_rate: f64,
    pub errors: Vec<String>,
}
```

### 3.3 Batch Writer

The persist task receives batches from the channel and:

1. **Inserts files** into `scout_files` table (existing)
2. **Upserts folders** into `scout_folders` table (new)
3. **Emits progress** events to TUI

**CRITICAL: Single Writer Guarantee**

The persist task is the ONLY writer to `scout_folders` during a scan. This eliminates race conditions on folder counts. Walker threads only send file paths via channel; they never write to the database.

**Transaction Boundaries:**

Each batch is wrapped in a single transaction using `BEGIN IMMEDIATE` to acquire a write lock immediately (prevents writer starvation under read load).

```rust
const BATCH_SIZE: usize = 1000;

async fn persist_task(
    rx: mpsc::Receiver<Vec<ScannedFile>>,
    tx: mpsc::Sender<ScanEvent>,
    db: &Database,
    source_id: &str,
) -> Result<ScanStats> {
    let mut stats = ScanStats::default();
    let mut batch_buffer = Vec::with_capacity(BATCH_SIZE);

    while let Some(files) = rx.recv().await {
        batch_buffer.extend(files);

        if batch_buffer.len() >= BATCH_SIZE {
            // Single transaction for files + folder counts
            let mut tx_db = db.pool().begin().await?;
            sqlx::query("PRAGMA busy_timeout = 5000")
                .execute(&mut *tx_db).await?;

            persist_batch(&batch_buffer, &mut tx_db, source_id).await?;
            batch_update_folder_counts(&batch_buffer, &mut tx_db, source_id).await?;

            tx_db.commit().await?;

            stats.total_files += batch_buffer.len();
            tx.send(ScanEvent::Progress { ... }).await?;

            batch_buffer.clear();
        }
    }

    // Flush remaining
    if !batch_buffer.is_empty() {
        let mut tx_db = db.pool().begin().await?;
        persist_batch(&batch_buffer, &mut tx_db, source_id).await?;
        batch_update_folder_counts(&batch_buffer, &mut tx_db, source_id).await?;
        tx_db.commit().await?;
    }

    Ok(stats)
}
```

---

## 4. Database Schema

### 4.1 New Table: scout_folders

```sql
CREATE TABLE scout_folders (
    id INTEGER PRIMARY KEY,
    source_id TEXT NOT NULL,
    -- Prefix path, e.g., "" for root, "logs/" for /logs folder
    prefix TEXT NOT NULL,
    -- Folder or file name at this level
    name TEXT NOT NULL,
    -- Count of files in this subtree
    file_count INTEGER NOT NULL DEFAULT 0,
    -- True for folders, false for files
    is_folder BOOLEAN NOT NULL,
    -- When this row was last updated
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(source_id, prefix, name),
    FOREIGN KEY (source_id) REFERENCES scout_sources(id) ON DELETE CASCADE
);

CREATE INDEX idx_scout_folders_lookup
    ON scout_folders(source_id, prefix);
```

### 4.2 Folder Count Updates

For a file path like `logs/errors/crash.log`, update all ancestors:

| prefix | name | file_count | is_folder |
|--------|------|------------|-----------|
| `""` | `logs` | +1 | true |
| `logs/` | `errors` | +1 | true |
| `logs/errors/` | `crash.log` | 1 | false |

**Performance Optimization (CRITICAL):**

The naive approach of one INSERT per path segment would generate ~4M queries for 1M files (assuming avg depth of 4). Instead, we aggregate counts in-memory first, then batch upsert.

```rust
use std::collections::HashMap;

/// Key: (prefix, name), Value: (count_delta, is_folder)
type FolderDelta = HashMap<(String, String), (i64, bool)>;

/// Aggregate folder counts from a batch of files
fn compute_folder_deltas(files: &[ScannedFile]) -> FolderDelta {
    let mut deltas = FolderDelta::new();

    for file in files {
        let segments: Vec<&str> = file.rel_path.split('/').collect();
        let mut prefix = String::new();

        for (i, segment) in segments.iter().enumerate() {
            let is_file = i == segments.len() - 1;
            let key = (prefix.clone(), segment.to_string());

            deltas
                .entry(key)
                .and_modify(|(count, _)| *count += 1)
                .or_insert((1, !is_file));

            if !is_file {
                prefix.push_str(segment);
                prefix.push('/');
            }
        }
    }

    deltas
}

/// Batch upsert folder counts (single transaction, O(unique_folders) queries)
async fn batch_update_folder_counts(
    files: &[ScannedFile],
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &str,
) -> Result<()> {
    let deltas = compute_folder_deltas(files);

    // Prepare statement once, execute many
    for ((prefix, name), (count, is_folder)) in deltas {
        sqlx::query(r#"
            INSERT INTO scout_folders (source_id, prefix, name, file_count, is_folder)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(source_id, prefix, name) DO UPDATE
            SET file_count = file_count + excluded.file_count,
                updated_at = datetime('now')
        "#)
        .bind(source_id)
        .bind(&prefix)
        .bind(&name)
        .bind(count)
        .bind(is_folder)
        .execute(&mut **tx).await?;
    }

    Ok(())
}
```

**Query Count Analysis:**

| Scenario | Naive Approach | Batch Approach |
|----------|---------------|----------------|
| 1K files, avg depth 4 | 4,000 queries | ~500 queries |
| 1M files, avg depth 4 | 4,000,000 queries | ~50,000 queries |

The batch approach reduces queries by ~80x by aggregating counts before upserting.

### 4.3 Folder Count Decrement (File Deletion)

When files are deleted (e.g., during rescan with `--clean`), counts must be decremented:

```rust
/// Decrement counts for removed files (called during rescan cleanup)
async fn decrement_folder_counts(
    removed_files: &[String],  // rel_paths of removed files
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &str,
) -> Result<()> {
    // Same aggregation logic but with negative deltas
    let mut deltas = FolderDelta::new();

    for rel_path in removed_files {
        let segments: Vec<&str> = rel_path.split('/').collect();
        let mut prefix = String::new();

        for (i, segment) in segments.iter().enumerate() {
            let is_file = i == segments.len() - 1;
            let key = (prefix.clone(), segment.to_string());

            deltas
                .entry(key)
                .and_modify(|(count, _)| *count -= 1)
                .or_insert((-1, !is_file));

            if !is_file {
                prefix.push_str(segment);
                prefix.push('/');
            }
        }
    }

    for ((prefix, name), (delta, _)) in deltas {
        sqlx::query(r#"
            UPDATE scout_folders
            SET file_count = file_count + ?,
                updated_at = datetime('now')
            WHERE source_id = ? AND prefix = ? AND name = ?
        "#)
        .bind(delta)
        .bind(source_id)
        .bind(&prefix)
        .bind(&name)
        .execute(&mut **tx).await?;
    }

    // Clean up zero-count folders
    sqlx::query(r#"
        DELETE FROM scout_folders
        WHERE source_id = ? AND file_count <= 0
    "#)
    .bind(source_id)
    .execute(&mut **tx).await?;

    Ok(())
}
```

### 4.4 Query for Drill-Down

```rust
pub async fn get_folder_children(
    db: &Database,
    source_id: &str,
    prefix: &str,
) -> Vec<FolderEntry> {
    sqlx::query_as(r#"
        SELECT name, file_count, is_folder
        FROM scout_folders
        WHERE source_id = ? AND prefix = ?
        ORDER BY is_folder DESC, name ASC
    "#)
    .bind(source_id)
    .bind(prefix)
    .fetch_all(db.pool())
    .await
    .unwrap_or_default()
}

pub struct FolderEntry {
    pub name: String,
    pub file_count: i64,
    pub is_folder: bool,
}
```

### 4.5 Staleness Detection

The TUI needs to know if folder data may be stale (e.g., if a scan is in progress or filesystem changed since last scan).

```sql
-- Add last_scan_at to scout_sources
ALTER TABLE scout_sources ADD COLUMN last_scan_at TEXT;

-- Query to check staleness
SELECT
    s.id,
    s.last_scan_at,
    (julianday('now') - julianday(s.last_scan_at)) * 24 * 60 AS minutes_since_scan
FROM scout_sources s
WHERE s.id = ?;
```

**TUI Staleness Indicator:**

```rust
pub struct GlobExplorerState {
    // ... existing fields ...

    /// True if a scan is currently running for this source
    scan_in_progress: bool,
    /// Minutes since last completed scan (None if never scanned)
    minutes_since_scan: Option<f64>,
}

impl GlobExplorerState {
    /// Returns true if data may be stale (>60 min since scan or scan in progress)
    pub fn is_stale(&self) -> bool {
        self.scan_in_progress ||
        self.minutes_since_scan.map(|m| m > 60.0).unwrap_or(true)
    }
}
```

**UI Indication:**
- Show `[Scanning...]` badge during active scan
- Show `[Stale]` badge if >60 min since last scan
- Offer `r` keybinding to refresh/rescan
```

---

## 5. TUI Integration

### 5.1 Scan Progress UI

```
┌─────────────────────────────────────────────────────────────────┐
│  Scanning: /data/logs                                           │
│                                                                  │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━░░░░░░░░░░░░░░░░░░░░  62%        │
│                                                                  │
│  Files discovered: 6,234,567                                     │
│  Rate: 45,234 files/sec                                         │
│  Current folder: 2024/03/15/logs/...                            │
│                                                                  │
│  [Esc] Cancel                                                    │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 On-Demand Folder Loading

Replace in-memory `FolderCache` with on-demand queries:

**Before (app.rs):**
```rust
// Load ENTIRE cache at source switch
let folder_cache = FolderCache::load(&source_id).ok()?;
let cache: HashMap<String, Vec<FolderInfo>> = folder_cache.folders.iter()...;
```

**After:**
```rust
// Query only visible entries
async fn load_folder_children(&mut self, prefix: &str) {
    self.current_items = get_folder_children(&self.db, &self.source_id, prefix).await;
    self.current_prefix = prefix.to_string();
}
```

### 5.3 State Changes

The `GlobExplorerState` struct changes:

```rust
pub struct GlobExplorerState {
    // REMOVED: folder_cache: Option<HashMap<String, Vec<FolderInfo>>>,

    // ADDED: On-demand loading
    current_prefix: String,
    current_items: Vec<FolderEntry>,
    loading: bool,

    // Existing fields unchanged
    source_id: String,
    pattern: String,
    ...
}
```

---

## 6. Cancellation

### 6.1 Cancel Flow

```
User presses Esc
       │
       ▼
scan_stream.cancel()
       │
       ▼
cancel.store(true, Ordering::Relaxed)
       │
       ├───▶ Walker threads check cancel flag
       │          └──▶ Return early from iteration
       │
       └───▶ Persist task drains channel
                  └──▶ Commits final batch
                  └──▶ Returns partial ScanStats
```

### 6.2 Walker Integration

```rust
// In parallel_walk()
walker.run(|| {
    if cancel.load(Ordering::Relaxed) {
        return ignore::WalkState::Quit;
    }
    // ... process file ...
    ignore::WalkState::Continue
});
```

### 6.3 Partial Database State (IMPORTANT)

On cancellation, the database will contain a **consistent partial scan**:

| Guarantee | Description |
|-----------|-------------|
| **Atomic batches** | Each batch is fully committed or not at all |
| **Accurate folder counts** | Counts reflect exactly the files persisted |
| **No orphan files** | Files without folder entries cannot exist |
| **Resumable** | Next scan will find existing files via `ON CONFLICT` |

**What is NOT guaranteed:**
- The scan covered the entire directory tree
- Files discovered late in alphabetical order are present

**User Recovery:**
- User can re-run scan to complete
- Existing files are skipped via `ON CONFLICT DO NOTHING` (for scout_files)
- Folder counts are updated incrementally

```rust
/// Mark source scan as incomplete on cancel
async fn mark_scan_cancelled(
    db: &Database,
    source_id: &str,
    stats: &ScanStats,
) -> Result<()> {
    sqlx::query(r#"
        UPDATE scout_sources
        SET last_scan_at = datetime('now'),
            last_scan_status = 'cancelled',
            last_scan_files = ?
        WHERE id = ?
    "#)
    .bind(stats.total_files as i64)
    .bind(source_id)
    .execute(db.pool()).await?;

    Ok(())
}
```

---

## 7. Performance Characteristics

### 7.1 Memory Comparison

| Files | Current | Streaming | Savings |
|-------|---------|-----------|---------|
| 1M | 490 MB | ~10 MB | 98% |
| 10M | 4.9 GB | ~10 MB | 99.8% |
| 100M | 49 GB (OOM) | ~10 MB | ∞ |

### 7.2 Latency Comparison

| Operation | Current | Streaming | Improvement |
|-----------|---------|-----------|-------------|
| Time to first result | 30+ sec | <1 sec | 30x |
| Source switch | 50-100ms | <5ms | 10-20x |
| Folder drill-down | <1ms (in-memory) | 1-5ms (query) | ~same |
| Cancel response | N/A | <100ms | ∞ |

### 7.3 SQLite Write Performance

To maintain scan speed with SQLite writes:

1. **WAL mode**: Concurrent reads during writes
2. **Batch inserts**: 1000 files per transaction
3. **Prepared statements**: Reuse query plans
4. **Single writer**: Persist task serializes writes

Expected throughput: 50,000+ files/sec (bottleneck is filesystem, not SQLite).

---

## 8. Migration

### 8.1 Deprecation

The following are deprecated and will be removed:

| Component | Replacement |
|-----------|-------------|
| `FolderCache` struct | `scout_folders` table |
| `folder_cache.rs` | Removed entirely |
| `.bin.zst` cache files | SQLite table |
| `build_folder_cache()` | Inline during persist |

### 8.2 Migration from .bin.zst Files

Existing users have folder data in `.bin.zst` cache files. Migration strategy:

**Option A: Lazy Migration (Recommended)**

On first access to a source without `scout_folders` data:
1. Check if `.bin.zst` file exists for source
2. If exists, read and populate `scout_folders` table
3. Delete `.bin.zst` file after successful migration
4. If no cache file, trigger background rescan

```rust
async fn ensure_folder_data(
    db: &Database,
    source_id: &str,
) -> Result<bool> {
    // Check if we have folder data in SQLite
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scout_folders WHERE source_id = ?"
    )
    .bind(source_id)
    .fetch_one(db.pool()).await?;

    if count > 0 {
        return Ok(true);  // Already migrated
    }

    // Try to migrate from .bin.zst
    let cache_path = get_cache_path(source_id);
    if cache_path.exists() {
        let folder_cache = FolderCache::load(&cache_path)?;
        migrate_folder_cache_to_duckdb(db, source_id, &folder_cache).await?;
        std::fs::remove_file(&cache_path)?;
        return Ok(true);
    }

    Ok(false)  // No data, needs rescan
}

async fn migrate_folder_cache_to_duckdb(
    db: &Database,
    source_id: &str,
    cache: &FolderCache,
) -> Result<()> {
    let mut tx = db.pool().begin().await?;

    for (prefix, children) in &cache.folders {
        for child in children {
            sqlx::query(r#"
                INSERT INTO scout_folders (source_id, prefix, name, file_count, is_folder)
                VALUES (?, ?, ?, ?, ?)
            "#)
            .bind(source_id)
            .bind(prefix)
            .bind(&child.name)
            .bind(child.file_count as i64)
            .bind(child.is_folder)
            .execute(&mut *tx).await?;
        }
    }

    tx.commit().await?;
    Ok(())
}
```

**Option B: Bulk Migration on Upgrade**

Run migration script for all sources at application startup:

```rust
async fn migrate_all_folder_caches(db: &Database) -> Result<MigrationStats> {
    let sources = get_all_sources(db).await?;
    let mut stats = MigrationStats::default();

    for source in sources {
        match ensure_folder_data(db, &source.id).await {
            Ok(true) => stats.migrated += 1,
            Ok(false) => stats.needs_rescan += 1,
            Err(e) => {
                tracing::warn!("Migration failed for {}: {}", source.id, e);
                stats.failed += 1;
            }
        }
    }

    Ok(stats)
}
```

### 8.3 Migration Steps

1. Add `scout_folders` table to schema
2. Add `last_scan_at`, `last_scan_status` columns to `scout_sources`
3. Implement streaming `parallel_walk()`
4. Add `ensure_folder_data()` lazy migration
5. Update TUI to use on-demand queries
6. Remove `FolderCache` code and files after deprecation period
7. Clean up old `.bin.zst` cache files (delete after successful migration)

---

## 9. Implementation Phases

### Phase 1: Streaming Glob (P0) - Critical

- [ ] Create `ScanStream` struct
- [ ] Modify `parallel_walk()` to send batches via channel
- [ ] Add cancel flag to walker threads
- [ ] Add `persist_task` that writes batches
- [ ] Memory: O(batch_size) achieved

### Phase 2: SQLite Folder Table (P1)

- [ ] Add `scout_folders` table to schema
- [ ] Implement `update_folder_counts()` during persist
- [ ] Implement `get_folder_children()` query
- [ ] Update TUI to query on-demand
- [ ] Remove `FolderCache` and `.bin.zst` files

### Phase 3: Progress UI (P2)

- [ ] Add progress panel to scan dialog
- [ ] Implement rate calculation (rolling window)
- [ ] Show current folder being scanned
- [ ] Implement Esc to cancel

### Phase 4: Cleanup (P3)

- [ ] Remove `folder_cache.rs`
- [ ] Remove old cache file cleanup
- [ ] Update tests

---

## 10. Testing

### 10.1 Unit Tests

```rust
#[tokio::test]
async fn test_streaming_scan_bounded_memory() {
    // Create 100K temp files
    // Run streaming scan
    // Assert peak memory < 50MB
}

#[tokio::test]
async fn test_cancel_stops_scan() {
    // Start scan on large directory
    // Cancel after 1 second
    // Assert scan stopped within 100ms
}

#[tokio::test]
async fn test_folder_counts_accurate() {
    // Scan known directory structure
    // Query folder counts
    // Assert counts match expected
}
```

### 10.2 Integration Tests

```rust
#[tokio::test]
async fn test_tui_folder_navigation() {
    // Scan test directory
    // Navigate via get_folder_children()
    // Assert correct items at each level
}
```

---

## 11. Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-14 | Initial specification |
| 1.1 | 2026-01-14 | Gap fixes: batch upserts (4.2), count decrement (4.3), staleness detection (4.5), partial cancel state (6.3), .bin.zst migration (8.2) |
| 1.2 | 2026-01-16 | Added Section 1.5: Scanner reliability fixes (GAP-SCAN-001 through GAP-SCAN-010) from docs/SCAN_REVIEW.md |
