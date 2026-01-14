## Review: GAP-STATE-002

**Proposal:** Parser Lab Wizard State Machine
**Author:** Engineer
**Reviewer:** Claude Opus 4.5
**Date:** 2026-01-13

---

### Validation Checklist

- [x] Can trace path from initial state to every other state
  - ANALYZING is the entry point
  - All result states reachable from ANALYZING
  - All input/action states reachable from result states
  - Terminal states reachable from appropriate ancestors

- [x] Can trace path from every state back to initial (or exit)
  - All states have Esc path to CANCELED or CLOSED
  - HINT_INPUT, SCHEMA_INPUT, TESTING return to previous result state on Esc
  - EDITING returns to previous state or triggers REGENERATING
  - All paths ultimately lead to terminal states (APPROVED, CANCELED, CLOSED)

- [x] No two transitions from same state triggered by same key
  - Verified keybinding table (lines 186-196): Each key has single action per state
  - Enter blocked on RESULT_FAILED is explicitly noted (line 161)

- [x] Terminal states are explicitly marked
  - APPROVED, CANCELED, CLOSED all marked as terminal
  - State definitions table (lines 113-127) shows "(terminal)" label
  - Transitions table shows "(dialog closes)" for terminals

- [x] State names are semantic
  - Names clearly describe purpose: ANALYZING, RESULT_VALIDATED, HINT_INPUT, etc.
  - Tri-state result naming (VALIDATED/WARNING/FAILED) maps to validation outcomes
  - Consistent with Pathfinder naming conventions

---

### Critical Issues

None found.

---

### High Priority

**H1: EDITING state transition on editor close is ambiguous**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Transitions table, lines 171-172 | "Editor closes + File modified" -> REGENERATING; "Editor closes + File unmodified" -> previous state | Clarify: Is "regenerating" the right behavior? Trade-off section (line 534) acknowledges this may lose user changes. The mitigation (line 541) suggests "run validation ONLY" - but this contradicts the transition table. |

**Suggested Fix:** Align transition table with mitigation. Change:
```
| EDITING | REGENERATING | Editor closes | File modified (triggers re-validation) |
```
to:
```
| EDITING | VALIDATING | Editor closes | File modified (validation only, no regenerate) |
```

Or: Add VALIDATING state that only runs validation without AI regeneration.

---

**H2: TESTING state exit conditions underspecified**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Lines 169-170, 576-582 | TESTING returns to "(previous result state)" but may change validation status | Clarify: If testing reveals failures on additional files, does result state change from RESULT_VALIDATED to RESULT_FAILED? The sub-state diagram (lines 569-582) suggests results update, but transition table lacks explicit state change logic. |

**Suggested Fix:** Add explicit transitions:
```
| TESTING | RESULT_VALIDATED | Testing completes | All tests pass, no warnings |
| TESTING | RESULT_WARNING | Testing completes | Tests pass with warnings |
| TESTING | RESULT_FAILED | Testing completes | Some tests fail |
```

---

### Medium Priority

**M1: ANALYSIS_ERROR retry exhaustion transition**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Line 164 | `ANALYSIS_ERROR -> CLOSED when retry_count >= 3` on 'r' press | Guard should be stated more explicitly in state definitions. User pressing 'r' on 4th attempt goes to CLOSED, but keybinding table (line 194) shows 'r' = "Retry" without noting the limit. |

**Suggested Fix:** Add note to keybinding table:
```
| r | - | Regenerate | Regenerate | Retry (max 3) | - | - | - | - | - |
```

---

**M2: Missing VALIDATING state for edit-only validation**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Trade-off mitigation (line 541) | Suggests validation-only after edit, but no VALIDATING state exists | If we want validation without regeneration (per mitigation), add VALIDATING state or document that REGENERATING handles both cases. |

---

**M3: previous_state boxing in sub-state data structures**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Lines 326, 345, 363, 370 | `previous_state: Box<ParserLabState>` creates recursive enum boxing | This is correct Rust, but consider whether storing `ValidationStatus` or a simpler marker would suffice. Boxing the full state may complicate state machine implementation. |

**Suggested Alternative:**
```rust
pub struct HintInputData {
    pub input_text: String,
    pub cursor_position: usize,
    /// Return to this validation status on cancel
    pub return_to_status: ValidationStatus,
    /// Full result data to restore
    pub result_data: ParserLabResultData,
}
```

---

### Low Priority / Nits

**L1: Tab keybinding scope unclear**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Line 196 | Tab cycles fields in result states, schema rows in SCHEMA_INPUT | Consider documenting which fields Tab cycles through in result states (name, version, topic?). |

---

**L2: Typing keybinding scope in SCHEMA_INPUT**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Line 195 | "(typing)" = "Edit schema" in SCHEMA_INPUT | The sub-state machine (lines 549-560) shows Type Dropdown and Constraint Input, but keybinding table groups all as "Edit schema". Consider separating actions per sub-state focus. |

---

**L3: Diagram complexity**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Lines 36-81 | Main diagram is dense and hard to follow | The simplified linear view (lines 86-106) is more useful. Consider making it the primary diagram. |

---

**L4: Missing explicit "Enter blocked" in keybinding table**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Line 188 | RESULT_FAILED column shows "(blocked)" for Enter | Good that it's marked, but consider adding footnote explaining UX (red border, error message as mentioned in example line 451). |

---

### Consistency Check: Pathfinder State Machine (Section 5.1.1)

| Aspect | Pathfinder | Parser Lab | Verdict |
|--------|-----------|-----------|---------|
| Entry state | ANALYZING | ANALYZING | Consistent |
| Error state | ANALYSIS_ERROR | ANALYSIS_ERROR | Consistent |
| Result states | YAML_RESULT / PYTHON_RESULT | RESULT_VALIDATED / WARNING / FAILED | Different but appropriate (different output types) |
| Input states | HINT_INPUT | HINT_INPUT, SCHEMA_INPUT | Parser Lab adds schema editing - appropriate |
| Testing state | N/A | TESTING | Parser Lab adds testing - appropriate |
| Exit keys | Enter/Esc | Enter/Esc | Consistent |
| Hint key | h | h | Consistent |
| Edit key | e | e | Consistent |
| Regenerate key | r | r | Consistent |
| Terminal states | APPROVED, CANCELED, CLOSED | APPROVED, CANCELED, CLOSED | Consistent |

**Verdict:** Parser Lab state machine is appropriately more complex than Pathfinder due to:
1. Tri-state validation (pass/warn/fail) vs binary (yaml/python)
2. Additional SCHEMA_INPUT state for type editing
3. Additional TESTING state for multi-file validation

These extensions are well-justified and follow the same patterns.

---

### Coverage Check: Section 5.2 Keybindings

Keys from mockup (line 1055-1057):
- [Enter] Approve - Covered
- [t] Test more - Covered
- [r] Regenerate - Covered
- [e] Edit - Covered
- [h] Give hint - Covered
- [s] Set schema - Covered
- [Esc] Cancel - Covered

**Verdict:** All keybindings from TUI mockup are covered in the state machine.

---

### Summary

| Category | Count |
|----------|-------|
| Critical | 0 |
| High | 2 |
| Medium | 3 |
| Low/Nits | 4 |

**Recommendation:** NEEDS_REVISION

The state machine is well-designed and comprehensive. The two high-priority issues are implementation-blocking:

1. **H1** (EDITING behavior): The transition table contradicts the trade-off mitigation. Must decide: Does edit trigger regeneration or validation-only?

2. **H2** (TESTING state changes): Testing additional files can change validation status (VALIDATED -> FAILED). The return transitions need explicit state change logic.

Both issues are resolvable with small clarifications. Once addressed, this proposal is ready for implementation.

---

### Suggested Next Steps

1. Engineer addresses H1 and H2
2. Consider M2 (VALIDATING state) as part of H1 resolution
3. Round 003 for quick validation of fixes
4. Implementation can begin after Round 003 approval
