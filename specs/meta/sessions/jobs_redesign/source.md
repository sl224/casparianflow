# Jobs View - Specification

**Status:** Ready for Implementation
**Version:** 1.0
**Last Updated:** January 2026

---

## 1. Overview

The Jobs view shows processing activity with two goals:
1. **Find problems fast** - Failed jobs surface immediately
2. **Know where output is** - Paths are always visible

**Non-goals:** Complex pipeline visualization, historical analytics, real-time dashboards.

---

## 2. User Questions This View Answers

| Question | Answer Location |
|----------|-----------------|
| "What failed?" | Failed jobs pinned to top with error |
| "Where's my output?" | Output path on every completed job |
| "How far along?" | Progress bar on running jobs |
| "How much is done?" | Status bar: X/Y files, Z rows |

---

## 3. Layout

```
┌─ JOBS ─────────────────────────────────────────────────────────────────────────┐
│                                                                                │
│  ↻ 1 running   ✓ 3 done   ✗ 1 failed     1,235/1,247 files • 847 MB output    │
│                                                                                │
│  ══════════════════════════════════════════════════════════════════════════════ │
│                                                                                │
│▸ ✗ PARSE    fix_parser v1.2                                        2m ago     │
│             12 files failed • SchemaViolation at row 42                       │
│             First failure: venue_nyse_20240115.log                            │
│                                                                                │
│  ↻ EXPORT   concordance → ./production_001/          ████████░░░░  67%       │
│             30,521/45,782 records • ETA 6m                                    │
│                                                                                │
│  ✓ PARSE    fix_parser v1.2                                        2m ago     │
│             1,235 files → ~/.casparian_flow/output/fix_orders/ (847 MB)      │
│                                                                                │
│  ✓ EXPORT   bloomberg-tca → ./tca_upload.csv (2.3 MB)              5m ago     │
│                                                                                │
│  ✓ SCAN     /data/fix_logs • 1,247 files                          15m ago     │
│                                                                                │
├────────────────────────────────────────────────────────────────────────────────┤
│  [j/k] Navigate  [Enter] Details  [R] Retry  [l] Logs  [y] Copy path          │
└────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Layout Components

### 4.1 Status Bar (Line 1)

One line showing aggregate state:

```
↻ 1 running   ✓ 3 done   ✗ 1 failed     1,235/1,247 files • 847 MB output
```

| Element | Description |
|---------|-------------|
| `↻ N running` | Count of jobs in progress |
| `✓ N done` | Completed successfully |
| `✗ N failed` | Needs attention (red) |
| `X/Y files` | Total files processed / total |
| `Z MB output` | Total output size |

**Omit zero counts.** If nothing failed, don't show `✗ 0 failed`.

### 4.2 Job List

Jobs sorted by: **Failed first**, then by recency.

Each job is 1-3 lines depending on type and status:

**Line 1 (always):** Status, type, name, timestamp/progress
**Line 2 (if relevant):** Details - counts, output path, error
**Line 3 (failed only):** First failure filename

### 4.3 Footer

Context-sensitive keybindings. Only show actions that apply to selected job.

---

## 5. Job Type Rendering

### 5.1 Scan Job

```
✓ SCAN     /data/fix_logs • 1,247 files                          15m ago
```

One line. Path scanned, file count, timestamp.

### 5.2 Parse Job (Complete)

```
✓ PARSE    fix_parser v1.2                                        2m ago
           1,235 files → ~/.casparian_flow/output/fix_orders/ (847 MB)
```

Two lines. Parser name/version, file count, **output path**, size.

### 5.3 Parse Job (Failed)

```
✗ PARSE    fix_parser v1.2                                        2m ago
           12 files failed • SchemaViolation at row 42
           First failure: venue_nyse_20240115.log
```

Three lines. Error summary, **first failing file** (so user can investigate).

### 5.4 Parse Job (Running)

```
↻ PARSE    fix_parser v1.2                            ████████░░░░  67%
           892/1,247 files • ETA 4m
```

Two lines. Progress bar, file count, ETA.

### 5.5 Export Job (Complete)

```
✓ EXPORT   concordance → ./production_001/ (156 MB)               5m ago
           45,782 records • Bates SMITH000001-045782
```

Two lines. Output path, record count, format-specific info (Bates for legal).

### 5.6 Export Job (Running)

```
↻ EXPORT   concordance → ./production_001/          ████████░░░░  67%
           30,521/45,782 records • ETA 6m
```

Two lines. Output path (even while running), progress, ETA.

---

## 6. Keybindings

### 6.1 Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Select next job |
| `k` / `↑` | Select previous job |
| `g` | Go to first job |
| `G` | Go to last job |

### 6.2 Actions

| Key | Action | When Available |
|-----|--------|----------------|
| `Enter` | Open detail panel | Always |
| `l` | View logs | Always |
| `R` | Retry failed files | Failed jobs |
| `c` | Cancel job | Running jobs |
| `y` | Copy output path to clipboard | Completed jobs with output |
| `o` | Open output folder | Completed exports |
| `x` | Clear completed jobs | When completed jobs exist |

### 6.3 Global

| Key | Action |
|-----|--------|
| `f` | Filter by type/status |
| `?` | Show help |

---

## 7. Detail Panel

Pressing `Enter` opens a right-side panel with full details:

```
┌─ JOB DETAILS ──────────────────────────────────────┐
│                                                     │
│  Type:       Parse                                  │
│  Parser:     fix_parser v1.2                        │
│  Status:     Failed (12 files)                      │
│  Started:    10:38:15                               │
│  Duration:   2m 34s                                 │
│                                                     │
│  OUTPUT                                             │
│  ~/.casparian_flow/output/fix_orders/              │
│  1,235 files • 847 MB                              │
│                                                     │
│  FAILURES (12)                                      │
│  ─────────────                                      │
│  venue_nyse_20240115.log                           │
│    SchemaViolation: expected Integer at row 42     │
│  venue_lse_20240115.log                            │
│    SchemaViolation: expected Integer at row 87     │
│  ... (10 more)                                      │
│                                                     │
├─────────────────────────────────────────────────────┤
│  [R] Retry all  [l] Logs  [Esc] Close              │
└─────────────────────────────────────────────────────┘
```

**Key info in detail panel:**
- All failures listed (not just first)
- Full error messages
- Output path (copyable)
- Timing information

---

## 8. State Machine

```
                    JOB_LIST
                   (default)
                       │
         ┌─────────────┼─────────────┐
         │             │             │
     Enter│          'l'│          'f'│
         ▼             ▼             ▼
    ┌─────────┐  ┌─────────┐  ┌─────────┐
    │ DETAIL  │  │  LOGS   │  │ FILTER  │
    │ PANEL   │  │ VIEWER  │  │ DIALOG  │
    └────┬────┘  └────┬────┘  └────┬────┘
         │            │            │
       Esc│         Esc│      Esc/Enter
         │            │            │
         └────────────┴────────────┘
                      │
                      ▼
                  JOB_LIST
```

Four states total. No nested states. No overlays.

---

## 9. Data Model

```rust
pub struct JobInfo {
    pub id: Uuid,
    pub job_type: JobType,
    pub name: String,                    // parser name, exporter name, source path
    pub version: Option<String>,         // parser/exporter version
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,

    // Progress
    pub items_total: u32,
    pub items_processed: u32,
    pub items_failed: u32,

    // Output
    pub output_path: Option<PathBuf>,
    pub output_size_bytes: Option<u64>,

    // Errors (for failed jobs)
    pub failures: Vec<JobFailure>,
}

pub struct JobFailure {
    pub file_path: PathBuf,
    pub error: String,
    pub line: Option<u32>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum JobType {
    Scan,
    Parse,
    Export,
}

#[derive(Clone, Copy, PartialEq)]
pub enum JobStatus {
    Running,
    Complete,
    Failed,
    Cancelled,
}
```

---

## 10. Live Stats (Simple Approach)

Instead of a separate monitoring panel, show live stats **inline in the status bar**:

```
↻ 1 running   ✓ 3 done     1,235/1,247 files • 847 MB • 2.4k rows/s
```

When a job is running, the status bar shows:
- Current throughput (`2.4k rows/s`)
- Updates every second
- Disappears when nothing is running

**That's it.** No sparklines, no trend charts, no separate panel. Just one number that tells you "things are moving."

For detailed monitoring, users can:
1. Look at the running job's progress bar
2. Check the detail panel for timing info
3. Watch the status bar throughput

---

## 11. Implementation Notes

### 11.1 Sorting

```rust
fn sort_jobs(jobs: &mut Vec<JobInfo>) {
    jobs.sort_by(|a, b| {
        // Failed jobs first
        let a_failed = a.status == JobStatus::Failed;
        let b_failed = b.status == JobStatus::Failed;
        if a_failed != b_failed {
            return b_failed.cmp(&a_failed);
        }
        // Then by recency (most recent first)
        b.started_at.cmp(&a.started_at)
    });
}
```

### 11.2 Progress Bar

20 characters wide. Filled proportionally.

```rust
fn render_progress(percent: u8) -> String {
    let filled = (percent as usize * 20) / 100;
    let empty = 20 - filled;
    format!("{}{}  {}%",
        "█".repeat(filled),
        "░".repeat(empty),
        percent)
}
```

### 11.3 ETA Calculation

```rust
fn calculate_eta(processed: u32, total: u32, elapsed_secs: u64) -> Option<Duration> {
    if processed == 0 { return None; }
    let remaining = total - processed;
    let rate = processed as f64 / elapsed_secs as f64;
    let eta_secs = (remaining as f64 / rate) as u64;
    Some(Duration::from_secs(eta_secs))
}
```

### 11.4 Output Path Display

Truncate from the left if too long, keeping the filename visible:

```rust
fn truncate_path(path: &Path, max_len: usize) -> String {
    let s = path.to_string_lossy();
    if s.len() <= max_len {
        return s.to_string();
    }
    format!("...{}", &s[s.len() - max_len + 3..])
}
```

---

## 12. What We Explicitly Don't Do

| Feature | Reason |
|---------|--------|
| Pipeline visualization | Adds complexity, single line is enough context |
| Sparkline charts | Overkill for CLI, throughput number is sufficient |
| Temporal grouping | Complicates sorting, failed-first is more useful |
| Sink detail view | Users know their sink config, just show path |
| Pause/reset stats | Over-engineering for rare use case |
| Historical trends | This is a job list, not an analytics dashboard |
| Multiple view modes | One view that does its job well |

---

## 13. Success Criteria

| Goal | Metric |
|------|--------|
| Find failures | Failed jobs visible without scrolling |
| Copy output path | `y` copies path in < 1 second |
| Understand progress | Running jobs show %, ETA, throughput |
| Retry failures | `R` on failed job retries immediately |

---

## 14. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01 | 1.0 | Crystallized spec: removed alternatives, simplified monitoring to inline throughput, focused on core user needs |
| 2026-01 | 0.2 | Added monitoring view (later simplified) |
| 2026-01 | 0.1 | Initial draft with 5 alternatives |
