# Reviewer Assessment: GAP-FLOW-002

## Verdict: APPROVED_WITH_NOTES

## Summary

The engineer's proposal provides a comprehensive, well-structured resolution to GAP-FLOW-002 that thoroughly addresses invocation patterns, context requirements, state machine integration, and differentiation from Pathfinder. The specification is implementation-ready with clear keybindings, edge case handling, and phase breakdown. However, three areas require clarification before engineering begins: (1) the relationship between pre-detection in round_019 and RECOGNIZING state from round_004, (2) transition semantics when confidence falls below thresholds, and (3) explicit definition of the "Not Semantic" fallback dialog's state transitions.

## Checklist

- [x] Gap fully addressed
- [x] Consistent with existing patterns
- [x] Implementation-ready
- [x] Testable success criteria
- [x] No critical gaps introduced

## Detailed Findings

### Strengths

1. **Clear Entry Points (Section 1)**: Three well-differentiated entry points (`S` from Sources, `S` from Files, `W` menu) with explicit activation conditions and behavioral variations. Each flow is distinct and context-aware.

2. **Comprehensive Context Requirements (Section 2)**: Table clearly distinguishes required (Source ID, File List) from optional (User Hints, Custom Sample) context. Default behaviors specified for missing context.

3. **Pre-Detection Algorithm (Section 3)**: Fast algorithmic pass before LLM invocation is excellent design choice for cost control and UX responsiveness. Three result states (Clear Detection ≥80%, Ambiguous 40-80%, Not Semantic <40%) provide clear branching logic.

4. **Strong Wizard Differentiation (Section 4)**: Auto-selection heuristic (Table 4.1, Recommendation Logic 4.2) gives implementers concrete decision logic. The "recommend semantic if ≥60% confidence" rule is actionable.

5. **Complete State Machine (Section 5)**: Flow diagram is comprehensive with clear naming (SAMPLING → PRE-DETECTION → GENERATING → RESULT). State Definitions table provides entry/exit/triggers. Focus management guidance (Section 8.2) shows attention to UX polish.

6. **Thorough Edge Case Handling (Section 9)**: Covers empty sources, identical paths, cross-source selection with explicit error messages and recovery options.

7. **Implementation Phases (Section 10)**: Time estimates provided (6 days total, broken into 1-2 day chunks). Phase ordering is logical (entry points → integration → algorithm → dialogs → state → testing).

8. **Keybinding Conflict Resolution (Section 6.2)**: Context-aware handling of `S` key collision between Semantic Wizard and Source Manager is elegant - resolves purely through focus context rather than requiring separate keys.

9. **UX Flow Example (Section 7)**: Concrete mission_data scenario grounds abstract concepts with realistic ASCII mockups showing confidence indicators, extraction rules, and preview output.

### Concerns

**MAJOR (Implementation blocker):**

1. **Relationship Between Round 019 Pre-Detection and Round 004 RECOGNIZING State** (Sections 3 & 5)
   - Round 004 defines RECOGNIZING as "algorithmic + AI disambiguation" with possible RESULT_LOW_CONFIDENCE → REGENERATING cycle
   - Round 019 defines Pre-Detection as purely "algorithmic check" with LLM invocation only for ambiguous (40-80%) cases
   - **Ambiguity**: When does the LLM get involved? Section 3.1 says "Score confidence for each detected primitive" (algorithmic) but Section 5.1 shows GENERATING state only for "if ambiguous"
   - **Question**: For Clear Detection (≥80%), does LLM run at all? The state machine says PRE-DETECTION directly to RESULT without GENERATING, but Section 3.1 shows both algorithmic AND LLM paths.
   - **Recommendation**: Clarify whether RECOGNIZING = PRE-DETECTION + optional GENERATING, or if PRE-DETECTION is a new fast path that bypasses RECOGNIZING entirely.

2. **"Not Semantic" Fallback Dialog Lacks Clear Exit States** (Section 3.3)
   - Dialog offers `[p]` Pathfinder, `[m]` Show Anyway, `[Esc]` Cancel
   - **Missing**: What state do we enter when user selects each option?
   - `[p]` → "Cancel current wizard, launch Pathfinder" (Section 4.3) - but from which state? PRE-DETECTION or RESULT?
   - `[m]` → "Show partial/ambiguous results" - but this was a <40% confidence result, so what "results" are shown? A RESULT state that doesn't meet confidence threshold? This seems to violate the separation between HIGH/LOW confidence states defined in Section 5.
   - **Recommendation**: Add explicit state transitions for "Not Semantic" fallback (e.g., "NOT_SEMANTIC_DIALOG" state with exits to PATHFINDER_SWITCHED, RESULT_AMBIGUOUS_FORCED, or CANCELED).

3. **5-File Sample Across 500+ File Sources** (Sections 2.1, 3.1)
   - Spec says "Maximum: 500 files (limit for cost, use clustering)" but then says "Random stratified sample (balanced depths)"
   - **Ambiguity**: How does stratified sampling work for 500 files? What if all 500 have identical structure? What if they have wildly varying depths?
   - Section 1.2 mentions "sampling" but doesn't define the algorithm
   - **Recommendation**: Define stratified sampling algorithm explicitly (e.g., "Group by folder depth, sample 1 file per depth level up to 5 files").

**MINOR (Polish / Clarity):**

4. **Minimum File Requirement Context Sensitivity**
   - Section 2.1 says minimum is "1 file (with AI)" but Section 1.1 says "3+ files (minimum for meaningful analysis)"
   - This is resolved in Section 7.1 with "Need 3+ files for reliable analysis" warning dialog, but the minimum definition in 2.1 could be clearer
   - **Recommendation**: In Section 2.1 table, clarify "Minimum: 1 (with lower confidence), Recommended: 3+" to make distinction explicit upfront.

5. **Keybinding `W` Menu Integration Lacks Detail**
   - Section 1.2 says "If already focused on a source: skip source selection, proceed directly to analysis"
   - **Question**: When menu shows up after pressing `W`, how does it know if you're focused on a source? Is the menu context-aware?
   - **Recommendation**: Clarify whether menu shows "Source already selected: /mnt/mission_data. Proceeding..." or if source selection dialog appears conditionally.

6. **Hint Re-analysis Flow**
   - Section 5.1 shows `[h]` Hint key from RESULT_SHOWN, leading back to RECOGNIZING
   - Section 7.1 shows `[h] Hint (re-analyze with hint)` as action
   - **Clarity**: Is hint an optional input at start (Section 2.2) AND a re-analysis trigger mid-wizard? Or only re-analysis?
   - Spec currently supports both (optional at start for SAMPLING, triggerable at RESULT) - this is fine, but the two flows could be clearer in sequence.

### Recommendations

**For APPROVED_WITH_NOTES:**

1. **Add Section 3.4: "Pre-Detection vs RECOGNIZING State"**
   - Clarify relationship to Round 004
   - Specify: "PRE-DETECTION is equivalent to the RECOGNIZING state from Round 004. For Clear Detection (≥80%), we skip the LLM and go directly to RESULT_HIGH_CONFIDENCE. For Ambiguous (40-80%), we transition to a GENERATING state that calls the LLM for disambiguation."
   - This resolves the conceptual alignment between rounds.

2. **Expand Section 3.3: Add State Transition Diagram**
   ```
   NOT_SEMANTIC_FALLBACK (confidence < 40%)
   ├── [p] → PATHFINDER_SWITCH_REQUESTED
   │          (wizard switched, sample paths preserved)
   ├── [m] → RESULT_SHOWN (with low-confidence results forced display)
   │         (user sees detected patterns even though <40%)
   └── [Esc] → CANCELED
   ```

3. **Add Section 2.1.1: Stratified Sampling Algorithm**
   - Define how 500 files → 5 files sample
   - Example: "Group files by depth (number of path segments). From each depth group, sample randomly up to 1 file. Repeat until 5 files selected or all depth groups exhausted. If fewer than 5 files available, use all."

4. **Refine Section 1.2 Wizard Menu Behavior**
   - Add: "When user presses `W` in Discover mode, the system checks: Is a source currently focused in the Sources dropdown? If yes, skip source picker. If no, show source picker dialog first."

5. **Verify Against Round 004 State Names**
   - Round 019 uses: SAMPLING, PRE-DETECTION, GENERATING, RESULT, APPROVED, CANCELED
   - Round 004 uses: RECOGNIZING, RESULT_HIGH_CONFIDENCE, RESULT_LOW_CONFIDENCE, RECOGNITION_ERROR, etc.
   - **Action**: Align naming or explicitly document why new names were chosen (e.g., "For clarity, SAMPLING replaces initial state before RECOGNIZING").

## New Gaps Identified

### None Critical

The proposal is self-contained and does not introduce new gaps. However, three areas deserve tracking as implementation proceeds:

1. **Confidence Scoring Algorithm (MINOR)**
   - Section 3.1 defines "score confidence for each detected primitive" but doesn't specify the scoring formula
   - Not strictly required for spec approval (implementation can define), but engineers will need guidance
   - Consider: Add brief note "Scoring algorithm defined during Phase 3 implementation" and reference to semantic_path_mapping.md if it exists

2. **LLM Token Budget for Disambiguation (MINOR)**
   - Section 3.1 mentions "Calling LLM" for 40-80% ambiguous cases but doesn't specify prompt or token limits
   - Not blocking (MCP implementation will handle), but worth documenting
   - Consider: Add note "LLM disambiguation uses <500 tokens, focuses on eliminating false primitives based on 5-file sample"

3. **Rule Naming Convention (MINOR)**
   - Section 7.1 example shows `tag: mission_data` but doesn't specify how tag name is generated
   - Recommendation: Clarify "Tag name auto-generated from dominant primitive (e.g., entity_folder(mission) → mission_data) and is editable before approval"

---

## Final Checklist for Engineering

Before implementation begins:

- [ ] **Round 004 Alignment**: Ensure RECOGNIZING/PRE-DETECTION conceptual alignment is resolved (Section 3.4 addition)
- [ ] **Not Semantic Exit States**: Implement state transitions for fallback dialog (Section 3.3 diagram addition)
- [ ] **Stratified Sampling**: Define algorithm before Phase 3 (Section 2.1.1 addition)
- [ ] **Confidence Scoring**: Plan formula during Phase 3 kickoff (reference semantic_path_mapping.md)
- [ ] **E2E Test Coverage**: Ensure tests cover all three entry points + all edge cases from Section 9
- [ ] **Focus Management**: Test focus restoration after wizard completes (Section 8.2 behavior)

---

## References

- **Round 004**: GAP-STATE-004 (RECOGNIZING state machine)
- **Existing Spec**: specs/ai_wizards.md Section 3.4, Section 5.5
- **Discover Spec**: specs/views/discover.md (keybindings, focus, state)
- **Implementation**: crates/casparian/src/cli/tui/app.rs (DiscoverViewState enum)

---

## Sign-Off

**Recommended Action**: APPROVED_WITH_NOTES

Submit for implementation after addressing three MAJOR recommendations (Round 004 alignment, Not Semantic fallback states, stratified sampling algorithm). MINOR recommendations are polish and can be addressed during implementation.

The proposal is comprehensive, well-reasoned, and implementation-ready with clarifications.
