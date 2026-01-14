# Reviewer Assessment: GAP-EX-002

## Verdict: APPROVED_WITH_NOTES

## Summary

The engineer's proposal comprehensively addresses the error handling gap in manual edit mode across all three wizards (Pathfinder YAML, Parser Lab Python, Semantic Path YAML). The specification is **well-structured, implementable, and consistent** with existing patterns in the codebase. Error categories are concrete, validation timing is sound, and recovery workflows are intuitive.

However, there are **minor gaps around edge cases and validation order** that should be clarified before implementation begins, primarily around what happens when multiple errors are discovered and the priority order for showing them.

**Confidence Level:** HIGH - Ready for implementation with noted revisions.

---

## Checklist

- [x] **Completeness**: Covers all error types for all three wizards
- [x] **Consistency**: Aligns with existing EDITING/VALIDATING state patterns
- [x] **Implementability**: Clear detection mechanisms and UI patterns
- [x] **Testability**: E2E test scenarios provided; success criteria defined
- [x] **State Machine Alignment**: Integrates cleanly with existing diagrams
- [x] **Recovery Workflows**: Three-level recovery is intuitive and complete
- [x] **Error Messages**: Concrete examples with code context
- [x] **Performance**: Validation timing (on-save, not on-type) is appropriate
- [ ] **Edge Cases**: Minor gaps on error prioritization and ordering (see Concerns)
- [ ] **Retry Limits**: Table provided but some edge cases undefined (see Concerns)

---

## Detailed Findings

### Strengths

1. **Comprehensive Error Categorization**
   - Section 1.1-1.3 cleanly separates syntax errors, schema errors, semantic/logic errors, and runtime errors
   - Detection mechanisms are concrete: `yaml.load()`, `ast.parse()`, sandbox execution
   - Severity levels (CRITICAL, HIGH, MEDIUM) provide clear prioritization

2. **Validation Timing Strategy is Sound**
   - Two/three-stage validation (on-save, not on-type) avoids performance penalty
   - Clear distinction between syntax validation (fast) and runtime testing (slow)
   - Aligns with existing `VALIDATING` state in spec

3. **State Machine Integration**
   - New states (VALIDATION_ERROR) fit naturally into existing Pathfinder/Parser Lab/Semantic diagrams
   - Transitions are well-defined in Section 7 with guards and actions
   - Reuses existing recovery options ([e], [r], [Esc]) familiar to users

4. **Error Message Design**
   - Sections 3.1-3.2 and 4 provide concrete examples with context
   - Line/column numbers, code excerpts, suggestions are present
   - Toast + detail dialog pattern is clear and progressive disclosure works well

5. **Three-Level Recovery** (Section 6)
   - Edit → Regenerate → Discard covers most failure modes
   - Unlimited edit cycles without artificial limits (good UX)
   - Error context preservation (line numbers, temp files) is thoughtful

6. **Implementation Roadmap**
   - Section 8 breaks down work into 6 phases
   - Phase-to-phase dependencies are implicit but reasonable
   - E2E test scenarios in Section 5 are specific and measurable

7. **Decision Transparency**
   - Section 9 shows "why" for each choice (on-save not on-type, 30s timeout, preserve original on discard)
   - Rationales align with practical constraints and UX goals

---

### Concerns

1. **Error Ordering and Prioritization (Implementability Risk: MEDIUM)**

   **Issue:** When multiple errors are found (e.g., YAML has 3 syntax errors + 2 schema errors), which is shown first?

   **Current Text (Section 4.2):**
   ```
   Error 1: Syntax Error (line 5)
   Error 2: Schema Error
   Error 3: Logic Error
   ```

   **Questions Unanswered:**
   - Are errors shown in detection order or severity order?
   - Should syntax errors block progress before schema errors are shown?
   - For runtime errors with 100 exceptions from large sample, show first 5 or all?
   - How are Python imports, type mismatches, and runtime errors ordered if all three fail?

   **Recommendation:** Add subsection to Section 4:
   ```markdown
   #### Error Display Ordering

   **Priority Order (highest → lowest):**
   1. Syntax errors (prevents loading)
   2. Semantic/Schema errors (missing fields, invalid types)
   3. Logic errors (pattern validation failures)
   4. Runtime errors (sample execution failures)
   5. Type mismatches (output schema validation)

   **Within category, order by:**
   - Line number (lowest first)
   - Error severity (HIGH before MEDIUM before LOW)

   **For multiple errors in same category:**
   - Show top 5 in toast detail, expand to all in modal
   - Example: "5 schema errors found (showing first 3)"
   ```

2. **Sandbox Error Capture: Exception Type Handling (Implementability Risk: LOW-MEDIUM)**

   **Issue:** Section 1.2 mentions "catch exceptions with traceback" but doesn't specify:
   - What if exception message is 5000 chars? Truncate?
   - Do we capture `sys.exc_info()` or just `str(exception)`?
   - Dataclass wrapper for parsed exception?

   **Current Text:** "Catch exceptions with traceback" (vague)

   **Recommendation:** Clarify in implementation checklist:
   ```markdown
   - [ ] Define Python exception capture struct:
      - exception_type: str (e.g., "ValueError")
      - exception_message: str (max 500 chars, truncate if longer)
      - traceback: Vec<StackFrame> with file, line, function
      - sample_data: Option<String> (the row that failed, if applicable)
   ```

3. **Retry Limits: Infinite Edit Cycles (Spec Clarity: LOW)**

   **Issue:** Section 6.2 says "Edit-validate cycles: Unlimited" but what about:
   - User edits 50 times, each failing? At what point do we suggest starting fresh?
   - Is there a UX pattern (e.g., "You've edited this 5 times, consider regenerating")?
   - Or truly unlimited, relying on user giving up?

   **Current Text:** "Unlimited | User can edit, validate, edit again indefinitely"

   **This is NOT a blocker**, but the intent should be clarified:
   - If truly unlimited, consider adding a "feedback loop detection" suggestion after 3-5 unsuccessful edits
   - If there's a limit, document it

   **Recommendation:** Minor update to Section 6.2:
   ```markdown
   | Edit-validate cycles | Unlimited | User can cycle indefinitely. After 5 unsuccessful edits, UI suggests "Consider regenerating from scratch [r]" |
   ```

4. **Python Sandbox Limits: Memory Monitoring Mechanism (Implementability Risk: LOW)**

   **Issue:** Section 1.2 and 6.2 mention "Memory limit: 512MB" but doesn't specify:
   - How is memory monitored? `resource.setrlimit()`? `psutil`?
   - What happens when limit is exceeded? Kill immediately or warn?
   - Is 512MB per process or shared with other operations?

   **Recommendation:** Add to implementation checklist:
   ```markdown
   - [ ] Implement sandbox process memory monitoring:
      - Use resource.setrlimit(resource.RLIMIT_AS, 512*1024*1024)
      - Or use psutil to monitor child process RSS
      - Kill process and emit MemoryExhaustion error on exceed
      - Test with intentional memory hog to verify
   ```

5. **VALIDATING State UI: Spinner vs Progress (UX Detail: LOW)**

   **Issue:** Section 2.2 shows a spinner with staged progress:
   ```
   [⠋] Syntax check
   [ ] Sample test
   [ ] Schema validation
   ```

   This is good, but what if:
   - Stage 1 takes 50ms, stage 2 takes 3s, then stage 3? Should user see "Step 1 of 3"?
   - If only 1 stage runs (e.g., YAML has no samples), show 1 item or 3 grayed-out?

   **Recommendation:** Clarify in UI spec section:
   ```markdown
   **Dynamic Stage Display:**
   - Show only stages relevant to current validation
   - Example YAML: Show only "Syntax check", "Schema check", "Glob pattern test" (3 items)
   - Example Python: Show "Syntax check", "Method validation", "Sample execution" (3 items)
   - Mark completed stages with ✓, in-progress with spinner, pending with [ ]
   ```

6. **VALIDATION_ERROR Dialog: Modal Stack Depth (State Machine Detail: LOW)**

   **Issue:** What happens if user presses [e] from VALIDATION_ERROR, makes more edits, hits errors again?
   - Is there a new VALIDATION_ERROR modal? Or does it replace?
   - Can user get stuck in nested dialogs?
   - Should there be a "back" button on error dialogs?

   **This is likely handled by the Dialog framework, but should be documented.**

   **Recommendation:** Add note to Section 5.3:
   ```markdown
   **Modal Behavior on Retry:**
   - VALIDATION_ERROR modal replaces (not stacks) previous result modal
   - Multiple edit attempts each show fresh VALIDATION_ERROR modal
   - [Esc] always closes error modal, returns to RESULT_* (not previous state)
   - This prevents modal nesting and keeps escape path simple
   ```

---

### Recommendations

1. **Before Implementation:**
   - Add error ordering section (Section 4.4) specifying priority
   - Clarify exception capture dataclass in implementation checklist
   - Add "feedback loop suggestion" after 5 failed edits (UX polish)
   - Document sandbox memory monitoring mechanism

2. **During Implementation:**
   - Create error type enum with variants for each category (Section 8, Phase 2)
   - Add comprehensive test for multi-error scenarios (e.g., YAML with 3 syntax + 2 schema errors)
   - Test exception handling edge cases (long messages, nested exceptions)

3. **After Implementation:**
   - Update `specs/ai_wizards.md` Section 5.1.1 and 5.2.1 with new diagrams showing VALIDATION_ERROR state
   - Add Section 4.3 "Error Handling and Recovery" with full error categories table
   - Update keybindings tables to show VALIDATION_ERROR transitions

---

## New Gaps Identified

### GAP-EX-003: Error Ordering Priority (DERIVED)
**Scope:** When multiple validation errors are detected, specify display order.
**Impact:** Medium - affects UX clarity but not core functionality.
**Suggested Priority:** MEDIUM - implement in Phase 2 (Error Messages).
**Owner:** Reviewer created during GAP-EX-002 review.

### GAP-EX-004: Sandbox Process Resource Limits (DERIVED)
**Scope:** Clarify mechanism for enforcing 512MB memory limit and 30s timeout in sandbox.
**Impact:** Low - mostly implementation detail.
**Suggested Priority:** LOW - document in implementation checklist (Phase 1).
**Owner:** Reviewer created during GAP-EX-002 review.

### GAP-EX-005: Feedback Loop Detection (DERIVED)
**Scope:** After N failed edit-validate cycles, suggest regenerating from scratch.
**Impact:** Low - UX polish, not critical path.
**Suggested Priority:** LOW - polish feature after Phase 3.
**Owner:** Reviewer created during GAP-EX-002 review.

---

## Closing Comments

This is a **solid, implementable proposal** that directly addresses the gap. The three-level recovery pattern is intuitive, error messages are concrete with good examples, and state machine integration is clean. The engineer has done good work identifying error categories and specifying them with detection mechanisms.

The concerns raised are **minor clarifications**, not blockers. Most are implementation details or UX polish that won't block the core feature. The spec is ready for coding with these notes in mind.

**Next Steps:**
1. Engineer reviews notes, particularly error ordering priority (Section 4.4)
2. If agreed, proceed to Phase 1 implementation (error detection infrastructure)
3. Reviewer conducts code review once Phase 1 is complete
4. Iterate through remaining phases with running review

---

## Sign-Off

**Reviewer:** Claude Code
**Date:** 2026-01-13
**Confidence:** HIGH
**Status:** APPROVED WITH NOTES - Proceed to implementation with noted clarifications.

