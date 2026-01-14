# Round 001 Synthesis

**Date:** 2026-01-13
**Status:** REVISE REQUIRED

---

## Summary

The Engineer proposed two foundational insights:
1. **Output-centric design** - Users care about "where is my output, is it ready?"
2. **Data-states model** - Users think in SOURCE → PROCESSED → OUTPUT states, not jobs

The Reviewer accepted these as **directionally useful hypotheses** but identified critical flaws preventing acceptance.

---

## Blocking Issues Requiring Resolution

### CRITICAL

| Issue | Problem | Impact |
|-------|---------|--------|
| **ISSUE-R1-001** | Monitoring vs debugging workflows conflated | UI can't optimize for both if treated as one use case |
| **ISSUE-R1-006** | Backtest doesn't fit data-states model | Model fails for 25% of job types (Scan, Parse, Export, **Backtest**) |

### HIGH

| Issue | Problem | Impact |
|-------|---------|--------|
| **ISSUE-R1-007** | Linear model assumes happy path | Real workflows branch (multiple parsers), cycle (retries), and loop (Backtest iteration) |
| **ISSUE-R1-011** | UI architecture unclear | Top bar (pipeline) and body (status groups) may conflict or duplicate |

---

## Key Questions for User Decision

### Q1: Primary Use Case
The Engineer claims users primarily want "output location + readiness" (monitoring).
The Reviewer argues debugging (failure investigation) may be where users spend more time.

**Decision needed:** What is the PRIMARY use case for Jobs view?
- A) **Monitoring** - "Is my output ready?" (Output prominence)
- B) **Debugging** - "Why did it fail? How do I fix it?" (Error prominence)
- C) **Both equally** - Design must serve both without compromise

### Q2: Backtest Fit
Backtest is a validation loop, not a data-state transition. It doesn't fit SOURCE → PROCESSED → OUTPUT.

**Decision needed:** How should Backtest appear in Jobs view?
- A) **Separate category** - Backtest is not a "job" in the same sense, show differently
- B) **Extended model** - Add "VALIDATED" state between PROCESSED and OUTPUT
- C) **Override model** - Backtest loops within PROCESSED state (iteration counts, pass rate)

### Q3: View Naming
If the view is "output-centric," calling it "Jobs" creates confusion.

**Decision needed:** Should we rename the view?
- A) **Keep "Jobs"** - Users expect it, design within that expectation
- B) **Rename to "Data"** - Aligns with data-states model
- C) **Rename to "Outputs"** - Aligns with output-centric design

### Q4: UI Architecture
The proposed mockup shows:
- Top bar: Pipeline summary (SOURCE → PROCESSED → OUTPUT counts)
- Body: Status groups (READY / IN PROGRESS / NEEDS ATTENTION)

**Decision needed:** Relationship between top bar and body?
- A) **Complementary** - Top bar is progress overview, body is actionable list
- B) **Drill-down** - Click pipeline stage to filter body
- C) **Redundant** - Remove top bar, status groups are sufficient

---

## Round 2 Requirements

If proceeding to Round 2, Engineer must:

1. **Separate monitoring from debugging** - Define distinct UI paths for each
2. **Resolve Backtest** - Either extend model or acknowledge limitation
3. **Handle non-linear flows** - Show how multiple parsers, retries work
4. **Clarify UI hierarchy** - Top bar vs body relationship

---

## Gaps Status After Round 1

| ID | Description | Status |
|----|-------------|--------|
| GAP-CORE-001 | Core value proposition | **PROPOSED** (needs revision) |
| GAP-MENTAL-001 | User mental model | **PROPOSED** (needs revision) |

**New gaps introduced:** 4
- GAP-OUTPUT-001, GAP-OUTPUT-002, GAP-STATE-001, GAP-STATE-002

**Issues raised:** 13
- 2 CRITICAL, 4 HIGH, 7 MEDIUM
