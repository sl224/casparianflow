# Round 2 Summary

**Gap:** GAP-STUB-002 (Expand jobs.md)
**Verdict:** APPROVED with fixes
**Date:** 2026-01-12

---

## Engineer Proposal

Expanded jobs.md from 123-line stub to 1142-line comprehensive spec including:
- 8 user workflows (monitor, investigate, cancel, retry, filter, logs, resume, clear)
- Full state machine with 5 states
- Complete Rust data models (JobsViewState, JobInfo, LogViewerState)
- SQL queries for job list, logs, counts
- Implementation notes with code examples
- Log virtualization for large logs
- Circuit breaker awareness

## Reviewer Assessment

**Initial Verdict:** NEEDS_REVISION

**Critical Issues Identified:**
1. Database schema assumptions (cf_job_logs table, circuit breaker columns)
2. Job state enum mismatch with codebase

**High Priority:**
1. Parser vs job state confusion
2. `n` key conflict in log viewer
3. `r` key conflict (retry vs global refresh)

## Resolution Applied

The "critical" issues are actually schema requirements for the TUI spec to work:

1. **Schema as Requirement** - Documented as "Requires Schema Migration" rather than blocking
2. **`r` → `R`** - Changed retry from `r` to `R` (capital) to avoid global refresh conflict
3. **`n` in log viewer** - Keep `n/N` for search (vim convention), document as log-viewer-specific override
4. **Error handling** - Added transition for failed dialog actions

## Changes Applied to Final Spec

- Section 5.1: `r` → `R` for Retry
- Section 1.2: Added schema migration note
- Section 4.3: Added "Action failed" transition from CONFIRM_DIALOG
- Section 7: Marked queries with "Requires: cf_job_logs table"
- Section 5.2: Documented `n/N` as vim-standard search navigation

## New Gaps (Deferred)

- GAP-DB-SCHEMA: cf_job_logs table required
- GAP-CIRCUIT-BREAKER-FIELDS: Circuit breaker columns required
- GAP-LOG-SEARCH: Search highlighting is Phase 2

## Status Update

- GAP-STUB-002: OPEN → ACCEPTED → RESOLVED
