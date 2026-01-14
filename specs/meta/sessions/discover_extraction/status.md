# Discover Extraction Integration - Refinement Status

**Session Started:** 2026-01-13
**Source Spec:** `specs/views/discover.md` (Phase 18: Extraction API Integration)
**Goal:** Refine the Extraction API integration in Glob Explorer to be complete and implementable

---

## Session Metrics

| Metric | Value |
|--------|-------|
| Total Gaps | 14 |
| Resolved | 10 |
| In Review | 0 |
| Open | 4 |
| Current Round | 4 |
| Convergence State | INTEGRATED |

---

## Gap Inventory

### CRITICAL (Blocks Implementation)

| ID | Description | State | Severity |
|----|-------------|-------|----------|
| GAP-STATE-001 | **State machine not updated in Section 13.3** - Phase 18a defines new states but Section 13.3 diagram only shows Explore/Focused. Need unified state diagram. | ACCEPTED (R2) | CRITICAL |
| GAP-TRANS-001 | **Transition triggers incomplete** - How does user enter EditRule from Browse? Press `e` on what? Pattern? Selected folder? File? | ACCEPTED (R2) | CRITICAL |

### HIGH (Implementation will be incorrect)

| ID | Description | State | Severity |
|----|-------------|-------|----------|
| GAP-FIELD-001 | **Field inference input unclear** - 18c says infer from `sample_paths` but where do these come from? Current pattern matches? Selected files? | ACCEPTED (R3) | HIGH |
| GAP-TEST-001 | **Test execution model unclear** - Does TEST run synchronously blocking UI? Background with progress? How many files before it becomes async? | ACCEPTED (R3) | HIGH |
| GAP-DATA-001 | **RuleDraft vs extraction.md schema mismatch** - Phase 18 defines `RuleDraft` but extraction.md defines different YAML schema. Need alignment. | ACCEPTED (R3) | HIGH |
| GAP-NAV-001 | **Return path from Published unclear** - After publish completes, user presses Enter. Do they return to Browse at root? Same prefix? Pattern preserved? | ACCEPTED (R3) | HIGH |

### MEDIUM (Implementation possible but suboptimal)

| ID | Description | State | Severity |
|----|-------------|-------|----------|
| GAP-UI-001 | **EDIT RULE layout undefined** - 18b describes state but no ASCII layout. Section 13.8 has layout but doesn't match 18b struct fields. | ACCEPTED (R4) | MEDIUM |
| GAP-INFER-001 | **Inference confidence thresholds undefined** - 18c mentions HIGH/MEDIUM/LOW confidence but no numeric thresholds. | ACCEPTED (R4) | MEDIUM |
| GAP-HIST-001 | **Histogram rendering details missing** - 18d shows histogram but doesn't specify: bar width, max values shown, truncation behavior. | ACCEPTED (R4) | MEDIUM |
| GAP-ERR-001 | **Error handling in PUBLISH undefined** - What if DB write fails? Job creation fails? Rule name conflicts? | ACCEPTED (R4) | MEDIUM |

### LOW (Polish)

| ID | Description | State | Severity |
|----|-------------|-------|----------|
| GAP-KEY-001 | **Keybinding conflicts possible** - `e` for edit in EditRule conflicts with existing `e` in Files panel (re-extract). | OPEN | LOW |
| GAP-HELP-001 | **No help text for new states** - Status bar hints not defined for EditRule/Testing/Publishing phases. | OPEN | LOW |
| GAP-CTX-001 | **Prefix context on return** - Need to specify what "prefix" means when returning to Browse from different states. | NEW (R1) | LOW |

### NEW GAPS (From Round 1)

| ID | Description | State | Severity |
|----|-------------|-------|----------|
| GAP-TMPL-001 | **Template matching flow UX** - Does `e` on single file show templates inline or separate mode? | NEW (R1) | MEDIUM |

---

## Round History

### Round 1
- **Focus:** CRITICAL gaps (GAP-STATE-001, GAP-TRANS-001)
- **Status:** NEEDS_REVISION
- **Engineer:** COMPLETE (see `round_001/engineer.md`)
- **Reviewer:** COMPLETE (see `round_001/reviewer.md`)
- **Verdict:** NEEDS_REVISION
- **Issues Found:**
  - 2 CRITICAL: Publishing confirmation flow, Esc destination inconsistency
  - 2 HIGH: Missing Enter key in diagram, template matching scope creep
  - 3 MEDIUM: Terminology, missing `j` key, scope creep
  - 2 LOW: Return location, emoji consistency
- **New Gaps Identified:**
  - GAP-CTX-001 (LOW): Prefix context definition on return to Browse
  - GAP-TMPL-001 (MEDIUM): Template matching flow UX details

### Round 2
- **Focus:** Revisions based on user decisions
- **Status:** APPROVED
- **Engineer:** COMPLETE (see `round_002/engineer.md`)
- **Reviewer:** COMPLETE (see `round_002/reviewer.md`)
- **Verdict:** APPROVED
- **User Decisions Applied:**
  - Esc from Testing → EditRule (preserve draft)
  - Require Enter to confirm publish
- **Gaps Resolved:**
  - GAP-STATE-001: Unified state machine with 6 states
  - GAP-TRANS-001: `e` requires Filtering state with matches > 0

### Round 3
- **Focus:** HIGH priority gaps
- **Status:** APPROVED
- **Engineer:** COMPLETE (see `round_003/engineer.md`)
- **Reviewer:** COMPLETE (see `round_003/reviewer.md`)
- **Verdict:** APPROVED
- **Gaps Resolved:**
  - GAP-FIELD-001: Stratified sampling, max 100 files from pattern matches
  - GAP-TEST-001: Always async with spawn_blocking, cancellable, per-file progress
  - GAP-DATA-001: DB schema authoritative, RuleDraft aligns with extraction.md
  - GAP-NAV-001: Return to Browse at root (clean slate after publish)

### Round 4 (Current)
- **Focus:** MEDIUM priority gaps
- **Status:** APPROVED
- **Engineer:** COMPLETE (see `round_004/engineer.md`)
- **Reviewer:** COMPLETE (see `round_004/reviewer.md`)
- **Verdict:** APPROVED
- **Gaps Resolved:**
  - GAP-UI-001: Definitive EDIT RULE ASCII layout with focus indicators, section keybindings
  - GAP-INFER-001: Confidence thresholds (HIGH >= 0.85, MEDIUM 0.50-0.84, LOW < 0.50) with multi-factor scoring
  - GAP-HIST-001: Histogram rendering spec (12-char bars, 5 max values, 15-char labels, truncation)
  - GAP-ERR-001: Typed errors with recovery options, conflict detection, partial success handling

---

## Decisions Log

| Round | Issue/Gap | Decision | Impact |
|-------|-----------|----------|--------|
| 1 | GAP-TRANS-001 | `e` key requires Filtering state with matches > 0 | Clear trigger condition |
| 1 | GAP-STATE-001 | Proposed "Navigation Layer / Rule Editing Layer" model | Clearer mental model |
| 1 | ISSUE-R1-002 | Esc from Testing should go to EditRule (preserve draft), not Browse | NEEDS RESOLUTION |

---

## Convergence Tracking

| Round | Resolved | New | Net | Weighted Net | State |
|-------|----------|-----|-----|--------------|-------|
| 0 | 0 | 12 | -12 | -48 | STARTING |
| 1 | 0 | 2 | -2 | -6 | ITERATING |
| 2 | 2 | 0 | +2 | +32 | CONVERGING |
| 3 | 4 | 0 | +4 | +16 | CONVERGING |
| 4 | 4* | 0 | +4 | +8 | CONVERGING |

**Note:** Round 4 resolved all MEDIUM gaps.
Cumulative: 10 resolved (+56 weighted), 4 remaining (all LOW).
Remaining: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 4 LOW gaps.

**Session Status: INTEGRATED** - All blocking gaps resolved and integrated into specs/views/discover.md v2.2. LOW gaps are polish items for future iterations.
