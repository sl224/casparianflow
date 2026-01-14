## Review: GAP-STATE-004

### Validation: PASS

### Issues: LOW

### Recommendation: ACCEPT

---

### Validation Details

#### 1. State Machine vs TUI Mockup (Section 5.5)

| Mockup Element | State Machine Coverage | Status |
|----------------|------------------------|--------|
| `[Enter] Create Rule` | RESULT_HIGH_CONFIDENCE -> APPROVED | PASS |
| `[a] Alternatives` | RESULT_* -> ALTERNATIVES_VIEW | PASS |
| `[e] Edit` | RESULT_* -> EDITING | PASS |
| `[h] Hint` | RESULT_* -> HINT_INPUT | PASS |
| `[Esc]` | All states have exit path | PASS |
| Confidence bar (94%) | Differentiates HIGH (>=80%) vs LOW (<80%) | PASS |
| Similar Sources section | `SimilarSource` struct, display-only | PASS |
| Path Breakdown | `SegmentAnalysis` struct | PASS |
| Tag input field | `tag_name` in `SemanticResultData` | PASS |

**All keybindings from mockup are covered in transition table.**

#### 2. Alternatives View

- Properly defined as separate state with j/k navigation
- Returns to previous result state on Esc
- Enter triggers REGENERATING with selected alternative
- `AlternativesViewData` includes `previous_state: Box<SemanticPathState>` for correct return behavior

**Well-specified.**

#### 3. Confidence-Based Dual States

| Aspect | Implementation | Assessment |
|--------|----------------|------------|
| HIGH threshold | >= 80% | Reasonable default |
| LOW behavior | Amber indicator, "a" highlighted as suggested | Good UX guidance |
| LOW approval | Requires confirmation "[y/N]" per keybinding note | Prevents accidental low-confidence commits |
| Both allow all actions | Enter/a/e/h/Esc available | Correct - not blocking |

**Makes sense. Users are guided toward alternatives when confidence is low, but not blocked.**

#### 4. Similar Sources Display

- Defined as read-only display section in result states
- `SimilarSource` struct captures source_id, name, expression, file_count
- ASCII mockup in engineer doc matches spec mockup
- Explicitly notes "bulk application happens through Sources Manager"

**Correctly scoped - discovery only, action elsewhere.**

#### 5. Minor Observations (Not Blocking)

1. **GAP-SEMANTIC-004 (confirmation dialog)**: Engineer notes low-confidence approval shows "[y/N]" but doesn't specify if it's modal or inline. Minor - can be resolved during implementation.

2. **No 'r' key for regenerate**: Unlike Pathfinder, Semantic Path Wizard doesn't have explicit 'r' keybinding in mockup. Engineer correctly omits it - regeneration happens after hint/edit/alternative selection. Consistent with mockup.

3. **RECOGNITION_ERROR has 'r' for retry**: Correct - this is the only context where direct retry makes sense (initial recognition failure).

---

### Summary

The state machine is comprehensive and accurately reflects the TUI mockup in Section 5.5. Key strengths:

- Confidence-based UX guidance without blocking user choice
- Alternatives view properly nested with return behavior
- Similar sources display is appropriately read-only
- All mockup keybindings are mapped
- Data model is complete with semantic primitives

The 5 new gaps identified (SEMANTIC-001 through 005) are appropriate follow-up items for implementation phase, not blockers for this state machine specification.
