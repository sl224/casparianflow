# Reviewer Response - Round 003

**Date:** 2026-01-13
**Reviewing:** Monitoring workflow and dual-view proposal
**Reviewer:** Claude (Opus 4.5)

---

## Overall Assessment: ACCEPT

The dual-view proposal (Monitor vs Debug) elegantly solves the "monitoring AND debugging" tension. The smart default (show Debug when issues exist) is good UX.

---

## What Works Well

1. **Dual-view concept is sound**
   - Monitor for "how's it going"
   - Debug for "what's broken"
   - Clear separation of concerns

2. **Smart defaults**
   - Auto-switch to Debug when failures exist
   - User can override
   - Visual badge shows issue count

3. **Metrics are comprehensive**
   - Rows published (users care about this)
   - Success rate
   - Duration with baseline comparison
   - Activity feed with context

4. **Activity feed design**
   - Self-contained entries
   - Actionable (inline retry/debug buttons)
   - Shows outcome metrics

---

## Minor Issues (Non-blocking)

### ISSUE-R3-001: Tab naming
- "Monitor" and "Debug" are functional but clinical
- Alternatives: "Activity" / "Issues", "Dashboard" / "Troubleshoot"
- **Recommendation:** Keep as proposed - clear beats clever

### ISSUE-R3-002: Aggregate stats time range
- Fixed to "today" - what if user wants weekly view?
- **Recommendation:** Add `[t]` toggle as proposed, start with Today/Week/All

### ISSUE-R3-003: Rows published placement
- Shown in aggregate bar and per-job - may feel redundant
- **Recommendation:** Keep both - aggregate for "big picture", per-job for "this specific run"

### ISSUE-R3-004: Activity feed depth
- "How far back" punted to gap
- **Recommendation:** Last 50 events or 24 hours, whichever is more. Paginate beyond.

---

## Verification Questions

1. **Baseline duration calculation** - How do we compute "typical" duration? Average of last N runs? Per-parser? Per-input-size?

2. **Rows published accuracy** - Does the worker shim report rows as they're written, or only at job completion?

3. **Real-time updates** - Does Monitor view poll or stream updates? Polling interval?

---

## Recommendation: ACCEPT

The dual-view proposal is ready. The three verification questions should be addressed during implementation, not spec refinement.

**Summary of Jobs View (Final):**

```
Jobs Panel
├── [Monitor] tab (default when healthy)
│   ├── Aggregate stats bar
│   ├── Active jobs with progress + throughput
│   └── Recent activity feed
│
├── [Debug] tab (default when issues)
│   ├── "Needs attention" section
│   ├── Filtered job list
│   └── Action shortcuts (retry, workbench, skip)
│
└── Shared: Job Detail → Error Detail drill-down
```

This covers both monitoring and debugging use cases with appropriate defaults.
