## Review: GAP-STATE-002 (Revision 2)

**Proposal:** Parser Lab Wizard State Machine v2
**Author:** Engineer
**Reviewer:** Claude Opus 4.5
**Date:** 2026-01-13

---

### Previous Issues Status

| Issue | Severity | Status | Notes |
|-------|----------|--------|-------|
| H1: EDITING -> REGENERATING contradiction | HIGH | RESOLVED | Engineer added VALIDATING state. EDITING now transitions to VALIDATING (lines 185-186), which runs validation only without AI regeneration. User edits are preserved. Clear distinction documented in new section "VALIDATING vs REGENERATING Distinction" (lines 462-479). |
| H2: TESTING exit underspecified | HIGH | RESOLVED | Explicit transitions added (lines 181-184): TESTING -> RESULT_VALIDATED / RESULT_WARNING / RESULT_FAILED based on cumulative test results. Logic documented in `compute_cumulative_status` function (lines 704-722). |
| M1: Retry exhaustion in keybindings | MEDIUM | RESOLVED | Footnote 1 added to keybinding table (line 223). New Example 7 (lines 607-623) demonstrates retry exhaustion behavior. |
| M2: Missing VALIDATING state | MEDIUM | RESOLVED | VALIDATING state added with full documentation: state diagram (lines 77-78), state definition (line 133), transitions (lines 185-189), data model (lines 418-427), and distinction table (lines 466-478). |
| M3: Recursive Box types | MEDIUM | RESOLVED | Data structures simplified to use `return_to_status: ValidationStatus` + `result_data: ParserLabResultData` instead of `Box<ParserLabState>`. Applied to HintInputData (lines 399-405), EditingData (lines 408-416), TestingData (lines 373-384), and SchemaInputData (lines 349-361). |
| L4: Enter blocked explanation | LOW | RESOLVED | Footnote 2 added to keybinding table (line 224) describing UX: "Enter is blocked when in RESULT_FAILED. UI shows red border with message 'Fix errors to approve'." |

---

### New Issues Found

**None critical or high priority.**

**L1 (NEW): VALIDATING cancellation path**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Lines 185-189 | VALIDATING has explicit transitions to result states on completion, but keybinding table (line 212) shows Esc = "Cancel" for VALIDATING | Clarify: What does Esc during VALIDATING do? Return to EDITING? Return to previous result state? Cancel validation mid-flight? This is minor since validation is typically fast, but should be documented. |

**Suggested Resolution:** Add transition:
```
| VALIDATING | (previous result state) | Esc | User cancels validation |
```
Or note that validation is non-cancellable (runs to completion).

---

**L2 (NEW): ValidatingData lacks return_to_status**

| Location | Issue | Recommendation |
|----------|-------|----------------|
| Lines 418-427 | `ValidatingData` struct doesn't include `return_to_status: ValidationStatus` like other intermediate states | For consistency, consider adding `return_to_status` to handle Esc mid-validation. However, since VALIDATING always produces a definitive outcome, this may be intentional. |

---

### Verification of Key Fixes

**H1 Fix Verification (EDITING ambiguity):**

The revised proposal correctly addresses this:

1. ✅ New VALIDATING state added (line 246)
2. ✅ EDITING -> VALIDATING on "File modified" (line 185)
3. ✅ EDITING -> (previous result state) on "File unmodified" (line 186)
4. ✅ VALIDATING -> RESULT_VALIDATED / RESULT_WARNING / RESULT_FAILED (lines 187-189)
5. ✅ Clear distinction: VALIDATING = "validation only, no AI", REGENERATING = "AI regenerates code" (lines 466-478)
6. ✅ Example 6 updated to show VALIDATING flow (lines 586-603)
7. ✅ Trade-offs updated to note "VALIDATING preserves edits" (line 637)

**H2 Fix Verification (TESTING exit):**

The revised proposal correctly addresses this:

1. ✅ Explicit transitions: TESTING -> RESULT_VALIDATED / RESULT_WARNING / RESULT_FAILED (lines 181-183)
2. ✅ Cumulative result logic documented with code example (lines 704-722)
3. ✅ Testing sub-state machine updated with explicit exit paths (lines 676-699)
4. ✅ Example 5 clarified to show state transition based on cumulative results (lines 560-580)

---

### Summary

| Metric | Value |
|--------|-------|
| Previous issues resolved | 6/6 (100%) |
| New critical issues | 0 |
| New high issues | 0 |
| New medium issues | 0 |
| New low issues | 2 |

**Recommendation:** ACCEPT

The engineer has thoroughly addressed all HIGH and MEDIUM priority issues from the original review:

1. **H1 resolved:** The VALIDATING state elegantly solves the EDITING ambiguity. User edits are preserved through validation-only processing. The AI regeneration path (via 'r' key) remains available if validation fails and user wants AI help.

2. **H2 resolved:** TESTING exit transitions are now explicit with clear cumulative logic. The state machine correctly models how testing additional files can change the overall validation status.

3. **Data model improved:** Removing recursive `Box<ParserLabState>` in favor of `ValidationStatus` + `ParserLabResultData` simplifies implementation while maintaining all necessary information for state restoration.

The two new low-priority issues (L1, L2) are minor edge cases that can be addressed during implementation if needed. They do not block acceptance.

The revised state machine is implementation-ready.

---

### Next Steps

1. ✅ ACCEPT this proposal
2. Implementation can begin
3. Address L1/L2 during implementation if edge cases arise
4. Consider GAP-SCHEMA-001, GAP-TEST-001, GAP-VERSION-001 as separate future work items
