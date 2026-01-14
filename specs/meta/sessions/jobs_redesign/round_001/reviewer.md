# Review: Round 001

**Reviewer:** Claude (Opus 4.5)
**Date:** 2026-01-13

---

## Summary

The Engineer proposes reframing the Jobs view from job-centric to output-centric, arguing that users care about "Is my output ready and where is it?" rather than job status. The analysis includes valuable insights about vertical-specific mental models, but fundamentally misreads the source spec's current state and proposes solutions that contradict already-crystallized decisions.

---

## Critical Issues

### ISSUE-R1-001: Proposed design contradicts crystallized decisions in source spec

**Location:** Engineer's "Recommended Design Direction" (lines 181-216)

**Quote:**
```
┌─ DATA STATUS ───────────────────────────────────────────────────┐
│                                                                  │
│  SOURCE            PROCESSED           OUTPUT                   │
│  1,247 files  ->   1,235 parsed   ->   bloomberg-tca (ready)    │
```

**Impact:** The source spec (Section 12) explicitly lists under "What We Explicitly Don't Do":

> Pipeline visualization - Adds complexity, single line is enough context

The Engineer's proposed design IS a pipeline visualization (SOURCE -> PROCESSED -> OUTPUT). This directly contradicts a crystallized v1.0 decision. Implementing this would require reopening and overturning an already-made design decision.

**Suggestion:** The Engineer must either:
1. Work within the existing spec's constraints (job list with output prominence), OR
2. Explicitly acknowledge this requires reopening a crystallized decision, with clear rationale for why the original reasoning ("Adds complexity, single line is enough context") was wrong.

---

### ISSUE-R1-002: Source spec already addresses output-centricity

**Location:** GAP-CORE-001 Analysis (lines 13-78)

**Quote:** "The spec presents 5 UI alternatives without first establishing what success looks like."

**Impact:** This statement applies to revision 0.1, not the current v1.0 spec. The source spec is marked "Ready for Implementation" and has already crystallized around output-prominence:

| Source Spec Section | Output-Centric Feature |
|---------------------|------------------------|
| Section 2 | "Where's my output?" is explicitly listed as a core user question |
| Section 5.2 | Parse job shows `output path` prominently |
| Section 5.5 | Export job shows `output path + record count` |
| Section 6.2 | `y` key copies output path, `o` opens output folder |

The Engineer is solving a problem the source spec already solves. The question is: what SPECIFIC gap remains after acknowledging the existing design?

**Suggestion:** Re-read source spec Sections 2 and 5. Acknowledge that output-centricity is already the design direction. Identify what specific aspect is STILL missing, if any. The current analysis treats a v0.1 draft state as if it were current.

---

## High Priority

### ISSUE-R1-003: Data model incompatibility with existing crate types

**Location:** GAP-MENTAL-001 Proposed Solution (lines 121-167)

**Quote:** "Reframe the entire view from 'Jobs' to 'Data Status'"

**Impact:** The existing `casparian_protocol` crate defines canonical types:

```rust
// From crates/casparian_protocol/src/types.rs
pub enum JobStatus {
    Success,
    Failed,
    Rejected,
    Aborted,
}

pub enum ProcessingStatus {
    Pending,
    // ... queue lifecycle states
}

pub struct JobReceipt {
    pub status: JobStatus,
    pub metrics: HashMap<String, i64>,
    pub artifacts: Vec<HashMap<String, String>>,
    // ...
}
```

The Engineer's "Data States" model (SOURCE/PROCESSED/OUTPUT) has no mapping to these existing types. The source spec's `JobInfo` struct (Section 9) is designed around jobs, not data states. Implementing the proposed model would require:
1. New data structures for state aggregation
2. Mapping layer between job events and state transitions
3. Changes to database schema (`cf_job_status` table)

None of this is addressed.

**Suggestion:** If pursuing data-state model, provide explicit mapping:
- How does `ProcessingStatus::Pending` map to SOURCE state?
- How does `JobStatus::Success` trigger PROCESSED -> OUTPUT transition?
- What aggregation logic combines individual job statuses into state-level metrics?

---

### ISSUE-R1-004: Backtest job excluded from data-state model

**Location:** GAP-STATE-001 (lines 166, 225)

**Quote:** "GAP-STATE-001: How Backtest jobs fit data-state model"

**Impact:** The Engineer correctly identifies Backtest as not fitting the model but defers resolution. Per CLAUDE.md, Backtest is a core architectural concept:

> When validating a parser against many files:
> 1. Test high-failure files first
> 2. If they still fail, stop early (parser not ready)
> 3. If they pass, continue with remaining files

Backtest is validation, not state transition. It doesn't produce "output" in the sense of Parse/Export. If the model fails for a core job type, the model may be wrong.

Interestingly, the source spec (Section 5) only lists SCAN, PARSE, EXPORT as job types - it also doesn't address BACKTEST. This is a real gap in BOTH documents.

**Suggestion:** Either:
1. Extend data-states to include validation states (e.g., SOURCE -> [VALIDATED] -> PROCESSED -> OUTPUT)
2. Acknowledge backtest as a special case outside the main model
3. Define where Backtest jobs appear in the UI (Jobs view? Parser Lab context only?)

---

### ISSUE-R1-005: Conflation of monitoring vs debugging use cases

**Location:** GAP-CORE-001 Analysis (lines 18-45)

**Quote:** Users ask "Is the TCA file ready?" (monitoring) and "Why did 12 files fail?" (debugging)

**Impact:** The analysis identifies TWO distinct workflows but treats them as one:

| Workflow | User Need | UI Priority |
|----------|-----------|-------------|
| Monitoring | "Is my output ready?" | Output location + status |
| Debugging | "Why did it fail?" | Error details + file paths |

These have different UI requirements. "Output-centric" design optimizes monitoring but may sacrifice debugging. The source spec balances both:
- Monitoring: Status bar shows aggregate counts, output paths visible
- Debugging: Failed jobs pinned to top, detail panel shows all failures

The Engineer's proposal doesn't clearly address how both workflows are served.

**Suggestion:** Explicitly separate monitoring from debugging. Define which is primary use case (time spent). Show how the proposed UI serves both without sacrificing either.

---

## Medium Priority

### ISSUE-R1-006: Linear model doesn't match real workflow complexity

**Location:** Data states model (lines 127-135)

**Quote:**
```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│   SOURCE    │  ->  │  PROCESSED  │  ->  │   OUTPUT    │
│  (Files)    │      │  (Parquet)  │      │  (Export)   │
└─────────────┘      └─────────────┘      └─────────────┘
```

**Impact:** Real workflows include:
- Same files through multiple parsers (branching at PROCESSED)
- Multiple exports from one parsed dataset (branching at OUTPUT)
- Retry failed files (backward flow)
- Backtest iteration until parser ready (cycles)

The linear model only fits the happy path. What does "1,235 parsed" mean when files go through 3 different parsers? Which parser's count is shown?

**Suggestion:** Either:
1. Acknowledge this is a simplification for single-parser workflows
2. Extend the model to show branching/cycling
3. Define aggregation logic (e.g., "parsed by ANY parser" vs "parsed by ALL parsers")

---

### ISSUE-R1-007: GAP-STATE-002 already addressed by source spec

**Location:** New gaps table (line 226)

**Quote:** "GAP-STATE-002: How to show partial/in-progress states"

**Impact:** This is already addressed in the source spec:
- Section 5.4: Running parse shows progress bar + file count + ETA
- Section 5.6: Running export shows progress bar + record count + ETA
- Section 4.2: "Each job is 1-3 lines depending on type and status"

This suggests incomplete reading of the source spec. Not all "new gaps" are actually new.

**Suggestion:** Verify each new gap against the source spec before listing. Remove duplicates.

---

### ISSUE-R1-008: Evidence citations cannot be verified

**Location:** Lines 66, 71-72, 74

**Quotes:**
1. "User feedback in Appendix A.2" - Source spec has no Appendix A.2
2. "Vercel succeeds with 'Clean, output-focused'" - No source cited
3. "Section 2.4 notes users typically have ONE source" - Source spec Section 2 has no 2.4

**Impact:** Key evidence supporting the argument cannot be verified against provided documents. Either:
1. These reference external documents not provided, OR
2. These reference an earlier version of the spec, OR
3. These are inaccurate citations

**Suggestion:** Cite only verifiable sources, or explicitly note when referencing external/earlier documents. Current citations undermine credibility of the analysis.

---

## Low Priority / Nits

### ISSUE-R1-009: Confidence ratings inverted

**Location:** Lines 12, 82

**Quote:** GAP-CORE-001 marked "HIGH confidence" while GAP-MENTAL-001 marked "MEDIUM confidence"

**Impact:** GAP-CORE-001 has a fundamental issue (source spec already addresses output-centricity) yet is marked HIGH. GAP-MENTAL-001 has extensive analysis with well-researched vertical breakdowns yet is marked MEDIUM. Confidence should reflect evidence quality and accuracy.

**Suggestion:** Recalibrate confidence based on how well the analysis matches reality.

---

### ISSUE-R1-010: "Feature not bug" assertion lacks support

**Location:** Lines 156-158

**Quote:** Alternative B's con "'Jobs' view that's not really about jobs" - "This is actually a FEATURE, not a bug."

**Impact:** Declaring a con to be a feature doesn't resolve the underlying UX problem. If users expect "Jobs" to show jobs, calling the mismatch a "feature" doesn't prevent confusion.

**Suggestion:** Either:
1. Propose renaming the view (e.g., "Data" or "Outputs")
2. Explain how users won't be confused by the mismatch
3. Accept the con and propose specific mitigation

---

## Recommendation

**NEEDS_REVISION**

### Rationale

The Engineer's core insights about user mental models and output-centricity are valuable observations. However:

1. **Misreads current state**: The source spec (v1.0) has already crystallized around output-prominent job list design
2. **Proposes contradictory solution**: Pipeline visualization is explicitly in "What We Explicitly Don't Do"
3. **Introduces gaps already addressed**: GAP-STATE-002 (partial states) is already solved by running job rendering
4. **Uses unverifiable evidence**: Multiple citations cannot be verified against provided documents
5. **Leaves critical gaps open**: Backtest integration is deferred without resolution

### Next Round Should

1. **Accept existing crystallized design**: Work within the v1.0 spec's job-list-with-output-prominence framework
2. **Identify REMAINING gaps**: What is actually missing after accounting for existing output-centric features?
3. **Resolve Backtest**: This is a real gap in both documents - propose concrete solution
4. **Separate workflows**: Explicitly address how both monitoring AND debugging are served
5. **Verify citations**: Only cite from provided documents, or explicitly note external sources

---

## Appendix: Issue Summary

| ID | Severity | Gap | Summary | Status |
|----|----------|-----|---------|--------|
| ISSUE-R1-001 | CRITICAL | - | Proposed design contradicts crystallized spec decision | OPEN |
| ISSUE-R1-002 | CRITICAL | CORE-001 | Source spec already addresses output-centricity | OPEN |
| ISSUE-R1-003 | HIGH | MENTAL-001 | No mapping to existing crate data model | OPEN |
| ISSUE-R1-004 | HIGH | - | Backtest excluded from data-state model | OPEN |
| ISSUE-R1-005 | HIGH | CORE-001 | Monitoring vs debugging not separated | OPEN |
| ISSUE-R1-006 | MEDIUM | MENTAL-001 | Linear model doesn't match workflow complexity | OPEN |
| ISSUE-R1-007 | MEDIUM | - | GAP-STATE-002 already addressed by source spec | OPEN |
| ISSUE-R1-008 | MEDIUM | - | Evidence citations unverifiable | OPEN |
| ISSUE-R1-009 | LOW | - | Confidence ratings inverted | OPEN |
| ISSUE-R1-010 | LOW | - | "Feature not bug" assertion unsupported | OPEN |
