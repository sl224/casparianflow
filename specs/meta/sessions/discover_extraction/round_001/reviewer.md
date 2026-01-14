# Review: Round 1

**Date:** 2026-01-13
**Reviewer Role:** Critically evaluate Engineer's proposals against spec and implementation reality
**Gaps Reviewed:** GAP-STATE-001, GAP-TRANS-001

---

## Critical Issues

### ISSUE-R1-001: State diagram shows incorrect transition from Testing to Publishing

- **Location:** GAP-STATE-001 solution, state diagram line `│ p (publish, from Complete) │`
- **Impact:** The spec (Section 13.12.5) shows `p` to publish from Testing, but Engineer's diagram also says Testing has sub-phases (Running..., Complete). However, the existing Section 13.7 PUBLISH State shows a *separate* confirmation screen after publish, not an auto-transition. The diagram conflates Publishing confirmation step with the "confirm enter" inside Publishing.
- **Suggestion:** Clarify the Testing → Publishing transition:
  - Testing (Complete) → `p` → Publishing (Confirming) - this is a confirmation prompt, NOT auto-confirm
  - Publishing (Confirming) → `Enter` → Publishing (Saving, Starting)
  - Publishing (Saving) → auto → Published

  The diagram currently shows `Publishing` having Confirming/Saving/Starting as sub-states but doesn't show the Enter key to confirm. Section 13.7 clearly shows `[Enter] Confirm and publish` is required.

### ISSUE-R1-002: Esc from Testing inconsistent between diagram and table

- **Location:** GAP-STATE-001 solution, State Definitions Table row for Testing
- **Impact:** Diagram shows `Esc (cancel, to BROWSE)` but table says `Testing | Esc→Browse`. This contradicts Section 13.12.5 which says TEST state Esc goes to `Focused`, not Browse: `| Esc | Cancel, return to Focused |`
- **Suggestion:** The spec is authoritative. Esc from Testing should return to Focused (or the equivalent in the new unified model which would be EditRule, since you were editing a rule before testing). Actually, re-reading the spec: `| Esc | Cancel, return to Focused |` - this is from the existing spec and refers to the old `Focused` state. In the Engineer's model, this should map to returning to EditRule (where you came from), not Browse.

---

## High Priority

### ISSUE-R1-003: GAP-TRANS-001 solution allows `e` from Filtering only, but spec Section 13.12.4 EDIT RULE shows `e` already exists in that state

- **Location:** GAP-TRANS-001 Trigger Context Table
- **Impact:** The Engineer correctly identifies that `e` enters EditRule from Filtering, but there's a keybinding collision. Section 13.12.4 shows EDIT RULE state already has `e` for a different purpose (implied by the section heading, though not explicitly listed). Need to verify no collision.
- **Suggestion:** This is actually a non-issue on closer inspection - Section 13.12.4 does not list `e` as a key in EDIT RULE state. However, Section 13.12.5 TEST state *does* have `| e | Edit rule (return to Edit) |`. The proposal is consistent with this. Downgrade to verification note.

### ISSUE-R1-004: State diagram missing Publishing confirmation key (`Enter`)

- **Location:** GAP-STATE-001 solution, PUBLISHING box in diagram
- **Impact:** The diagram shows `Publishing` with `Esc` going back to EditRule, but doesn't show the `Enter` key to confirm. Section 13.9.5 (listed as 13.9.5 in the spec though numbered oddly) shows: `| Enter | Confirm publish |`
- **Suggestion:** Add explicit `Enter` transition from Publishing (Confirming) to the saving/starting sub-state. The current diagram makes it look like Publishing auto-proceeds, which contradicts the spec's confirmation requirement.

---

## Medium Priority

### ISSUE-R1-005: Navigation Layer vs Rule Editing Layer terminology not in existing spec

- **Location:** GAP-STATE-001 solution, diagram header
- **Impact:** The "Navigation Layer" and "Rule Editing Layer" terminology is a useful conceptual addition but is not present in the existing spec. This should be noted as a proposed enhancement rather than a direct replacement.
- **Suggestion:** Mark this as "PROPOSED: Two-layer conceptual model" and explicitly call out that this is a new framing not currently in Section 13.3.

### ISSUE-R1-006: Diagram shows `j` for job status in Published, but spec uses different key

- **Location:** GAP-STATE-001 solution, Published state shows `Enter/Esc → [Return to BROWSE at root]`
- **Impact:** Section 13.7 shows `[j] View job status` as a key in the published state, but the Engineer's diagram doesn't include this transition. The state machine should be complete.
- **Suggestion:** Add `j → [View job status screen]` as a transition from Published state, or explicitly note it's omitted for simplicity.

### ISSUE-R1-007: GAP-TRANS-001 Template Matching flow adds complexity not in original gap

- **Location:** GAP-TRANS-001 solution, "Alternative: `e` on Selected File for Template Matching" section
- **Impact:** The original gap was about clarifying what `e` means. The Engineer has added an entirely new flow (template matching from single file) that references Phase 18g. This creates scope creep and potentially a new gap (GAP-TMPL-001) that wasn't in the original 12.
- **Suggestion:** Keep the core resolution ("`e` requires Filtering state with matches > 0") as the primary answer. Move the template matching discussion to the "New Gaps Introduced" section or explicitly mark it as "DEFERRED TO PHASE 18g".

---

## Low Priority / Nits

### ISSUE-R1-008: Esc from Published goes to "Browse (root)" but Section 13.7 says "Return to explorer"

- **Location:** GAP-STATE-001 solution, State Definitions Table, Published row
- **Impact:** Minor terminology inconsistency. "Browse (root)" vs "Return to explorer". The spec is vague about exact return location.
- **Suggestion:** Accept Engineer's proposal (return to root) but note this is a design decision that should be confirmed. Alternative: return to the prefix where user started the rule creation.

### ISSUE-R1-009: Diagram uses emoji (folder icon) not present in spec

- **Location:** GAP-TRANS-001 solution, State Diagram Annotation section with folder icons
- **Impact:** The spec uses plain text folder indicators ("> folder"). Emoji usage is inconsistent.
- **Suggestion:** Remove emoji, use consistent notation with existing spec patterns.

---

## Completeness Checks (State Machine Validation)

### REACHABILITY: PASS with notes
- [x] Browse → reachable (entry point)
- [x] Filtering → reachable via `/` from Browse
- [x] EditRule → reachable via `e` from Filtering (when matches > 0)
- [x] Testing → reachable via `t` from EditRule
- [x] Publishing → reachable via `p` from Testing (Complete)
- [x] Published → reachable via auto-transition from Publishing

**Note:** The reachability is valid, but the trigger conditions are now explicit thanks to GAP-TRANS-001 resolution.

### ESCAPABILITY: PASS with notes
- [x] Browse → can exit via `g`/`Esc`
- [x] Filtering → can return to Browse via `Esc`
- [x] EditRule → can return to Browse via `Esc`
- [x] Testing → can return to Browse via `Esc` (ISSUE-R1-002 questions this)
- [x] Publishing → can return to EditRule via `Esc`
- [x] Published → can return to Browse via `Enter`/`Esc`

**Concern:** Esc chain from deep states should be validated. Testing → Esc → Browse loses context. Consider Testing → Esc → EditRule (preserving rule draft) as alternative.

### DETERMINISM: PASS
- [x] Each (state, key) pair maps to exactly one action
- [x] `e` in Browse = DISABLED (no conflict)
- [x] `e` in Filtering = EditRule (no conflict)
- [x] `e` in Testing = EditRule (no conflict - return to edit)
- [x] No duplicate keys in any state

### COMPLETENESS: NEEDS IMPROVEMENT
- [ ] `j` key in Published state not shown in diagram
- [ ] `Enter` key in Publishing state not shown in diagram
- [ ] All other keybindings appear in transition table

### CONSISTENCY: PASS with notes
- [x] `Esc` always cancels/goes back (though destination varies - see ISSUE-R1-002)
- [x] `Enter` always confirms/selects
- [x] `t` always means "test"
- [x] `e` always means "edit" (in contexts where it's enabled)

---

## Compression Opportunities

### COMPRESS-001: State definitions appear in both diagram AND table

- **Locations:**
  - State diagram (visual)
  - State Definitions Table (tabular)
- **Suggested extraction:** The table is more precise and should be the authoritative source. The diagram should focus on transitions only, with a reference to the table for state details.

---

## Verdict

**NEEDS_REVISION**

---

## Summary

The Engineer's proposals for GAP-STATE-001 and GAP-TRANS-001 are fundamentally sound and provide the missing clarity. However, there are 2 CRITICAL issues (incorrect Testing→Publishing transition flow, and Esc destination inconsistency with spec) and 2 HIGH priority issues (missing Enter confirmation in diagram, scope creep on template matching). After these are addressed, the unified state diagram will be ready for integration into Section 13.3.

### Required Changes Before Approval

1. **CRITICAL:** Fix Publishing confirmation flow - `Enter` must be shown as explicit trigger, not auto-transition
2. **CRITICAL:** Resolve Esc from Testing destination (spec says Focused, proposal says Browse)
3. **HIGH:** Add `j` key to Published state transitions
4. **MEDIUM:** Move template matching discussion to deferred section

### Recommendation for Round 2

Address the critical/high issues above, then proceed to resolve GAP-FIELD-001, GAP-TEST-001, GAP-DATA-001, GAP-NAV-001 as originally planned. The new gaps identified (GAP-CTX-001, GAP-TMPL-001) can remain in the backlog for Round 3.
