# Review: GAP-STATE-001

## Validation Checklist (per Section 15.6)

- [x] Can trace path from initial state to every other state
- [x] Can trace path from every state back to initial (or exit)
- [x] No two transitions from same state triggered by same key
- [x] Terminal states are explicitly marked
- [x] State names are semantic (not just "State1")

All five validation criteria pass. The state machine is well-designed.

---

## Critical Issues

None.

---

## High Priority

### [ISSUE-R1-001]: Missing REGENERATING escape path

**Location:** Transitions table, REGENERATING state

**Description:** The REGENERATING state has no Esc handler. If AI regeneration hangs or takes too long, the user is stuck with no way to cancel.

**Impact:** User cannot escape from a long-running regeneration. The only exit is to wait for completion or failure.

**Suggestion:** Add transition:
```
| REGENERATING | CANCELED | Esc | - |
```
Also add to State Definitions: "User can press Esc to cancel regeneration."

---

### [ISSUE-R1-002]: Tab keybinding documented but no state transition

**Location:** Keybinding Summary table, Tab key

**Description:** Tab is listed as "Focus next field" for YAML_RESULT and PYTHON_RESULT states, but there is no corresponding state or focus tracking in the data model.

**Impact:** Implementation will need to track focus state (name field vs pattern list) but this is not modeled.

**Suggestion:** Either:
1. Add `focused_element: FocusedElement` field to `PathfinderResultData` with enum for focus targets, or
2. Remove Tab from keybindings if single-field focus is sufficient (just the name field)

---

## Medium Priority

### [ISSUE-R1-003]: HintInput previous_state uses Box recursion

**Location:** Data Model, HintInputData struct

**Description:** `previous_state: Box<PathfinderState>` creates recursive type. While this compiles in Rust, it means HintInput stores a full copy of the previous result state including all patterns, preview, etc.

**Impact:** Memory duplication. More importantly, if the user makes multiple hints, each stores a snapshot, which is wasteful.

**Suggestion:** Store only what's needed to return:
```rust
pub enum PreviousResultKind {
    Yaml,
    Python,
}

pub struct HintInputData {
    pub input_text: String,
    pub cursor_position: usize,
    pub return_to: PreviousResultKind,
    // The result data is stored separately, not duplicated
}
```
This requires restructuring how state is managed (result data lives outside the state enum).

---

### [ISSUE-R1-004]: Diagram shows abstract "RESULT_SHOWN" but not in state definitions

**Location:** State Diagram vs State Definitions table

**Description:** The diagram shows "RESULT_SHOWN (abstract parent)" but this state does not appear in the State Definitions table or the Rust enum.

**Impact:** Confusion about whether this is an actual state. Since YAML_RESULT and PYTHON_RESULT share behavior, the abstract parent is conceptually useful but not implemented.

**Suggestion:** Either:
1. Remove RESULT_SHOWN from diagram (it's not a real state, just visual grouping), or
2. Document in a note: "RESULT_SHOWN is a conceptual grouping, not an implementation state"

---

### [ISSUE-R1-005]: Missing validation for empty hint submission

**Location:** Transitions table, HINT_INPUT state

**Description:** Guard says "Hint text non-empty" for Enter submission, but what happens if user presses Enter with empty text?

**Impact:** Undefined behavior. Should probably stay in HINT_INPUT or show error.

**Suggestion:** Add explicit handling:
- Option A: Enter with empty text does nothing (stays in HINT_INPUT)
- Option B: Enter with empty text returns to previous result state (equivalent to Esc)
Document the chosen behavior in the transition table.

---

### [ISSUE-R1-006]: EDITING state has no explicit keybindings

**Location:** Keybinding Summary table

**Description:** EDITING column shows "-" for all keys. This is correct (user is in external editor), but the state definition says "TUI shows waiting message" without specifying how to detect editor close.

**Impact:** Implementation needs to know: Does the TUI poll? Does it wait synchronously? What key brings focus back?

**Suggestion:** Add note to State Definitions:
"EDITING: TUI suspends, user is in $EDITOR. When editor process exits, TUI resumes and transitions based on file modification status."

This is also documented as GAP-TUI-001 in New Gaps Introduced, which is good.

---

## Low Priority / Nits

### [ISSUE-R1-007]: Inconsistent terminology: "rule" vs "extractor"

**Location:** Throughout, especially State Definitions and Examples

**Description:** YAML output is called "rule" but Python output is called "extractor". The spec uses both terms, which is correct per the parent spec, but the state names use "RESULT" not "RULE" or "EXTRACTOR".

**Impact:** Minor confusion. State names are neutral, which is actually good.

**Suggestion:** No change needed. Just noting for awareness.

---

### [ISSUE-R1-008]: DetectedPattern.field_type is String, not enum

**Location:** Data Model, DetectedPattern struct

**Description:** `field_type: String` could be `"integer"`, `"date_iso"`, etc. Using String allows arbitrary values.

**Impact:** No compile-time validation of field types.

**Suggestion:** Consider:
```rust
pub enum FieldType {
    Integer,
    DateIso,
    String,
    // etc.
}
```
Or document that String is intentional for extensibility.

---

### [ISSUE-R1-009]: PreviewResult.extracted uses HashMap<String, String>

**Location:** Data Model, PreviewResult struct

**Description:** All extracted values are strings, but some fields are typed (integer, date). The preview should show the typed value.

**Impact:** Preview might show "2024" for year when it should show `2024` (integer). Minor display issue.

**Suggestion:** Use `HashMap<String, serde_json::Value>` or a custom enum for typed preview values.

---

### [ISSUE-R1-010]: Example 5 shows "Press any key when done"

**Location:** Examples section, Example 5

**Description:** "TUI shows: 'Editing in vim... Press any key when done'" - but the TUI is suspended while editor runs. User cannot press a key in TUI.

**Impact:** Example is misleading. The TUI auto-resumes when editor closes.

**Suggestion:** Change to: "TUI suspends. When editor closes, validation runs automatically."

---

## Alignment with UI Mockup (Section 5.1)

Checked mockups against state machine:

| Mockup Element | State Machine Coverage | Status |
|----------------|----------------------|--------|
| Sample Path display | PathfinderResultData.sample_paths | OK |
| Output type indicator | YAML_RESULT vs PYTHON_RESULT states | OK |
| Detected Patterns panel | DetectedPattern struct | OK |
| Generated Rule/Code panel | generated_content field | OK |
| Preview panel | preview field, PreviewResult struct | OK |
| Name input field | name field | OK |
| [Enter] Approve | Transition to APPROVED | OK |
| [r] Regenerate | Transition to REGENERATING | OK |
| [e] Edit | Transition to EDITING | OK |
| [h] Hint | Transition to HINT_INPUT | OK |
| [Esc] Cancel | Transition to CANCELED | OK |

All keybindings from the mockup are covered.

---

## Summary

- **Issues found:** 10
  - Critical: 0
  - High: 2
  - Medium: 4
  - Low/Nits: 4

- **Recommendation:** NEEDS_REVISION

The state machine is fundamentally sound and covers all UI elements from the mockup. The two HIGH priority issues should be addressed:

1. **ISSUE-R1-001**: Add Esc escape from REGENERATING state (safety)
2. **ISSUE-R1-002**: Clarify Tab/focus handling or remove Tab keybinding

The MEDIUM issues are about data model refinement and can be addressed during implementation, but documenting intent now will help.

The new gaps identified (GAP-TUI-001, GAP-YAML-001, GAP-FOCUS-001) are appropriately flagged for future rounds.
