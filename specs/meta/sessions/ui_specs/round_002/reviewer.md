## Review: GAP-STUB-002

### Verdict
**NEEDS_REVISION**

---

### Critical Issues

#### 1. Database Schema Assumptions - Tables Do Not Exist

The spec references tables and columns that do not exist in the current codebase:

**cf_job_logs table (Sections 7.3, 7.4, 8.3):**
- The spec assumes a `cf_job_logs` table for log storage
- No such table exists - searched all Rust code, no references found
- This is correctly identified as GAP-DB-SCHEMA but should be CRITICAL since the log viewer is a core feature

**cf_job_status columns (Section 7.2):**
- Spec assumes columns: `error_type`, `error_message`, `error_file`, `error_row`, `error_column`
- Spec assumes columns: `circuit_breaker_failure_rate`, `circuit_breaker_threshold`, `circuit_breaker_consecutive_failures`, `circuit_breaker_resume_at`
- Current codebase has `cf_job_failures` table (separate from `cf_job_status`)
- Circuit breaker exists in `casparian_sentinel` but column presence in DB is unverified

**Resolution Required:** Either:
1. Document these as Phase 2 features with stub/placeholder data model, OR
2. Align spec with actual database schema (check `cf_job_failures`, `cf_processing_history`)

#### 2. Job State Mismatch with Existing Codebase

The spec uses status `complete` but the existing code uses `ProcessingStatus` enum from `casparian_protocol`. Need to verify:
- Is it `complete` or `completed`?
- Is it `cancelled` or `canceled`?

In `job.rs` line 100, we see `JobAction::Retry` - need to check actual status values.

**Resolution Required:** Audit `ProcessingStatus` enum and align spec with actual values.

---

### High Priority

#### 3. Inconsistency with parser_bench.md Job States

**parser_bench.md** Section 5.2 defines parser states:
- `Healthy`, `Unknown`, `Warning`, `Paused`, `Broken`

**jobs.md** (proposed) Section 4 defines job states:
- `Queued`, `Running`, `Complete`, `Failed`, `Cancelled`, `Paused`

These are different entities (parser health vs job status), but the spec should clarify:
- How does parser health relate to job status?
- If a parser is `Paused` (circuit breaker), what happens to its jobs?
- The spec mentions "Resume paused job" but parser_bench mentions "Resume paused parser" - are these the same action?

**Section 2.7 (Resume workflow)** references circuit breaker but this is parser-level in parser_bench. The spec conflates parser-level circuit breaker with job-level pause.

#### 4. `n` Key Conflict

**tui.md Section 3.3** defines `n` as "New / Create" across views.
**jobs.md Section 5.2** defines `n` in Log Viewer as "Next match" (search navigation).

While this is technically different contexts (view state vs modal state), it may cause user confusion. The log search uses `n/N` for next/previous match, which aligns with vim convention, but conflicts with the global "common action" pattern.

**Recommendation:** Document this explicitly or use `Ctrl+n` for search navigation to avoid conflict.

#### 5. Missing `u` (Resume) in Log Viewer and Filter Dialog

Section 4.3 shows `u` transitions from JOB_LIST to CONFIRM_DIALOG for resume.
However, if user is in LOG_VIEWER viewing a paused job's logs, `u` is not available to resume.

Should `u` be available in all states when viewing a paused job?

---

### Medium Priority

#### 6. Filter Values Inconsistency

**Section 5.1 Filter Dialog** shows options including `Queued`, `Cancelled`, `Paused`.
**Section 6.1 JobFilter enum** uses `Complete` (singular).
**Section 6.2 JobStatus enum** uses `Complete` (singular).
**Section 7.2 SQL** uses `'complete'` (lowercase).

The spec should be consistent:
- UI labels: "Completed" (user-friendly)
- Enum variants: `Complete` (Rust convention)
- SQL values: `'complete'` (lowercase, matches processing protocol)

But Section 2.5 workflow shows "Completed (47)" in UI which is inconsistent with `JobFilter::Complete`.

**Fix:** Standardize to `Complete` in code, "Completed" in UI display.

#### 7. Query Correctness Issue

**Section 7.2** query:
```sql
LEFT JOIN cf_parsers p ON j.parser_name = p.name
```

But `cf_parsers` likely has multiple versions of the same parser name. This join would create duplicates or return arbitrary version. Should be:

```sql
LEFT JOIN cf_parsers p ON j.parser_id = p.id
```

Or if using name-based join, need version matching:
```sql
LEFT JOIN cf_parsers p ON j.parser_name = p.name AND j.parser_version = p.version
```

#### 8. Missing State Transition: Error During Confirm

Section 4.3 shows transitions for CONFIRM_DIALOG, but what if the action fails?
- User presses Enter to cancel job
- Cancel fails (job already completed, network error, etc.)
- Spec doesn't define: does it stay in CONFIRM_DIALOG? Return to JOB_LIST with error toast?

**Recommendation:** Add transition: `CONFIRM_DIALOG | Action failed | JOB_LIST | Show error toast`

#### 9. Refresh Interval Conflict with Global `r`

**Section 8.1** says `r` is global keybinding for manual refresh.
**Section 5.1** says `r` is "Retry job" in Job List State.

This is a direct conflict. The global `r` (refresh) from tui.md Section 3.3 conflicts with view-specific `r` (retry).

**Resolution Options:**
1. Use `Shift+R` for retry (matches pattern in parser_bench: `R` for Resume)
2. Use `t` for retry (mnemonic: "try again")
3. Override global `r` in this view (requires documenting exception)

---

### Low Priority / Nits

#### 10. Duration Type Mismatch

**Section 8.4** `calculate_eta()` returns `Option<Duration>` but uses:
```rust
let eta_secs = (remaining as f64 / rate) as i64;
Some(Duration::seconds(eta_secs))
```

`Duration::seconds()` is from `chrono`, not `std::time::Duration`. The spec mixes types - should clarify which `Duration` type is used (chrono or std).

#### 11. Inconsistent Section Numbering

home.md has Section 8 "Implementation Notes" with subsections 8.1-8.7.
jobs.md (proposed) has Section 8 "Implementation Notes" with subsections 8.1-8.8.

This is fine, but jobs.md is missing a "Data Sources" section number - it's Section 7, but home.md also has Section 7 for Data Sources. Good consistency here.

#### 12. Missing View Trait Methods

**Section 8.7** shows `View` trait implementation but is missing:
- `tick()` method for timer-based updates (needed for 500ms refresh)
- How does `has_running_jobs()` trigger re-render without tick?

The refresh strategy (Section 8.1) implies a tick mechanism but doesn't show implementation.

#### 13. Log Level Colors Redundancy

**Section 6.3** `LogLevel::color()` duplicates the semantic color mapping from tui.md Section 5.1. Could reference master spec instead of defining custom colors:
- Warning = Yellow (matches tui.md)
- Error = Red (matches tui.md)

#### 14. Typo in Layout

**Section 3.1** shows `[u] Resume` in footer but only applies when paused job is selected. Footer hints should be context-sensitive per tui.md Section 6.2.

---

### Compression Opportunities

#### 1. Status Indicator Pattern

Both jobs.md and home.md define similar status indicators:
- home.md Section 3.3: `Healthy/Empty/InProgress/Error/Warning`
- jobs.md Section 3.3: `Running/Complete/Failed/Queued/Cancelled/Paused`

These could be unified in tui.md as a "Status Indicator Library" with:
- Generic indicators: `Active`, `Inactive`, `Progress`, `Success`, `Error`, `Warning`
- View-specific mappings

#### 2. Dialog Pattern

Both home.md and jobs.md define dialog states. The dialog pattern could be extracted to tui.md Section 4.4 as a reusable component specification.

#### 3. List Navigation

Section 5.1 keybindings for list navigation (`j/k`, `g/G`) duplicates tui.md Section 3.2. Could simply say "Standard list navigation (see tui.md 3.2)".

---

### Summary

The Engineer's expansion is comprehensive and well-structured, closely following the template established by home.md. The workflows are clear, the state machine is mostly complete, and the implementation notes provide good guidance.

However, there are **two critical issues** that require revision:

1. **Database schema assumptions** - The spec depends on tables (`cf_job_logs`) and columns (circuit breaker fields, error details) that don't exist in the current codebase. This needs to be either aligned with reality or explicitly marked as "requires schema migration" with a GAP reference.

2. **Job state enum mismatch** - Need to verify actual `ProcessingStatus` values and align the spec.

Additionally, the **`r` keybinding conflict** (retry vs refresh) is a high-priority issue that needs resolution before implementation.

Once these issues are addressed, the spec will be ready for approval.

**Recommended Actions:**
1. Audit actual database schema for `cf_job_status` and related tables
2. Check `ProcessingStatus` enum in `casparian_protocol`
3. Resolve `r` key conflict with explicit decision
4. Clarify relationship between parser-level circuit breaker (parser_bench) and job-level pause (jobs)
5. Add error handling transition for failed dialog actions
