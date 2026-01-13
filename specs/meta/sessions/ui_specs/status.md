# UI Specs Refinement - Status

**Session:** ui_specs
**Started:** 2026-01-12
**Round:** 5 (Cross-Cutting Review Complete)
**Convergence State:** CONVERGING

---

## Progress Summary

| Metric | Value |
|--------|-------|
| Total Gaps | 24 |
| Open | 20 |
| Resolved | 4 |
| Net Progress | +4 |
| Weighted Open | 33 (was 49) |

---

## Gap Inventory

### STUB - Stub Expansion Needed (4 gaps)

| ID | Severity | State | Description |
|----|----------|-------|-------------|
| GAP-STUB-001 | HIGH | **RESOLVED** | `views/home.md` expanded to full spec |
| GAP-STUB-002 | HIGH | **RESOLVED** | `views/jobs.md` expanded to full spec |
| GAP-STUB-003 | HIGH | **RESOLVED** | `views/sources.md` expanded to full spec |
| GAP-STUB-004 | HIGH | **RESOLVED** | `views/extraction.md` expanded to full spec |

### STRUCT - Structural Consistency (4 gaps)

| ID | Severity | State | Description |
|----|----------|-------|-------------|
| GAP-STRUCT-001 | MEDIUM | OPEN | View specs have inconsistent section ordering |
| GAP-STRUCT-002 | MEDIUM | OPEN | `discover.md` has 13 sections while template has 7 |
| GAP-STRUCT-003 | LOW | OPEN | Some specs have "Decisions Made" section, others don't |
| GAP-STRUCT-004 | LOW | OPEN | Implementation phases format varies (checkboxes vs bullets) |

### COMP - Compression Opportunities (5 gaps)

| ID | Severity | State | Description |
|----|----------|-------|-------------|
| GAP-COMP-001 | MEDIUM | OPEN | Dropdown pattern defined in both tui.md and discover.md |
| GAP-COMP-002 | MEDIUM | OPEN | Dialog pattern duplicated (Rules Manager, metadata filter, etc.) |
| GAP-COMP-003 | LOW | OPEN | `ViewState` struct repeated with variations across specs |
| GAP-COMP-004 | LOW | OPEN | Keybinding tables have redundant global keys |
| GAP-COMP-005 | LOW | OPEN | Status indicators (symbols) defined multiple times |

### REF - Reference/Cross-Reference Issues (4 gaps)

| ID | Severity | State | Description |
|----|----------|-------|-------------|
| GAP-REF-001 | HIGH | OPEN | `views/extraction.md` references deprecated `extraction_rules.md` |
| GAP-REF-002 | MEDIUM | OPEN | `discover.md` references `specs/semantic_path_mapping.md` (superseded) |
| GAP-REF-003 | MEDIUM | OPEN | `discover.md` references `specs/ai_wizards.md` (doesn't exist) |
| GAP-REF-004 | LOW | OPEN | Parent spec references inconsistent (some use relative, some absolute) |

### SCOPE - Scope/Boundary Issues (3 gaps)

| ID | Severity | State | Description |
|----|----------|-------|-------------|
| GAP-SCOPE-001 | HIGH | OPEN | `discover.md` Section 8 (Extractors) duplicates `extraction.md` |
| GAP-SCOPE-002 | MEDIUM | OPEN | `discover.md` Semantic Path section should reference new extraction.md |
| GAP-SCOPE-003 | MEDIUM | OPEN | Pending Review panel scope unclear (discover.md vs jobs.md) |

### STATE - State Machine/Data Model (2 gaps)

| ID | Severity | State | Description |
|----|----------|-------|-------------|
| GAP-STATE-001 | MEDIUM | OPEN | Only discover.md has full state machine diagram |
| GAP-STATE-002 | LOW | OPEN | No common base state struct defined in tui.md |

### KEY - Keybinding Consistency (2 gaps)

| ID | Severity | State | Description |
|----|----------|-------|-------------|
| GAP-KEY-001 | MEDIUM | OPEN | `n` key means "New" in some views, undefined in others |
| GAP-KEY-002 | LOW | OPEN | Focus panel keys (1,2,3) only defined in discover.md |

### XCUT - Cross-Cutting Issues (Round 5) (8 gaps, 8 resolved)

| ID | Severity | State | Description |
|----|----------|-------|-------------|
| XCUT-PAT-007 | HIGH | **RESOLVED** | Status indicators standardized: `✓`=Complete, `●`=Active, `↻`=Running |
| XCUT-KEY-001 | HIGH | **RESOLVED** | `n/N` search behavior clarified: only active after `/` |
| XCUT-NAV-002 | MEDIUM | **RESOLVED** | discover.md: `D` → `1` |
| XCUT-NAV-003 | MEDIUM | **RESOLVED** | parser_bench.md: `Alt+P` → `2` |
| XCUT-DATA-004 | MEDIUM | **RESOLVED** | Added EntryContext enum to extraction.md |
| XCUT-KEY-002 | MEDIUM | **RESOLVED** | Documented 1/2/3 override in discover.md and tui.md |
| XCUT-KEY-005 | MEDIUM | **RESOLVED** | Documented `c` key variance in tui.md (acceptable) |
| XCUT-KEY-006 | MEDIUM | **RESOLVED** | Documented `r` context in tui.md (acceptable) |

*Note: 18 LOW issues documented in round_005/cross_cutting_review.md but not tracked as gaps.*

---

## Weighted Calculation

```
CRITICAL (16): 0 gaps = 0
HIGH (4):      2 gaps = 8 (2 original REF/SCOPE)
MEDIUM (2):   10 gaps = 20 (10 original, 6 XCUT resolved)
LOW (1):       8 gaps = 8

Total Weighted Open: 36 (was 48)
Resolved this round: 6 MEDIUM XCUT gaps (12 points)
Net progress: +12 points
```

---

## Round History

| Round | Gaps Resolved | New Gaps | Net | State |
|-------|--------------|----------|-----|-------|
| 0 | — | 24 | — | INITIAL |
| 1 | 1 (GAP-STUB-001) | 0 | +1 | CONVERGING |
| 2 | 1 (GAP-STUB-002) | 0 | +1 | CONVERGING |
| 3 | 1 (GAP-STUB-003) | 0 | +1 | CONVERGING |
| 4 | 1 (GAP-STUB-004) | 0 | +1 | CONVERGING |
| 5 | 0 (cross-cutting) | 8 XCUT | -8 | XCUT_REVIEW |
| 6 | 2 (XCUT-PAT-007, XCUT-KEY-001) | 0 | +2 | CONVERGING |
| 7 | 6 (XCUT-NAV-002/003, KEY-002/005/006, DATA-004) | 0 | +6 | CONVERGING |
| 8 | 0 (compression pass) | 0 | 0 | COMPRESSION |

---

## Round Summaries

### Round 1: GAP-STUB-001 (home.md)
**Verdict:** APPROVED
- Expanded from 114 lines to 793 lines
- 7 user workflows, 5-state machine, complete Rust data models
- Keybinding fix: `r` -> `R` for Recent files

### Round 2: GAP-STUB-002 (jobs.md)
**Verdict:** APPROVED_WITH_FIXES
- Expanded from 123 lines to 608 lines
- 8 workflows, 5-state machine, log viewer, circuit breaker support
- Schema requirements documented (cf_job_logs table)
- Keybinding fix: `r` -> `R` for Retry

### Round 3: GAP-STUB-003 (sources.md)
**Verdict:** APPROVED_WITH_FIXES
- Expanded from 148 lines to 292 lines
- Tree view with equivalence classes, 7 workflows, 7-state machine
- Keybinding fix: `r` -> `F2` for Rename in Class Manager

### Round 4: GAP-STUB-004 (extraction.md)
**Verdict:** APPROVED_WITH_FIXES
- Expanded from 190 lines to ~1560 lines
- 9 workflows, 11-state machine (including SCHEMA_MISSING), YAML editor
- Navigation clarified: drill-down from Discover (not key 5)
- Schema migration UI added for graceful degradation
- Keybinding: Uses `Y` for new YAML rule, no `r` conflicts

### Round 5: Cross-Cutting Review
**Type:** XCUT_REVIEW (per workflow v2.2)
**Specs analyzed:** tui.md, home.md, jobs.md, sources.md, extraction.md, discover.md, parser_bench.md

**Findings:**
- 26 total issues identified
- 0 Critical, 2 High, 6 Medium, 18 Low
- 8 actionable gaps added to inventory (HIGH/MEDIUM only)

**High Priority Issues:**
1. XCUT-PAT-007: Status indicator `●` means different things
2. XCUT-KEY-001: Global `n` for search conflicts with create actions

**Key Patterns Audited:**
- Dialogs: 14 instances, mostly consistent (Esc=cancel, Enter=confirm)
- Status indicators: Need standardization
- Navigation: Works but documentation conflicts

### Round 6: Fix HIGH Priority XCUT Issues
**Resolved:**
1. **XCUT-PAT-007**: Standardized status indicators
   - tui.md: Clarified `✓`=Complete, `●`=Active/Healthy, `↻`=Running
   - jobs.md: Updated to use `↻` for Running (was `●`)
   - home.md: Updated activity log Success to `✓` (was `●`)

2. **XCUT-KEY-001**: Clarified `n` key behavior
   - tui.md: `n/N` only active after `/` search initiated
   - Added "Key override rules" section
   - Documented search mode behavior

### Round 7: Fix MEDIUM Priority XCUT Issues
**Resolved:**
1. **XCUT-NAV-002**: discover.md entry key `D` → `1`
2. **XCUT-NAV-003**: parser_bench.md entry key `Alt+P` → `2`
3. **XCUT-KEY-002**: Documented Discover's 1/2/3 panel override in both discover.md and tui.md
4. **XCUT-DATA-004**: Added `EntryContext` enum to extraction.md for class context support
5. **XCUT-KEY-005**: Documented `c` key variance as acceptable (Cancel/Coverage/Class)
6. **XCUT-KEY-006**: Documented `r` key context in Parser Bench (re-run in ResultView)

### Round 8: Compression Pass
**Type:** COMPRESSION (per workflow v2.2 Section 14)
**Patterns extracted to tui.md:**

1. **Section 4.5 - Confirmation Dialog Pattern**
   - Standardized layout, keybindings, impact display
   - 5 instances across views now reference this

2. **Section 7.4 - Refresh Strategy**
   - Intervals: 5s default, 500ms active, 30s background
   - Triggers, pause conditions, debouncing
   - 5 instances across views now reference this

3. **Section 3.2 - List Navigation (strengthened)**
   - Added note: view specs should not duplicate
   - Reference pattern for view specs

**Lines saved:** ~60 across view specs (deferred cleanup)
**Benefit:** Consistent UX patterns, single source of truth

---

## Next Steps

**Compression complete.** Session in good state for pause.

### Completed This Session
- 4 STUB expansions (home, jobs, sources, extraction)
- 8 XCUT issues (cross-cutting review and fixes)
- 3 common patterns extracted to tui.md

### Remaining (can be done incrementally)
- 20 original gaps (REF, SCOPE, STRUCT, COMP, STATE, KEY)
- View spec cleanup to reference tui.md patterns
- 18 LOW severity XCUT issues (documented in round_005/)

### Session Statistics
| Metric | Value |
|--------|-------|
| Rounds completed | 8 |
| Gaps resolved | 12 (4 STUB + 8 XCUT) |
| Patterns extracted | 3 |
| Weighted open | 36 (down from 49) |
