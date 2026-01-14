# Round 002 Synthesis

**Date:** 2026-01-13
**Status:** ACCEPT WITH MODIFICATIONS

---

## Summary

Round 2 addressed the deep debugging workflow. The Reviewer rated it **ACCEPT WITH MODIFICATIONS** - substantially complete with three items requiring clarification.

### What's Settled

1. **Three-level information hierarchy**
   - Jobs List → Job Detail → Error Detail
   - Each level has appropriate depth

2. **Job-type-specific rendering**
   - Parse: stack trace, input context, stderr
   - Scan: path, permission error, suggestions
   - Export: failed records, destination issues
   - Backtest: pass rate, iteration, failure breakdown

3. **Contextual navigation**
   - "Jump to Workbench" passes error context
   - Views receive WHY you navigated there

4. **Database schema**
   - Jobs, job_failures, job_logs tables
   - Supports the information needs

### What Needs Decisions

Three implementation feasibility questions:

| Question | Options |
|----------|---------|
| **Input context capture** | Parser reports line, OR framework tracks, OR best-effort search |
| **Stack trace availability** | Always (exceptions only) vs explicit "not available" for schema violations |
| **Partial success status** | Add PARTIAL status vs treat as FAILED with count |

---

## Open Questions for User

### Q1: Input Context Capture

When a parser fails on line 4523 of an input file, how do we know which line?

- **A) Parser responsibility** - Parser must call `ctx.set_current_line(4523)` as it reads
- **B) Framework tracking** - Framework wraps file reads, tracks automatically (overhead)
- **C) Best-effort** - Search input file for pattern that matches error (may fail)

### Q2: Stack Trace Availability

Stack traces are only available for Python exceptions. Schema violations (detected by framework) don't have them.

- **A) Show "N/A"** - Display "No stack trace - schema violation detected by framework"
- **B) Synthesize** - Framework generates pseudo-trace showing what check failed
- **C) Omit section** - Only show stack trace section when available

### Q3: Partial Success Status

A job with 1,235 successes and 12 failures - is it FAILED or something else?

- **A) PARTIAL status** - New status between COMPLETE and FAILED
- **B) FAILED with count** - "FAILED (12)" - status is FAILED, count shows extent
- **C) COMPLETE with warnings** - "COMPLETE ⚠️ 12 errors" - focus on what succeeded

---

## Gaps Remaining

| ID | Description | Priority | Status |
|----|-------------|----------|--------|
| GAP-DEBUG-002 | Large stderr handling | HIGH | Reviewer suggests: truncate + "show all" button |
| GAP-DEBUG-003 | Retry history tracking | MEDIUM | Open |
| GAP-NAV-001 | Workbench context interface | HIGH | Needs Workbench spec verification |

---

## Recommendation

Round 2 is ready for user decision on the three questions. Once answered, the spec can be updated and moved to implementation.
