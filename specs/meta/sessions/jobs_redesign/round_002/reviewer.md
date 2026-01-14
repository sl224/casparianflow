# Review: Round 002

**Reviewer:** Claude (Opus 4.5)
**Date:** 2026-01-13
**Reviewing:** /specs/meta/sessions/jobs_redesign/round_002/engineer.md

---

## Summary

The Engineer has addressed all three user decisions (pipeline visualization, backtest job type, full monitoring panel) with comprehensive proposals. The data flow architecture via database polling is sound and aligns with existing patterns. However, there are several technical gaps and inconsistencies that need resolution before implementation.

---

## Critical Issues

### CRIT-001: Missing `file_version_id` in JobInfo struct

The proposed `JobInfo` struct (line 254-274) does not include `file_version_id`, but the existing `cf_processing_queue` schema requires it as a foreign key:

```sql
-- Existing schema (demo/schema.sql:62)
file_version_id INTEGER NOT NULL,
```

The Engineer's proposed fetch queries (lines 779-790, 804-817) reference `file_version_id` but the Rust struct has no field for it.

**Fix:** Add `file_version_id: i64` to `JobInfo` and clarify how it relates to parse jobs (single file) vs scan/backtest jobs (multiple files).

### CRIT-002: Conflicting state machine scope

The Engineer proposes adding MONITORING as a state transition from JOB_LIST (line 659-678), but this conflicts with the existing TUI architecture where each mode (Home, Discover, ParserBench, Jobs) is a separate `TuiMode` enum variant (app.rs lines 21-29).

The proposal shows:
```
JOB_LIST --'M'--> MONITORING
```

But should Monitoring be:
1. A sub-state of Jobs mode (like DETAIL_PANEL)?
2. A separate TuiMode?
3. An overlay dialog (like RULES_MANAGER in Discover)?

**Fix:** Clarify whether `MonitoringPanel` is a TUI mode or a Jobs sub-state. Recommend treating it as a sub-state (like DetailPanel) for consistency with existing patterns.

---

## High Priority

### HIGH-001: Pipeline summary query references incorrect join logic

Line 779-790 queries parsed files:
```sql
SELECT COUNT(DISTINCT file_version_id) FROM cf_processing_queue
WHERE job_type = 'PARSE' AND status = 'COMPLETED'
```

This is correct. However, the issue is that the pipeline summary assumes one job per file, but in reality:
- A file can have multiple parse jobs (different parsers)
- Backtest jobs test one parser against many files

**Fix:** Clarify the aggregation logic. Consider whether "parsed files" means unique files or unique (file, parser) combinations.

### HIGH-002: Backtest `items_total` semantics unclear

For `BacktestJobInfo` (lines 277-285), the `items_processed` and `items_total` inherited from `JobInfo` are used for file counts. But backtest also has:
- `pass_rate: f64`
- `high_failure_tested: u32`
- `high_failure_passed: u32`

This creates confusion: what does `items_failed` mean for a backtest? Is it files that failed parsing, or files that did not match the schema?

**Fix:** Add explicit documentation that for Backtest jobs:
- `items_processed` = files tested
- `items_failed` = files that failed validation (not parse errors)
- `pass_rate` = `(items_processed - items_failed) / items_processed`

### HIGH-003: Keybinding conflicts

The proposal adds these keybindings:
- `M` - Monitoring panel (line 401)
- `P` - Toggle pipeline (line 849)
- `S` - Stop backtest (line 975)

But the existing TUI uses:
- `M` in Discover mode - Sources Manager (discover.md line 205)
- Global mode switching uses number keys (1-4)

Need to verify `M` and `P` are not used in Jobs mode. Also `S` conflicts with potential "Sort" functionality common in list views.

**Fix:** Either:
1. Use `m` (lowercase) for monitoring since `M` pattern exists in Discover
2. Document that Jobs mode has its own keymap distinct from Discover
3. Choose a different key for monitoring (e.g., `?` for status, or `Shift+M`)

### HIGH-004: Missing `started_at` timestamp in existing schema

The existing `cf_processing_queue` has no `started_at` column (see demo/schema.sql:59-74). The Engineer proposes adding `updated_at` but the `JobInfo` struct requires `started_at: DateTime<Utc>` (line 259).

The existing schema has:
- `claim_time TEXT` - when worker claimed the job
- `end_time TEXT` - when job completed

**Fix:** Either:
1. Map `claim_time` to `started_at` in the Rust code (recommended - no schema change)
2. Or add explicit `ALTER TABLE` for `started_at` column

---

## Medium Priority

### MED-001: Sparkline rendering uses inconsistent character range

Lines 535-558 use ASCII characters for sparklines:
```rust
const BLOCKS: [char; 5] = ['.', '_', '-', '=', '#'];
```

This is inconsistent with existing TUI patterns. The status bar in source.md line 36 uses Unicode:
```
\u21bb 1 running   \u2713 3 done   \u2717 1 failed
```

**Suggestion:** Consider Unicode block characters for visual consistency:
```rust
const BLOCKS: [char; 8] = [' ', '\u2581', '\u2582', '\u2583', '\u2584', '\u2585', '\u2586', '\u2587'];
```

### MED-002: Retention cleanup gap not addressed

GAP-RETENTION-001 is noted (line 202) but marked as "for future rounds." However, without retention cleanup, `cf_job_metrics` will grow unbounded during long processing jobs.

**Suggestion:** Add a simple cleanup query to run every poll cycle:
```sql
DELETE FROM cf_job_metrics
WHERE metric_time < datetime('now', '-5 minutes')
```

### MED-003: Worker heartbeat tracking deferred

GAP-WORKER-001 (line 691) is critical for the Workers panel in monitoring (lines 420-430). Without heartbeat data, the Workers section will be empty.

**Suggestion:** Add `cf_worker_heartbeat` table to the schema now, even if population is deferred:
```sql
CREATE TABLE cf_worker_heartbeat (
    worker_id TEXT PRIMARY KEY,
    job_id INTEGER,
    last_heartbeat TEXT,
    status TEXT
);
```

### MED-004: Pipeline collapsed-by-default may conflict with user intent

Line 849 states "Pipeline summary is collapsed by default (jobs-first philosophy)." However, the user explicitly requested reopening pipeline visualization (decisions.md line 16).

**Suggestion:** Consider showing pipeline expanded on first visit, then remembering user preference. Or at minimum, make the default configurable.

---

## Low Priority / Nits

### NIT-001: Inconsistent job type symbols

The Engineer proposes symbols (lines 228-232):
```rust
Scan => "S", Parse => "P", Export => "E", Backtest => "B"
```

But the existing source.md uses full words with status symbols:
```
\u2717 PARSE    fix_parser v1.2
```

The single-letter symbols appear nowhere in the ASCII layouts. Clarify if these are for internal use only or if they should appear in the UI.

### NIT-002: Backtest stop action is destructive without confirmation

Line 365-366 shows:
```
[S] Stop backtest  [l] Logs  [Esc] Close
```

The `S` key stopping a backtest is potentially destructive (loses progress). Consider `Ctrl+C` or requiring confirmation dialog.

### NIT-003: Query result types need explicit casting

Lines 568-579 use tuples for query results:
```rust
let queue_stats: (i64, i64, i64, i64) = sqlx::query_as(...)
```

This works but is fragile. Consider using `query_scalar` with named fields or a dedicated struct for maintainability.

### NIT-004: Date parsing format mismatch

Line 608 parses RFC3339:
```rust
DateTime::parse_from_rfc3339(&time_str)
```

But SQLite `datetime()` returns `YYYY-MM-DD HH:MM:SS` format, not RFC3339. This will fail at runtime.

**Fix:** Use appropriate SQLite datetime parsing:
```rust
chrono::NaiveDateTime::parse_from_str(&time_str, "%Y-%m-%d %H:%M:%S")
```

---

## Recommendation

**NEEDS_REVISION**

The proposal is comprehensive and directionally correct. However, the following must be resolved before implementation:

1. **CRIT-001**: Add `file_version_id` to JobInfo or clarify the relationship for multi-file jobs
2. **CRIT-002**: Clarify Monitoring as sub-state vs TuiMode (recommend sub-state)
3. **HIGH-003**: Resolve keybinding conflicts with existing TUI patterns
4. **HIGH-004**: Map existing `claim_time` to `started_at` (no schema change needed)
5. **NIT-004**: Fix datetime parsing format (will cause runtime errors)

Once these are addressed, the spec is ready for implementation.
