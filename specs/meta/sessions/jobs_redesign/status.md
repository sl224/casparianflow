# Jobs Redesign - Refinement Status

**Session Started:** 2026-01-13
**Session Completed:** 2026-01-13
**Source:** specs/views/jobs_redesign.md
**Final Round:** 2
**Status:** COMPLETE

---

## Convergence

| Metric | Value |
|--------|-------|
| Total Gaps | 8 |
| Open | 0 |
| Resolved | 6 |
| Deferred | 2 |
| State | COMPLETE |

---

## Gap Inventory

### CRITICAL (Weight: 16)

| ID | Description | State | Resolution |
|----|-------------|-------|------------|
| GAP-DATA-001 | No specification for how job data flows from workers to TUI | RESOLVED | Section 9: Database polling with change detection |

### HIGH (Weight: 4)

| ID | Description | State | Resolution |
|----|-------------|-------|------------|
| GAP-MONITOR-001 | Monitoring approach unclear | RESOLVED | Section 5: Full monitoring panel per user decision |
| GAP-STATE-001 | State machine incomplete | RESOLVED | Section 6: Complete state machine with all states |
| GAP-RENDER-001 | Job rendering line widths | DEFERRED | Implementation detail, not blocking |

### MEDIUM (Weight: 2)

| ID | Description | State | Resolution |
|----|-------------|-------|------------|
| GAP-ACTION-001 | Contextual actions undefined | RESOLVED | Section 7: Full keybinding table |
| GAP-FILTER-001 | Filter dialog behavior | DEFERRED | Implementation detail |
| GAP-REFRESH-001 | Job list refresh strategy | RESOLVED | Section 9.1: 500ms polling |

### LOW (Weight: 1)

| ID | Description | State | Resolution |
|----|-------------|-------|------------|
| GAP-COPY-001 | Clipboard mechanism | DEFERRED | Platform-specific, not blocking |

---

## Round History

| Round | Gaps Resolved | Gaps Added | Net | State |
|-------|---------------|------------|-----|-------|
| 0 (initial) | - | 8 | - | START |
| 1 | 0 | 0 | 0 | User decisions collected |
| 2 | 6 | 2 (deferred) | +6 | COMPLETE |

---

## Notes

Initial gaps identified from source spec review:

1. **GAP-DATA-001**: Spec defines JobInfo struct but doesn't explain how this data gets populated. Workers run in separate processes - how does job status reach the TUI?

2. **GAP-MONITOR-001**: Section 10 says "inline throughput" but original request was for monitoring view. Need to decide: is inline enough, or does user need more?

3. **GAP-STATE-001**: State machine diagram shows 4 states but LOG_VIEWER and FILTER_DIALOG have no behavioral specification.

4. **GAP-RENDER-001**: ASCII layouts assume ~80 char width but job lines can be long (parser name + path + size + timestamp). What truncates?

5. **GAP-ACTION-001**: Footer shows [q] Query but there's no specification for what Query does.

6. **GAP-FILTER-001**: Filter dialog mentioned but filters not defined.

7. **GAP-REFRESH-001**: Job list is live data but refresh strategy not specified.

8. **GAP-COPY-001**: Minor - terminal clipboard varies by platform.
