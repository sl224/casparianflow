# Reviewer Assessment - Round 3

**Date:** 2026-01-12
**Reviewer:** Principal Engineer Instance
**Focus:** GAP-FLOW-002 (Stall Detection) - Revised Proposal
**Context:** Round 2 foundations (GAP-FLOW-010, GAP-FLOW-012) are now RESOLVED. This review evaluates the revised stall detection proposal.

---

## Review: GAP-FLOW-002 (Revised) - Stall Detection

### Foundation Integration Assessment

#### GAP-FLOW-010 (Gap Lifecycle) Integration

| Integration Point | Status | Notes |
|-------------------|--------|-------|
| Counting formula uses lifecycle states | CORRECT | Uses exact formula: `OPEN + IN_PROGRESS + PROPOSED + NEEDS_REVISION + 0.5*USER_DEFERRED` |
| "Resolved" defined operationally | CORRECT | `transitioned TO (ACCEPTED \| RESOLVED \| WONT_FIX)` matches GAP-FLOW-010 |
| "New" defined operationally | CORRECT | `created this round in OPEN state` is unambiguous |
| State transitions referenced | CORRECT | Recovery transitions (DIVERGENCE_WARNING -> CONVERGING) trigger correct lifecycle updates |

**Verdict:** GAP-FLOW-010 integration is complete and correct.

#### GAP-FLOW-012 (Severity Definitions) Integration

| Integration Point | Status | Notes |
|-------------------|--------|-------|
| Severity weights used | CORRECT | Uses {CRITICAL: 16, HIGH: 4, MEDIUM: 2, LOW: 1} |
| CRITICAL weight increased per R2-010 | CORRECT | Changed from 8 to 16 with clear rationale |
| Weighted formula authoritative | CORRECT | Matches GAP-FLOW-012 guidance for stall/divergence detection |
| Termination impact aligned | CORRECT | DIVERGENCE_WARNING leads to COMPLETE with "CRITICAL gap in Known Limitations" warning |

**Verdict:** GAP-FLOW-012 integration is complete and correct.

---

### Round 1 Issue Resolution Assessment

#### ISSUE-R1-008: Gap counting is subjective

**Original Issue:** "The ENTIRE stall detection system depends on accurate gap counting. Without lifecycle definition, the convergence tracker is meaningless."

**Resolution Claimed:** Now uses GAP-FLOW-010 lifecycle states for counting.

**Verification:**
- [x] `open_gap_count` formula explicitly references lifecycle states
- [x] `resolved_this_round` operationally defined as state transition
- [x] `new_this_round` operationally defined as creation in OPEN state
- [x] Examples show state transitions, not subjective counts

**Verdict:** RESOLVED. Counting is now objective and computable.

---

#### ISSUE-R1-009: Severity not factored into convergence calculation

**Original Issue:** "A round that closes 10 LOW gaps but opens 1 CRITICAL shows '+9' progress but is actually regression."

**Resolution Claimed:** Uses GAP-FLOW-012 weighted convergence formula with CRITICAL override.

**Verification:**
- [x] Weighted formula: `weighted_net = weighted_resolved - weighted_new`
- [x] CRITICAL weight increased to 16 (was 8 in original GAP-FLOW-012)
- [x] Example 2 explicitly demonstrates false positive avoidance
- [x] CRITICAL override: Any new CRITICAL gap triggers DIVERGENCE_WARNING regardless of thresholds

**Verdict:** RESOLVED. Severity is now the authoritative factor for stall/divergence detection.

---

#### ISSUE-R1-010: Threshold values are unjustified

**Original Issue:** "Why 2 rounds? Why -2? For simple specs, 2 rounds of no progress is concerning. For complex specs, it might be normal during exploration phase."

**Resolution Claimed:** Thresholds now have rationales and are configurable.

**Verification:**
- [x] N=2 rationale: "productive refinement sessions show progress every round"
- [x] D=8 rationale: "single CRITICAL (16) triggers, two HIGH (4+4) triggers"
- [x] Thresholds configurable at session initialization
- [x] Configuration guidance by spec complexity (simple/complex/large)

**Verdict:** RESOLVED. Thresholds are justified and configurable.

---

#### ISSUE-R1-011: No RECOVERY state after user action

**Original Issue:** "After user chooses 'Narrow scope' or 'Accept complexity,' what state do we return to?"

**Resolution Claimed:** Added explicit transitions from DIVERGENCE_WARNING.

**Verification:**
- [x] DIVERGENCE_WARNING -> CONVERGING (Narrow scope)
- [x] DIVERGENCE_WARNING -> CONVERGING (Accept complexity)
- [x] DIVERGENCE_WARNING -> PAUSED (new state)
- [x] DIVERGENCE_WARNING -> COMPLETE (Force complete/Abandon)
- [x] State transition rules table includes all four paths
- [x] Example 4 demonstrates recovery flow

**Verdict:** RESOLVED. Recovery paths are explicit and complete.

---

#### ISSUE-R1-012: Example inconsistent with rule

**Original Issue:** "Example table vs rule definition... the rule should have triggered at -3 in the same round."

**Resolution Claimed:** Fixed: Net=-4 triggers IMMEDIATE divergence per threshold.

**Verification:**
- [x] Divergence threshold D=8 (not -2 as in Round 1)
- [x] weighted_net < -D triggers immediately
- [x] Example 3 shows flat_count=3 triggering via stall path (correct)
- [x] Examples show CRITICAL introduction triggering immediate DIVERGENCE_WARNING

**Minor Observation:** The Round 1 example showed Net=-4 triggering divergence. With D=8 default, Net=-4 would NOT trigger divergence (since -4 > -8). This is actually a fix: the threshold is now appropriate. Net=-4 enters FLAT state, not immediate divergence.

**Verdict:** RESOLVED. Examples are now consistent with rules.

---

### State Machine Completeness Assessment

#### States Defined

| State | Definition | Entry Conditions | Exit Conditions |
|-------|------------|------------------|-----------------|
| CONVERGING | Normal progress | Initial, recovery from FLAT, recovery from DIVERGENCE_WARNING | weighted_net <= 0 OR weighted_net < -D |
| FLAT | No net progress, watching | weighted_net <= 0 from CONVERGING | weighted_net > 0 OR flat_count >= threshold OR weighted_net < -D |
| STALLED | Flat count exceeded | flat_count >= stall_threshold | Always to DIVERGENCE_WARNING (transient) |
| DIVERGENCE_WARNING | User intervention required | weighted_net < -D, new CRITICAL, or via STALLED | User action |
| PAUSED | Session saved | User: "Pause for later" | Resume (external) |
| COMPLETE | Session ended | User: "Force complete" or normal termination | Terminal |

**Assessment:** All states have clear entry/exit conditions. STALLED is correctly identified as transient (immediately transitions to DIVERGENCE_WARNING).

#### Transitions Verified

| From | To | Trigger | Verified |
|------|-----|---------|----------|
| CONVERGING | CONVERGING | weighted_net > 0 | YES |
| CONVERGING | FLAT | weighted_net <= 0 AND >= -D | YES |
| CONVERGING | DIVERGENCE_WARNING | weighted_net < -D OR new CRITICAL | YES |
| FLAT | CONVERGING | weighted_net > 0 | YES |
| FLAT | FLAT | weighted_net <= 0 AND flat_count < threshold | YES |
| FLAT | STALLED | weighted_net <= 0 AND flat_count >= threshold | YES |
| FLAT | DIVERGENCE_WARNING | weighted_net < -D OR new CRITICAL | YES |
| STALLED | DIVERGENCE_WARNING | always | YES |
| DIVERGENCE_WARNING | CONVERGING | User: Narrow/Accept | YES |
| DIVERGENCE_WARNING | PAUSED | User: Pause | YES |
| DIVERGENCE_WARNING | COMPLETE | User: Force/Abandon | YES |

**Assessment:** No missing transitions. State machine is complete.

#### Edge Cases

- **Immediate CRITICAL:** Handled by override path
- **Multiple consecutive FLAT rounds:** flat_count increments correctly
- **Recovery then immediate regression:** Returns to FLAT/DIVERGENCE_WARNING appropriately
- **Session saved in PAUSED:** Status.md format includes state (implicit resume support)

---

### Threshold Configuration Assessment

**Configuration Approach:**
- Set at session initialization
- Recorded in status.md
- Three presets by spec complexity

**Soundness Check:**

| Parameter | Default | Rationale | Assessment |
|-----------|---------|-----------|------------|
| stall_threshold | 2 | Progress expected every round | SOUND - conservative default |
| divergence_threshold | 8 | 1 CRITICAL or 2 HIGH triggers | SOUND - matches severity weights |
| critical_override | true | CRITICAL is categorical | SOUND - safety net |

**Configuration Guidance:**
- Simple (<10 gaps): 2/8 - REASONABLE
- Complex (10-30 gaps): 3/12 - REASONABLE (allows one more flat round, tolerates one HIGH introduction)
- Large (30+ gaps): 4/16 - REASONABLE (complex specs need exploration time)

**Concern:** The guidance doesn't specify whether these are "hard" presets or starting points. Recommend:

> "Guidance values are starting points. User may adjust based on session characteristics."

---

### New Issues Identified

#### HIGH Priority

- **ISSUE-R3-001**: FLAT -> STALLED transition condition is off-by-one in Example 3
  - Location: Example 3, Round 6 shows flat_count=3 >= stall_threshold=2
  - Impact: If stall_threshold=2, then flat_count=2 should trigger STALLED (not 3). Example shows 3 rounds of flat progress but threshold is 2.
  - Clarification Needed: Is stall_threshold "number of flat rounds to tolerate" (then 2 means trigger on 3rd) or "flat_count threshold" (then 2 means trigger when flat_count reaches 2)?
  - Suggestion: Clarify in definition: "STALLED triggers after stall_threshold consecutive rounds of non-progress (i.e., on round stall_threshold + 1 of flat progress)."

- **ISSUE-R3-002**: AskUserQuestion format diverges from Round 2 standard
  - Location: AskUserQuestion on Divergence (lines 189-237)
  - Impact: The format adds new sections (Cause Analysis, Current Gap Inventory) not present in GAP-FLOW-012's AskUserQuestion format. While improvements, they should be explicitly adopted as the new standard.
  - Suggestion: State: "This format extends GAP-FLOW-012 AskUserQuestion with Cause Analysis and Inventory sections. This becomes the standard for divergence-related questions."

#### MEDIUM Priority

- **ISSUE-R3-003**: "Prioritize CRITICAL" option (Option 3) has unclear mechanics
  - Location: AskUserQuestion Option 3
  - Impact: "Pause all other work until CRITICAL resolved" - how is this enforced? Does Mediator reject proposals for non-CRITICAL gaps? Does it filter the priority list?
  - Suggestion: Define operationally: "Engineer's next round is scoped to CRITICAL gap only. Other gaps remain IN_PROGRESS but are not assigned."

- **ISSUE-R3-004**: PAUSED state lacks resume mechanism
  - Location: State definitions, "Session saved" for PAUSED
  - Impact: PAUSED is defined but resume is "(external)". How does resume work? New session? Same session ID? State restored from where?
  - Suggestion: Defer to GAP-FLOW-007 (Rollback/Resume) or create GAP-FLOW-016 for session resume. Note dependency.

- **ISSUE-R3-005**: status.md format shows Flat Count column but no guidance on when to display
  - Location: Recording in status.md, Round Progress table
  - Impact: Flat Count is 0 for CONVERGING, 1-N for FLAT, "-" for DIVERGENCE_WARNING. The "-" notation isn't explained.
  - Suggestion: Add: "Flat Count shows N during FLAT state, 0 after recovery to CONVERGING, '-' when state is DIVERGENCE_WARNING or terminal."

#### LOW Priority

- **ISSUE-R3-006**: Baseline reset on "Accept complexity" is underspecified
  - Location: DIVERGENCE_WARNING -> CONVERGING transition via "Accept complexity"
  - Impact: "baseline reset" means what? Is open_gap_count reset? Is weighted history cleared? Does this affect future divergence detection?
  - Suggestion: Define: "Accept complexity resets flat_count to 0 but does NOT reset gap counts. Next round's net progress is calculated from current state."

- **ISSUE-R3-007**: Severity breakdown table math inconsistency in example
  - Location: Recording in status.md, Severity Breakdown (Round 3)
  - Impact: Table shows Total resolved weight=2 and new weight=26, but detailed rows show CRITICAL new=1 (weight 16), HIGH new=2 (weight 8), MEDIUM resolved=1 new=2 (net -2). Total should be: resolved=2 (1 MEDIUM), new=16+8+4=28. The 26 appears incorrect.
  - Note: This is a documentation example, not logic error. Minor.

---

### Cross-Cutting Consistency

#### Consistency with GAP-FLOW-010

- [x] State transitions use lifecycle terminology correctly
- [x] USER_DEFERRED handling (0.5 weight) preserved
- [x] "Resolved" definition aligned

#### Consistency with GAP-FLOW-012

- [x] Severity weights match (with approved CRITICAL increase)
- [x] Weighted formula used appropriately
- [x] Termination criteria aligned

#### Consistency with Previous Rounds

- [x] Addresses all five Round 1 issues (R1-008 through R1-012)
- [x] Incorporates Round 2 recommendation (CRITICAL weight increase from R2-010)
- [x] Does not conflict with approved GAP-FLOW-003 (Handoff Mechanics)

---

## Summary

| Aspect | Assessment |
|--------|------------|
| Foundation Integration | EXCELLENT - Both GAP-FLOW-010 and GAP-FLOW-012 correctly integrated |
| Issue Resolution | COMPLETE - All five Round 1 issues (R1-008 through R1-012) resolved |
| State Machine | COMPLETE - All states and transitions well-defined |
| Threshold Configuration | SOUND - Justified defaults with appropriate flexibility |
| Examples | GOOD - Demonstrate key scenarios; minor math inconsistency |

### Issue Summary

| Severity | Count | Issues |
|----------|-------|--------|
| CRITICAL | 0 | - |
| HIGH | 2 | R3-001 (off-by-one clarification), R3-002 (AskUserQuestion format adoption) |
| MEDIUM | 3 | R3-003 (Option 3 mechanics), R3-004 (PAUSED resume), R3-005 (Flat Count notation) |
| LOW | 2 | R3-006 (baseline reset), R3-007 (example math) |

---

## Verdict: APPROVED

GAP-FLOW-002 (Revised) is **APPROVED** for integration into the workflow specification.

**Rationale:**
1. All Round 1 blocking issues are resolved
2. Foundation integration is correct and complete
3. State machine is well-defined with no missing transitions
4. Threshold configuration is sound and flexible
5. HIGH issues are clarification requests, not fundamental flaws

**Must Address Before Final Integration:**
- ISSUE-R3-001: Clarify flat_count semantics (off-by-one ambiguity)
- ISSUE-R3-002: Explicitly adopt extended AskUserQuestion format

**May Address in Future Rounds:**
- ISSUE-R3-003 through R3-007 are refinements that don't block the core design

---

## Recommendation for Round 4

With GAP-FLOW-002 now APPROVED:

1. **Address clarifications** - R3-001 and R3-002 are minor but should be cleaned up
2. **Proceed to GAP-FLOW-001** (Error Recovery) - Now unblocked by GAP-FLOW-010 and GAP-FLOW-008 (if addressed)
3. **Consider GAP-FLOW-005** (Termination) - Can now use both lifecycle and severity foundations
4. **Define PAUSED resume** - R3-004 notes a gap; either expand GAP-FLOW-007 or create new gap

**Net Assessment:** Round 3 is a success. GAP-FLOW-002 is now implementable. The stall detection mechanism has objective metrics, appropriate thresholds, and clear user intervention points.

---

## Revision

| Date | Changes |
|------|---------|
| 2026-01-12 | Initial review of Round 3 revised GAP-FLOW-002 proposal |
