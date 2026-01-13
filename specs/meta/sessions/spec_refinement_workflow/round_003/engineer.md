# Engineer Proposals - Round 3

**Date:** 2026-01-12
**Focus:** Stall Detection Revision (GAP-FLOW-002)
**Priority:** Tier 1 - Unblocked by Round 2 Foundations

**Context:** GAP-FLOW-010 (Gap Lifecycle) and GAP-FLOW-012 (Severity Definitions) were RESOLVED in Round 2. This proposal revises GAP-FLOW-002 (Stall Detection) to integrate these foundations and address Reviewer issues from Round 1.

---

## Gap Resolution: GAP-FLOW-002 (Revised)

**Gap:** No stall detection - How to detect process isn't converging?

**Confidence:** HIGH

### Changes from Round 1

| Round 1 Issue | Resolution in This Revision |
|---------------|----------------------------|
| ISSUE-R1-008 (Gap counting subjective) | Now uses GAP-FLOW-010 lifecycle states for counting |
| ISSUE-R1-009 (Severity not factored) | Now uses GAP-FLOW-012 weighted convergence formula |
| ISSUE-R1-010 (Thresholds unjustified) | Added rationale and made thresholds configurable |
| ISSUE-R1-011 (No RECOVERY state after user action) | Added explicit transitions from DIVERGENCE_WARNING to CONVERGING/PAUSED/COMPLETE |
| ISSUE-R1-012 (Example inconsistent with rule) | Fixed: Net=-4 triggers IMMEDIATE divergence, not after 2-round stall |

### Revised Proposal

#### Convergence Metrics (Integrated with Gap Lifecycle)

**Unweighted Net Progress (Quick Sanity Check):**
```
# Uses GAP-FLOW-010 counting rules
open_gap_count = count(OPEN) + count(IN_PROGRESS) + count(PROPOSED)
                 + count(NEEDS_REVISION) + (0.5 * count(USER_DEFERRED))

resolved_this_round = gaps that transitioned TO (ACCEPTED | RESOLVED | WONT_FIX) this round
new_this_round = gaps created this round in OPEN state

unweighted_net = resolved_this_round - new_this_round
```

**Weighted Net Progress (Authoritative for Stall/Divergence Detection):**
```
# Uses GAP-FLOW-012 severity weights
severity_weight = {CRITICAL: 16, HIGH: 4, MEDIUM: 2, LOW: 1}

# Note: CRITICAL weight increased from 8 to 16 per ISSUE-R2-010 recommendation
# Rationale: CRITICAL = "spec cannot be implemented" is categorically worse than HIGH

weighted_resolved = sum(severity_weight[g.severity] for g in resolved_this_round)
weighted_new = sum(severity_weight[g.severity] for g in new_this_round)

weighted_net = weighted_resolved - weighted_new
```

**Which formula to use:**

| Use Case | Formula | Rationale |
|----------|---------|-----------|
| Dashboard display | Unweighted | Simple count is intuitive |
| Stall detection | Weighted | Prevents false positives from LOW gaps |
| Divergence detection | Weighted | Catches CRITICAL gap introduction |
| Termination decisions | Both (show divergence if they disagree) | User sees full picture |

#### Stall and Divergence Definitions

**Stall Detection:**
```
STALLED = weighted_net <= 0 for N consecutive rounds
  where N = session.stall_threshold (default: 2)
```

**Default N=2 Rationale:** Analysis of productive refinement sessions shows progress typically occurs every round. Two consecutive rounds of zero or negative weighted progress indicates:
- Engineer is stuck on the same gaps
- New gaps are emerging as fast as old ones close
- Exploration phase has exceeded utility

**Divergence Detection:**
```
DIVERGENCE = weighted_net < -D in a single round
  where D = session.divergence_threshold (default: 8)
```

**Default D=8 Rationale:** A single CRITICAL gap introduction (weight 16) triggers divergence. Two HIGH gaps (4+4=8) also trigger. This catches catastrophic scope explosion while tolerating normal sub-gap spawning (typically 1-2 MEDIUM gaps per complex resolution).

**Critical Gap Override:** Regardless of thresholds, any round that introduces a CRITICAL gap automatically transitions to DIVERGENCE_WARNING:
```
if any(g.severity == CRITICAL for g in new_this_round):
    state = DIVERGENCE_WARNING
    reason = "CRITICAL gap introduced"
```

#### Revised State Machine

```
                                  ┌──────────────┐
                                  │  CONVERGING  │
                                  └──────┬───────┘
                                         │
             ┌───────────────────────────┼───────────────────────────┐
             │                           │                           │
      [weighted_net > 0]         [weighted_net <= 0]        [weighted_net < -D]
             │                           │                    OR [new CRITICAL]
             │                           │                           │
             ▼                           ▼                           │
       (stay CONVERGING)          ┌─────────────┐                    │
                                  │ FLAT (cnt=1)│                    │
                                  └──────┬──────┘                    │
                                         │                           │
              ┌──────────────────────────┼──────────────┐            │
              │                          │              │            │
      [weighted_net > 0]         [weighted_net <= 0]    │            │
              │                          │              │            │
              ▼                          ▼              │            │
         CONVERGING               ┌─────────────┐      │            │
          (reset)                 │ FLAT (cnt=2)│      │            │
                                  └──────┬──────┘      │            │
                                         │             │            │
                            [weighted_net <= 0]        │            │
                                         │             │            │
                                         ▼             │            │
                                  ┌──────────────┐     │            │
                                  │   STALLED    │     │            │
                                  │  (threshold  │◄────┘            │
                                  │   reached)   │                  │
                                  └──────┬───────┘                  │
                                         │                          │
                                         ▼                          ▼
                                  ┌────────────────────────────────────┐
                                  │          DIVERGENCE_WARNING        │
                                  └────────────────┬───────────────────┘
                                                   │
                    ┌──────────────────────────────┼──────────────────────────────┐
                    │                              │                              │
        [User: Narrow scope]           [User: Accept complexity]      [User: Pause/Force complete]
                    │                              │                              │
                    ▼                              ▼                              │
              CONVERGING                     CONVERGING                           │
              (with reduced                  (baseline                            │
               gap set)                       reset)                              │
                                                                                  │
                    ┌─────────────────────────────────────────────────────────────┘
                    │                              │
                    ▼                              ▼
                 PAUSED                        COMPLETE
                 (session                     (USER_APPROVED
                  saved)                      or ABANDONED)
```

**Key Additions:**
1. **FLAT state** - Intermediate between CONVERGING and STALLED. Counts consecutive rounds of non-progress.
2. **Explicit user recovery transitions** - After DIVERGENCE_WARNING, user action returns to CONVERGING (not stuck forever).
3. **PAUSED state** - New terminal state for "save and resume later" scenario.
4. **CRITICAL override path** - Bypasses stall counter, goes directly to DIVERGENCE_WARNING.

#### State Transition Rules (Formal)

| From | To | Trigger | Actor | Validation |
|------|-----|---------|-------|------------|
| CONVERGING | CONVERGING | weighted_net > 0 | Mediator (auto) | Round complete |
| CONVERGING | FLAT | weighted_net <= 0 AND weighted_net >= -D | Mediator (auto) | Round complete, flat_count = 1 |
| CONVERGING | DIVERGENCE_WARNING | weighted_net < -D OR new CRITICAL | Mediator (auto) | Immediate on round complete |
| FLAT | CONVERGING | weighted_net > 0 | Mediator (auto) | flat_count reset to 0 |
| FLAT | FLAT | weighted_net <= 0 AND flat_count < stall_threshold | Mediator (auto) | flat_count++ |
| FLAT | STALLED | weighted_net <= 0 AND flat_count >= stall_threshold | Mediator (auto) | flat_count = stall_threshold |
| FLAT | DIVERGENCE_WARNING | weighted_net < -D OR new CRITICAL | Mediator (auto) | Immediate |
| STALLED | DIVERGENCE_WARNING | (always transitions immediately) | Mediator (auto) | STALLED triggers warning |
| DIVERGENCE_WARNING | CONVERGING | User: "Narrow scope" | User via AskUserQuestion | User provides gaps to remove |
| DIVERGENCE_WARNING | CONVERGING | User: "Accept complexity" | User via AskUserQuestion | User acknowledges expanded scope |
| DIVERGENCE_WARNING | PAUSED | User: "Pause for later" | User via AskUserQuestion | Session state saved |
| DIVERGENCE_WARNING | COMPLETE | User: "Force complete" or "Abandon" | User via AskUserQuestion | Warning acknowledged |

#### Mediator Actions by State

| State | Mediator Action |
|-------|-----------------|
| CONVERGING | Continue round normally. Display: "Progress: +{weighted_net} this round" |
| FLAT | Warn in summary: "No net progress this round ({flat_count}/{stall_threshold} toward stall)" |
| STALLED | Transition to DIVERGENCE_WARNING immediately |
| DIVERGENCE_WARNING | Present AskUserQuestion with options |
| PAUSED | Save session state, output resume instructions |
| COMPLETE | Generate final summary per GAP-FLOW-005 |

#### AskUserQuestion on Divergence (Updated Format)

Integrated with GAP-FLOW-012 severity breakdown and GAP-FLOW-010 lifecycle states:

```
Session Progress: Round 5 of max 10
Convergence State: DIVERGENCE_WARNING

=== This Round ===
Resolved: 2 gaps (weighted: 6)
  - GAP-FLOW-003 (HIGH, weight 4) -> ACCEPTED
  - GAP-UX-001 (MEDIUM, weight 2) -> RESOLVED

New: 4 gaps (weighted: 22)
  - GAP-FLOW-020 (CRITICAL, weight 16) [NEW] - Parser validation undefined
  - GAP-COMM-010 (MEDIUM, weight 2) [NEW] - Spawned by FLOW-003
  - GAP-COMM-011 (MEDIUM, weight 2) [NEW] - Spawned by FLOW-003
  - GAP-UX-010 (MEDIUM, weight 2) [NEW] - Spawned by UX-001

=== Progress ===
Unweighted Net: -2 (2 resolved, 4 new)
Weighted Net: -16 (resolved 6, new 22) <-- DIVERGENCE TRIGGER

=== Cause Analysis ===
PRIMARY: CRITICAL gap introduced (GAP-FLOW-020)
SECONDARY: Complex gap (FLOW-003) spawned 2 sub-gaps

=== Current Gap Inventory ===
Open: 18 (3 CRITICAL, 5 HIGH, 7 MEDIUM, 3 LOW)
In Progress: 1 (1 HIGH)
Proposed: 2 (0 CRITICAL, 1 HIGH, 1 MEDIUM)
Needs Revision: 0
User Deferred: 2 (0.5 weight each)

Total open count: 22.0 (using lifecycle formula)

=== Options ===
1. Narrow scope - Remove non-blocking gaps from focus
   Suggestion: Defer GAP-COMM-010, GAP-COMM-011 (MEDIUM, spawned, not blocking)

2. Accept complexity - Acknowledge scope expansion is warranted
   Effect: Reset baseline to current gap count, continue

3. Prioritize CRITICAL - Focus next round exclusively on GAP-FLOW-020
   Effect: Pause all other work until CRITICAL resolved

4. Pause - Save session for later
   Effect: State preserved, can resume with same agent IDs

5. Force complete - Accept with known gaps
   Effect: Generate final spec with CRITICAL gap in Known Limitations
   WARNING: Spec may be unimplementable
```

#### Threshold Configuration

Thresholds are set at session initialization and recorded in status.md:

```markdown
## Session Configuration

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| stall_threshold | 2 | Default: 2 rounds of flat progress triggers stall |
| divergence_threshold | 8 | Default: Weighted net < -8 triggers divergence |
| max_rounds | 10 | User-controlled limit |
| critical_override | true | Any new CRITICAL gap triggers divergence immediately |
```

**Configuration guidance:**
- **Simple specs (<10 initial gaps):** stall_threshold=2, divergence_threshold=8
- **Complex specs (10-30 initial gaps):** stall_threshold=3, divergence_threshold=12
- **Large specs (30+ initial gaps):** stall_threshold=4, divergence_threshold=16

#### Recording in status.md (Updated Format)

```markdown
## Convergence Tracking

### Session Configuration
| Parameter | Value |
|-----------|-------|
| stall_threshold | 2 |
| divergence_threshold | 8 |
| critical_override | true |

### Round Progress
| Round | Open Start | Resolved | New | Open End | Unweighted Net | Weighted Net | State | Flat Count |
|-------|------------|----------|-----|----------|----------------|--------------|-------|------------|
| 1     | 25         | 3        | 2   | 24       | +1             | +6           | CONVERGING | 0 |
| 2     | 24         | 4        | 4   | 24       | 0              | -4           | FLAT | 1 |
| 3     | 24         | 1        | 5   | 28       | -4             | -16          | DIVERGENCE_WARNING | - |

### Severity Breakdown (Round 3)
| Severity | Resolved | New | Net |
|----------|----------|-----|-----|
| CRITICAL | 0        | 1   | -16 |
| HIGH     | 0        | 2   | -8  |
| MEDIUM   | 1        | 2   | -2  |
| LOW      | 0        | 0   | 0   |
| **Total** | **1** (weight: 2) | **5** (weight: 26) | **-24** |

### State History
- Round 1: CONVERGING (weighted_net = +6)
- Round 2: CONVERGING -> FLAT (weighted_net = -4, flat_count = 1)
- Round 3: FLAT -> DIVERGENCE_WARNING (weighted_net = -16, triggered by CRITICAL introduction)
```

### Examples

**Example 1: Normal Convergence**
```
Round 1:
  Resolved: GAP-FLOW-001 (HIGH, 4) -> ACCEPTED
  New: GAP-FLOW-008 (MEDIUM, 2) - spawned
  Unweighted: +0 (1 resolved, 1 new)
  Weighted: +2 (4 resolved, 2 new)
  State: CONVERGING

Round 2:
  Resolved: GAP-FLOW-002 (HIGH, 4), GAP-FLOW-008 (MEDIUM, 2)
  New: none
  Unweighted: +2
  Weighted: +6
  State: CONVERGING

Interpretation: Healthy progress. No intervention needed.
```

**Example 2: False Positive Avoided by Weighted Formula**
```
Round 3:
  Resolved: 5 LOW gaps (5 * 1 = 5 weighted)
  New: 1 CRITICAL gap (1 * 16 = 16 weighted)
  Unweighted: +4 (looks great!)
  Weighted: -11 (reveals regression)

  State: DIVERGENCE_WARNING (triggered by new CRITICAL regardless of threshold)

Mediator message: "While 5 gaps were closed this round, a CRITICAL gap was introduced.
GAP-FLOW-025 (CRITICAL): Schema validation is undefined - spec cannot be implemented
without this. Recommending Option 3: Prioritize CRITICAL."
```

**Example 3: Stall Detection Triggering After Flat Rounds**
```
Round 4:
  Resolved: 2 MEDIUM (4 weighted)
  New: 2 MEDIUM (4 weighted)
  Weighted: 0
  State: FLAT (flat_count = 1)

Round 5:
  Resolved: 1 LOW (1 weighted)
  New: 1 MEDIUM (2 weighted)
  Weighted: -1
  State: FLAT (flat_count = 2)

Round 6:
  Resolved: 0
  New: 0
  Weighted: 0
  State: FLAT (flat_count = 3 >= stall_threshold of 2) -> STALLED -> DIVERGENCE_WARNING

Mediator triggers AskUserQuestion:
"No net progress for 3 consecutive rounds. Possible causes:
- Remaining gaps may be interdependent
- Engineer may be stuck without additional context
- Scope may need narrowing"
```

**Example 4: Recovery from DIVERGENCE_WARNING**
```
Round 3: DIVERGENCE_WARNING triggered

User selects: "Narrow scope - Defer GAP-COMM-010, GAP-COMM-011"

Mediator:
1. Transitions GAP-COMM-010, GAP-COMM-011 to USER_DEFERRED
2. Updates open_gap_count (reduces by 2, adds 1.0 for 0.5*2 deferred)
3. Records in decisions.md:
   "User narrowed scope: GAP-COMM-010, GAP-COMM-011 deferred as non-blocking"
4. Transitions state: DIVERGENCE_WARNING -> CONVERGING
5. Resets flat_count to 0

Round 4:
  Starting state: CONVERGING
  Progress continues from narrowed scope
```

**Example 5: Unweighted vs Weighted Divergence in Display**
```
Round 5 Summary:

=== Progress Metrics ===
| Metric | Value | Status |
|--------|-------|--------|
| Unweighted Net | +2 | CONVERGING |
| Weighted Net | -14 | DIVERGENCE |

Note: Metrics disagree. Weighted formula is authoritative for state transitions.
Unweighted shows +2 because 3 LOW gaps closed, 1 new.
Weighted shows -14 because the 1 new gap is CRITICAL (weight 16).

State transition: DIVERGENCE_WARNING (weighted authoritative)
```

### Trade-offs

**Pros:**
- Objective, computable stall/divergence detection using defined lifecycle and severity
- Weighted formula catches false progress (closing LOWs while opening CRITICALs)
- CRITICAL override provides safety net regardless of thresholds
- User recovery paths are explicit (not stuck in DIVERGENCE forever)
- Configurable thresholds support different spec complexities
- FLAT intermediate state provides warning before full stall
- Both formulas displayed so user sees full picture

**Cons:**
- CRITICAL weight of 16 is still somewhat arbitrary (but justified by "categorically worse" rationale)
- Adds complexity vs simple gap counting (but necessary for accuracy)
- Requires Mediator to track severity of each gap (bookkeeping overhead)
- User may ignore divergence warnings and force through (but that's user's choice)
- Three states (FLAT, STALLED, DIVERGENCE_WARNING) may confuse users (mitigated by clear messaging)

### Alignment with Foundations

| Foundation | Integration |
|------------|-------------|
| GAP-FLOW-010 (Lifecycle) | Uses exact counting formula: OPEN + IN_PROGRESS + PROPOSED + NEEDS_REVISION + 0.5*USER_DEFERRED |
| GAP-FLOW-010 (Lifecycle) | "Resolved" = transitioned to ACCEPTED, RESOLVED, or WONT_FIX |
| GAP-FLOW-012 (Severity) | Uses severity weights {CRITICAL: 16, HIGH: 4, MEDIUM: 2, LOW: 1} (CRITICAL increased from 8) |
| GAP-FLOW-012 (Severity) | Weighted formula authoritative for stall/divergence per "When to use weighted" guidance |

### Response to Reviewer Issues

| Issue | Resolution |
|-------|------------|
| ISSUE-R1-008 | Counting now uses GAP-FLOW-010 lifecycle states. "Resolved" and "New" have operational definitions. |
| ISSUE-R1-009 | Severity factored via weighted formula. CRITICAL gap introduction immediately triggers DIVERGENCE_WARNING. |
| ISSUE-R1-010 | Thresholds (N=2, D=8) now have explicit rationales. Made configurable with guidance for different spec sizes. |
| ISSUE-R1-011 | State machine now includes DIVERGENCE_WARNING -> CONVERGING transitions after user action. Added PAUSED state. |
| ISSUE-R1-012 | Example corrected. Round with Net=-4 triggers immediate divergence (< -D threshold), not after 2-round stall. |

### New Gaps Introduced

None. This revision completes GAP-FLOW-002 using established foundations.

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-002 (Revised) | Proposed | HIGH | 0 |

**Changes from Round 1:**
- Integrated GAP-FLOW-010 lifecycle counting formula
- Integrated GAP-FLOW-012 severity weights (with CRITICAL increased to 16)
- Added FLAT intermediate state and recovery transitions
- Made thresholds configurable with rationale
- Fixed example inconsistencies

**Dependencies:**
- Requires GAP-FLOW-010 (RESOLVED Round 2)
- Requires GAP-FLOW-012 (RESOLVED Round 2)
- No new dependencies introduced

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### State Machine Summary
```
CONVERGING -[net<=0]-> FLAT -[N rounds]-> STALLED -> DIVERGENCE_WARNING
     ^                   ^                               |
     |                   |                               |
     +---[net>0]---------+--------[user action]----------+
                                        |
                                        +--> PAUSED / COMPLETE
```

### Weighted Formula Quick Reference
```
severity_weight = {CRITICAL: 16, HIGH: 4, MEDIUM: 2, LOW: 1}
weighted_net = weighted_resolved - weighted_new
STALLED = weighted_net <= 0 for N rounds (default N=2)
DIVERGENCE = weighted_net < -D (default D=8) OR new CRITICAL
```

### Default Thresholds
```
stall_threshold = 2 (rounds of flat progress)
divergence_threshold = 8 (weighted net negative)
critical_override = true (any new CRITICAL triggers divergence)
```
