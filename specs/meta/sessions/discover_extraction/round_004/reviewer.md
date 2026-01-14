# Reviewer Response: Round 004

**Date:** 2026-01-13
**Focus:** 4 MEDIUM priority gaps (GAP-UI-001, GAP-INFER-001, GAP-HIST-001, GAP-ERR-001)
**Reviewer Role:** Validate proposals for consistency, feasibility, and completeness

---

## Review: Round 4

### GAP-UI-001 Review: EDIT RULE Layout

**Verdict:** APPROVED

**Assessment:**

The engineer has provided an excellent, comprehensive layout specification for the EDIT RULE state. The proposal successfully addresses all requirements:

1. **Four focus sections clearly shown**: The ASCII layout explicitly shows GLOB PATTERN (1/4), FIELDS (2/4), BASE TAG (3/4), and CONDITIONS (4/4), directly mapping to the `RuleEditorFocus` enum values.

2. **Visual focus indicators well-defined**: The double-line border (`+== ... ==+`) vs single-line border (`+-- ... --+`) differentiation is a clear, terminal-safe pattern that will render consistently. The `>>` cursor indicator adds additional clarity.

3. **Status bar hints per section**: Each focus section has a corresponding status bar hint that documents available keybindings, matching the existing discover.md pattern for context-sensitive help.

4. **Keybindings are comprehensive**: The proposal distinguishes between section-specific keybindings (e.g., `a` for add in FieldList, character input in GlobPattern) and global keybindings (Tab, Shift+Tab, t, Esc), which is consistent with the existing state machine patterns.

5. **Field edit sub-focus addressed**: The inline field editing UI with `[EDITING]` indicator and sub-field navigation is a nice addition that was implicitly needed but not specified in Phase 18b.

**Consistency check:**
- The Tab wrapping behavior (Glob -> Fields -> Tag -> Conditions -> Glob) matches how other dialogs handle section navigation in discover.md
- The `[n]` section numbers (1/4, 2/4...) are a new pattern but add helpful orientation without conflicting with existing UI conventions
- The live match count `[847]` in the GLOB section is consistent with the existing pattern of showing file counts in brackets

**Minor observations (not blocking):**
- The `i` key for "Infer fields from pattern" in FieldList could be documented in Phase 18c as well for consistency
- Consider whether Shift+Tab should be documented in the status bar hint (currently only Tab is mentioned)

---

### GAP-INFER-001 Review: Inference Confidence Thresholds

**Verdict:** APPROVED

**Assessment:**

The engineer has provided a well-reasoned confidence scoring system with clear, testable thresholds:

1. **Thresholds are sensible**:
   - HIGH >= 0.85: Represents high certainty - appropriate for "auto-accept" scenarios
   - MEDIUM 0.50 - 0.84: Reasonable "verify" range
   - LOW < 0.50: Clear "uncertain" territory

2. **Multi-factor scoring is robust**: The `ConfidenceFactor` enum captures the key signals:
   - `TypeConsistency`: Core signal based on sample uniformity
   - `PatternRecognition`: Bonus for recognized date/ID patterns (aligns with Phase 18c pattern primitives)
   - `ValueDistribution`: Handles categorical vs ID distinction
   - `SampleSize`: Penalizes insufficient data

3. **Algorithm is explainable**: The factor breakdown example shows users exactly why a field received its score, which aids debugging and trust.

4. **Visual indicators are clear**: The `++` for HIGH and `??` for MEDIUM/LOW provides quick visual scanning. The legend is helpful.

**Consistency check:**
- The inferred field display format matches the existing Phase 18c ASCII mockup structure
- The confidence levels align with how templates show confidence in Phase 18g (e.g., "82%", "52%")

**Minor observations (not blocking):**
- The worked Example 2 correction mid-calculation is slightly awkward in the doc but doesn't affect the specification
- ColorBlind accessibility mentioned in trade-offs - the `++` vs `??` symbols provide non-color differentiation, which is good

**Edge case coverage:**
- Empty samples: `SampleSize` factor handles this via penalty
- All values unique: `ValueDistribution` handles high unique_ratio case
- Mixed types: `TypeConsistency` naturally penalizes this

---

### GAP-HIST-001 Review: Histogram Rendering Details

**Verdict:** APPROVED

**Assessment:**

The engineer has provided precise, implementable specifications for histogram rendering:

1. **Bar width (12 chars)**: Reasonable for readability while fitting the 38-char column width. Provides enough granularity for proportional display.

2. **Max values (5)**: Appropriate for quick scanning. Trade-off table acknowledges "may miss long tail" which is acceptable for MEDIUM severity.

3. **Truncation (15 chars with "...")**: Clear algorithm. The `truncate_label` function is straightforward and handles edge cases.

4. **Proportional scaling**: The algorithm correctly ensures:
   - Zero count = empty bar
   - Non-zero count = at least 1 filled char
   - Max count = full bar
   - Proportional distribution between

5. **Two-column layout**: The 38+3+38 = 79 chars (within 80) math is correct. The `render_field_metrics_panel` function shows proper pairing logic.

**Consistency check:**
- The histogram format matches the Phase 18d mockup (`042 ████████████ 423`)
- The two-column separator `|` is consistent with other TUI dividers
- The `HistogramConfig` struct follows the existing pattern of configuration structs

**Edge cases explicitly handled:**
- Count = 0: Empty bar
- Skewed distribution: Minimum 1 char for non-zero
- Value label empty: Show "(empty)"
- Odd number of fields: Left-aligned single column
- Single field: Works correctly

**Minor observations (not blocking):**
- The visual example annotation is helpful but could be formatted as a code block for consistency
- Consider adding a config option for minimum bar chars to show (currently hardcoded to 1)

---

### GAP-ERR-001 Review: Error Handling in PUBLISH

**Verdict:** APPROVED

**Assessment:**

The engineer has provided comprehensive error handling that covers all failure modes and recovery paths:

1. **Error types are complete**:
   - `DatabaseConnection`: Infrastructure failure
   - `RuleNameConflict`: Business rule violation
   - `PatternConflict`: Warning scenario (not blocking)
   - `DatabaseWrite`: Persistence failure
   - `JobCreation`: Partial success scenario
   - `Cancelled`: User-initiated abort

2. **Recovery options are sensible**:
   - `Retry`: For transient failures
   - `EditRule`: For fixable conflicts
   - `Overwrite`: For intentional replacement
   - `Cancel`: Escape hatch

3. **State machine is clear**: The flow diagram shows all transitions with appropriate recovery paths. The Validating -> Saving -> StartingJob progression is correct.

4. **Error display layouts are user-friendly**:
   - Shows both the user's rule and the conflicting rule details
   - Provides context (existing rule creation date, ID)
   - Lists available recovery actions
   - Includes CLI fallback for job failure recovery

5. **Partial success handling is thoughtful**: The "Rule saved, but job creation failed" scenario correctly preserves the user's work and provides a CLI recovery path.

**Consistency check:**
- Error dialog style matches the existing dialog patterns in discover.md (double borders, centered, keybinding legend at bottom)
- The `handle_error_key` function follows the same pattern as other key handlers in the codebase
- SQL queries use `sqlx` which is the project standard per CLAUDE.md

**Edge cases handled:**
- Overwrite confirmation (Example 4 shows two-step confirmation)
- Pattern conflict as warning vs name conflict as error
- User can continue without job and run extraction later via CLI

**Minor observations (not blocking):**
- The overwrite confirmation dialog layout isn't shown (only mentioned in Example 4)
- Consider whether `[c] Continue anyway` for pattern conflict should be documented in the main error dialog layout

---

## Overall Verdict

**APPROVED**

### Summary

All four MEDIUM priority gaps have been resolved with high-quality, implementable specifications. The engineer's proposals are:
- **Consistent** with existing TUI patterns in discover.md
- **Complete** with edge cases explicitly addressed
- **Implementable** with clear algorithms, data structures, and layouts
- **Well-documented** with examples that demonstrate the specifications in practice

No new gaps were introduced. The proposals can proceed directly to spec integration.

### Recommended Next Steps

1. **Integrate into discover.md:**
   - Update Section 13.8 with GAP-UI-001 EDIT RULE layout
   - Add GAP-INFER-001 confidence thresholds to Phase 18c
   - Add GAP-HIST-001 histogram specification to Phase 18d
   - Add GAP-ERR-001 error handling to Phase 18e

2. **Implementation priorities (per engineer recommendation):**
   - GAP-UI-001: Required for basic rule editing UX
   - GAP-ERR-001: Required for production robustness
   - GAP-HIST-001: Required for TEST state usability
   - GAP-INFER-001: Required for field inference UX

3. **Consider for future rounds:**
   - Overwrite confirmation dialog layout (minor gap, LOW priority)
   - Pattern conflict warning vs error distinction in UI chrome (polish item)
