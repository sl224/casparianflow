# Engineer Response - Round 003

**Date:** 2026-01-13
**Focus:** Monitoring workflow and metrics
**Engineer:** Claude (Opus 4.5)

---

## Context

Rounds 1-2 established debugging as primary, but user correctly notes:
> "We also want strong monitoring too. Rows published, recent activity, aggregate metrics."

This round explores what monitoring looks like and whether Jobs should have dual views (Monitor vs Debug).

---

## What Is Monitoring?

Monitoring answers: **"How is the system performing?"**

Unlike debugging ("why did X fail?"), monitoring is about:
- Throughput: How much work is being done?
- Health: Is everything running smoothly?
- Trends: Is performance improving or degrading?
- Activity: What just happened?

### Monitoring Questions by Vertical

**Finance (FIX → TCA):**
- "How many orders processed today?" (throughput)
- "Are we on track to hit the 4:30 deadline?" (progress)
- "What's our parse rate? (orders/second)" (performance)

**Legal (PST → Concordance):**
- "How many documents produced this session?" (throughput)
- "What's the current Bates range?" (progress)
- "How long until production is complete?" (ETA)

**Healthcare (HL7 → FHIR):**
- "How many messages processed in the last hour?" (throughput)
- "What's our success rate?" (health)
- "Any spikes in failures?" (trends)

**Defense (CoT → KML):**
- "Are all feeds being processed?" (health)
- "Data freshness - when was last update?" (recency)
- "Track count in current export?" (throughput)

---

## Monitoring Metrics Inventory

### Throughput Metrics

| Metric | Description | Granularity |
|--------|-------------|-------------|
| **Rows published** | Total output rows written | Per job, per parser, aggregate |
| **Files processed** | Input files completed | Per job, aggregate |
| **Records/second** | Processing rate | Per job (live), historical average |
| **Files/minute** | File completion rate | Aggregate |

### Health Metrics

| Metric | Description | Display |
|--------|-------------|---------|
| **Success rate** | % of items that succeeded | Per job, trailing average |
| **Error rate** | % of items that failed | Trend indicator (↑↓→) |
| **Active jobs** | Currently running | Count |
| **Queue depth** | Jobs waiting | Count (if queuing exists) |

### Progress Metrics

| Metric | Description | Display |
|--------|-------------|---------|
| **Completion %** | Progress through current job | Progress bar |
| **ETA** | Estimated time to completion | Duration |
| **Elapsed** | Time since job started | Duration |

### Trend Metrics

| Metric | Description | Display |
|--------|-------------|---------|
| **Duration trend** | Is this job faster/slower than average? | Comparison to baseline |
| **Error trend** | Are errors increasing? | Sparkline or indicator |
| **Throughput trend** | Is processing speeding up or slowing? | Comparison |

### Activity Feed

| Event | Description |
|-------|-------------|
| **Job started** | "parse_fix_logs started" |
| **Job completed** | "parse_fix_logs completed (1,247 rows, 2m 34s)" |
| **Job failed** | "export_tca failed: permission denied" |
| **Milestone** | "10,000 rows processed" |
| **Warning** | "Parse rate dropped below 100/sec" |

---

## Dual-View Proposal: Monitor vs Debug

The Jobs panel should have TWO modes:

### Tab 1: Monitor (Default for healthy system)

```
┌─ JOBS ─────────────────────────────────────────────────────────────────────┐
│  [Monitor]  [Debug]                                    Last updated: 14:35 │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│  ┌─ AGGREGATE STATS ────────────────────────────────────────────────────┐  │
│  │  Today:  45,231 rows  │  127 files  │  98.2% success  │  3 running   │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  ┌─ ACTIVE JOBS ────────────────────────────────────────────────────────┐  │
│  │  parse_fix_logs     ████████░░░░  67%   1,234 rows   ETA 2m         │  │
│  │  export_concordance ██████████░░  83%   890 docs     ETA 45s        │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  ┌─ RECENT ACTIVITY ────────────────────────────────────────────────────┐  │
│  │  14:34  ✓ scan_archive completed      1,247 files    5m 12s         │  │
│  │  14:32  ✓ parse_csv_batch completed   8,923 rows     1m 45s         │  │
│  │  14:28  ⚠ backtest_v2 plateaued       89% pass rate  iter 5         │  │
│  │  14:25  ✗ export_tca failed           permission denied              │  │
│  │  14:20  ✓ parse_fix_logs completed    12,450 rows    3m 22s         │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  [Tab] switch view  [Enter] job details  [r] refresh  [?] help            │
└────────────────────────────────────────────────────────────────────────────┘
```

**Monitor view shows:**
- Aggregate stats bar (today's totals)
- Active jobs with progress, throughput, ETA
- Recent activity feed (reverse chronological)
- Warnings/failures bubble up but don't dominate

### Tab 2: Debug (Default when failures exist)

```
┌─ JOBS ─────────────────────────────────────────────────────────────────────┐
│  [Monitor]  [Debug]                                    3 issues need attention │
├────────────────────────────────────────────────────────────────────────────┤
│                                                                            │
│  Filter: [All] [Scan] [Parse] [Export] [Backtest]  [x] Failed only        │
│                                                                            │
│  ┌─ NEEDS ATTENTION ────────────────────────────────────────────────────┐  │
│  │  ✗ parse_fix_logs    PARTIAL   12 failed    ValueError (date)  [→]  │  │
│  │  ✗ export_tca        FAILED    permission denied               [→]  │  │
│  │  ⚠ backtest_v2       PLATEAU   89% (stuck)  5 iterations       [→]  │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  ┌─ ALL JOBS ───────────────────────────────────────────────────────────┐  │
│  │  ↻ export_concordance  RUNNING   83%        ETA 45s                  │  │
│  │  ✓ scan_archive        COMPLETE  1,247      5m 12s                   │  │
│  │  ✓ parse_csv_batch     COMPLETE  8,923      1m 45s                   │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
│  [b]acktest  [w]orkbench  [r]etry  [s]kip  [Enter] details  [?] help      │
└────────────────────────────────────────────────────────────────────────────┘
```

**Debug view shows:**
- "Needs attention" section at top (failures, warnings)
- All jobs list with filters
- Action-oriented shortcuts
- Focus on resolving issues

---

## Automatic View Switching

**Smart default:**
- If 0 failures/warnings → Show Monitor view
- If ≥1 failure/warning → Show Debug view

**User can always override** by clicking tab or pressing `[Tab]`.

**Visual indicator:**
- Monitor tab shows green dot when healthy
- Debug tab shows red badge with count when issues exist

```
[Monitor ●]  [Debug (3)]
```

---

## Detailed Metrics Breakdown

### Rows Published (Critical Metric)

Users care deeply about output volume. Show it prominently.

**Per-job display:**
```
parse_fix_logs  ████████░░  67%  │  12,450 rows  │  523 rows/sec
```

**Aggregate display:**
```
Today: 145,231 rows published  │  This week: 1.2M rows
```

**Per-parser breakdown (in detail view):**
```
Parser Performance Today:
  fix_parser v1.2.0      45,231 rows    98.2% success    412 rows/sec avg
  csv_parser v2.0.1      89,000 rows    100% success     890 rows/sec avg
  legacy_parser v0.9.0   11,000 rows    87.3% success    125 rows/sec avg
```

### Duration and Performance

Show both actual duration and comparison to baseline:

```
parse_fix_logs  Duration: 2m 34s (typical: 2m 45s) ✓ 7% faster
export_tca      Duration: 45s (typical: 30s) ⚠ 50% slower
```

### Activity Feed Entries

Each entry should be self-contained:

```
14:34  ✓ parse_fix_logs completed
         1,247 files → 45,231 rows  │  2m 34s  │  98.2% success

14:28  ⚠ backtest_v2 plateaued at 89%
         Parser: fix_parser v1.2.0  │  5 iterations  │  [View details]

14:25  ✗ export_tca failed
         Error: Permission denied: /output/tca.csv  │  [Retry] [Debug]
```

---

## View-Specific Keybindings

### Monitor View

| Key | Action |
|-----|--------|
| `Tab` | Switch to Debug view |
| `Enter` | Open job details |
| `r` | Refresh metrics |
| `t` | Toggle time range (today/week/all) |
| `p` | Show parser breakdown |

### Debug View

| Key | Action |
|-----|--------|
| `Tab` | Switch to Monitor view |
| `Enter` | Drill into job/error |
| `b` | Backtest selected file |
| `w` | Open in Workbench |
| `r` | Retry failed |
| `s` | Skip file |
| `f` | Toggle "failed only" filter |

---

## Data Requirements (Additional)

### Job Metrics Table

```sql
CREATE TABLE cf_job_metrics (
    job_id TEXT PRIMARY KEY REFERENCES cf_jobs(job_id),

    -- Throughput
    rows_published INTEGER DEFAULT 0,
    files_processed INTEGER DEFAULT 0,

    -- Performance
    rows_per_second REAL,
    peak_rows_per_second REAL,

    -- Timing
    duration_ms INTEGER,
    baseline_duration_ms INTEGER,  -- historical average for this parser

    -- Calculated
    success_rate REAL,
    performance_ratio REAL  -- actual/baseline
);
```

### Activity Log Table

```sql
CREATE TABLE cf_activity_log (
    event_id TEXT PRIMARY KEY,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    event_type TEXT NOT NULL,  -- job_started, job_completed, job_failed, milestone, warning
    job_id TEXT REFERENCES cf_jobs(job_id),

    -- Event details
    summary TEXT NOT NULL,     -- One-line summary
    details TEXT,              -- JSON with additional context

    -- For aggregation
    rows_affected INTEGER,
    files_affected INTEGER
);
```

### Aggregate Stats Query

```sql
-- Today's stats
SELECT
    SUM(rows_published) as total_rows,
    COUNT(DISTINCT job_id) as total_jobs,
    SUM(files_processed) as total_files,
    AVG(success_rate) as avg_success_rate,
    COUNT(*) FILTER (WHERE status = 'running') as active_jobs
FROM cf_jobs
JOIN cf_job_metrics USING (job_id)
WHERE started_at >= date('now');
```

---

## Summary: Two Complementary Views

| Aspect | Monitor View | Debug View |
|--------|--------------|------------|
| **Purpose** | "How's it going?" | "What's broken?" |
| **Focus** | Throughput, progress, activity | Failures, errors, fixes |
| **Default when** | No issues | Issues exist |
| **Key metrics** | Rows, files, success rate, ETA | Error details, stack traces |
| **Actions** | View details, refresh | Retry, skip, workbench |

Both views access the same underlying data. The difference is **presentation and priority**.

---

## New Gaps

| ID | Description | Priority |
|----|-------------|----------|
| GAP-MON-001 | How far back does activity feed go? Pagination? | MEDIUM |
| GAP-MON-002 | Should aggregate stats be configurable (today/week/custom)? | LOW |
| GAP-MON-003 | How to handle very long-running jobs in monitor view? | MEDIUM |
