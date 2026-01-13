# Reviewer Assessment - Round 2

**Date:** 2026-01-12
**Reviewer:** Principal Engineer Instance
**Focus:** Foundational Definitions (GAP-FLOW-010, GAP-FLOW-012)
**Context:** Round 1 revealed circular dependencies. This round addresses the two most critical foundations.

---

## Review: GAP-FLOW-010 (Gap Lifecycle Definition)

### Critical Issues

None identified. The core state machine is well-designed.

### High Priority

- **ISSUE-R2-001**: IN_PROGRESS -> PROPOSED transition skips validation
  - Location: Transition Rules table, row 2
  - Impact: Transition requires "Proposal must exist in engineer.md" but says nothing about proposal QUALITY. An engineer can write "## Gap Resolution: GAP-XXX" with no content and transition state.
  - Suggestion: Add minimum content validation: "Proposal must include ### Proposed Solution section with non-empty content."

- **ISSUE-R2-002**: USER_DEFERRED transition only from OPEN state is too restrictive
  - Location: State Transition Diagram, "OPEN -> USER_DEFERRED"
  - Impact: User may want to defer a gap AFTER seeing a proposal fail twice (in NEEDS_REVISION state). Current FSM forces them to either push through or mark WONT_FIX.
  - Suggestion: Allow USER_DEFERRED transition from: OPEN, IN_PROGRESS, NEEDS_REVISION. Not from PROPOSED (let review complete) or ACCEPTED (already done).

- **ISSUE-R2-003**: ACCEPTED -> RESOLVED has ambiguous trigger
  - Location: Transition Rules table, row 8: "User confirms OR auto-confirm after 1 round"
  - Impact: "Auto-confirm after 1 round" needs clarification. Is this 1 round after ACCEPTED state, or 1 round of the entire session? What if session terminates before the round passes?
  - Suggestion: Define explicitly: "ACCEPTED transitions to RESOLVED when: (1) User explicitly confirms via AskUserQuestion, OR (2) Next round completes without user objection, OR (3) Session terminates with COMPLETE/GOOD_ENOUGH status."

- **ISSUE-R2-004**: NEEDS_REVISION -> PROPOSED doesn't require issue acknowledgment
  - Location: Transition Rules table, row 5: "Must reference issue IDs"
  - Impact: "Reference" is weak. Engineer can write "Per ISSUE-R1-001, I disagree" and move to PROPOSED without actually addressing the issue. This is the implicit disagreement problem from Round 1 (ISSUE-R1-028).
  - Suggestion: Strengthen: "For each CRITICAL/HIGH issue that caused NEEDS_REVISION, proposal must either: (1) demonstrate resolution, (2) explicitly DISAGREE with rationale, or (3) mark as DEFERRED with justification." Mediator validates before state transition.

### Medium Priority

- **ISSUE-R2-005**: 0.5 weighting for USER_DEFERRED is presented as arbitrary
  - Location: Counting Rules, "0.5 * count(USER_DEFERRED)"
  - Impact: Why 0.5? A deferred gap is still a gap. The weighting affects convergence math without clear justification.
  - Suggestion: Provide rationale: "0.5 reflects that deferred gaps are acknowledged (not forgotten) but not actively blocking session progress. User has made a decision, which is progress." Alternatively, make weight configurable.

- **ISSUE-R2-006**: "Blocking" column in status.md format is unclear
  - Location: State Recording example, "Blocking?" column with "Yes (on GAP-FLOW-010)"
  - Impact: The open question section mentions "blocked_by" field as optional recommendation. The example uses "Blocking?" which is the inverse relationship. Blocking vs blocked_by are different directions.
  - Suggestion: Standardize: Use `blocked_by: [GAP-XXX]` consistently. Example should show: "| GAP-FLOW-002 | NEEDS_REVISION | 1 | 3 | GAP-FLOW-010 |" with column header "Blocked By".

- **ISSUE-R2-007**: No transition from USER_DEFERRED back to OPEN
  - Location: State Transition Diagram
  - Impact: If user defers in Round 2 but wants to revisit in Round 5, there's no defined path. Gap is stuck at 0.5 weight forever.
  - Suggestion: Add: "USER_DEFERRED -> OPEN: User explicitly reopens gap via AskUserQuestion."

### Low Priority / Nits

- **ISSUE-R2-008**: State diagram ASCII art has visual inconsistencies
  - Location: The diagram shows PROPOSED appearing twice, with an arrow from ACCEPTED back to PROPOSED
  - Impact: The arrow from ACCEPTED to PROPOSED in the diagram (lines 56-58) is confusing - it appears to show a regression path that's not in the transition rules.
  - Suggestion: Clean up diagram. ACCEPTED should not have an arrow to PROPOSED. The arrow from NEEDS_REVISION -> PROPOSED is correct, but layout implies ACCEPTED -> PROPOSED which is invalid.

- **ISSUE-R2-009**: Example 4 (Spawning Sub-Gaps) shows negative net progress as "expected"
  - Location: "net_progress = 0 - 2 = -2 (but this is expected for complex gaps)"
  - Impact: This weakens divergence detection. If negative progress is "expected," when is it NOT expected?
  - Suggestion: Add heuristic: "Negative progress is expected when: (1) First round addressing a complex gap, (2) Gap spawns explicitly related sub-gaps. Negative progress is concerning when: sub-gaps are unrelated, or persists for 2+ rounds."

---

## Review: GAP-FLOW-012 (Severity Level Definitions)

### Critical Issues

None identified. The severity rubric is clear and actionable.

### High Priority

- **ISSUE-R2-010**: Severity weight ratios (8/4/2/1) don't match severity impact descriptions
  - Location: Weighted Convergence Formula and Severity Definitions table
  - Impact: CRITICAL is defined as "Spec cannot be implemented" (total blocker), while HIGH is "Implementation will be incorrect/incomplete" (partial blocker). The 8:4 ratio suggests CRITICAL is only 2x worse than HIGH. In reality, CRITICAL is infinitely worse - no spec vs bad spec.
  - Suggestion: Consider exponential weighting: CRITICAL=16 or 32, HIGH=4, MEDIUM=2, LOW=1. Or: CRITICAL gaps automatically trigger DIVERGENCE state regardless of count. The linear ratio undersells CRITICAL severity.

- **ISSUE-R2-011**: Reviewer-can-only-upgrade rule creates asymmetric information
  - Location: "Reviewer can upgrade, not downgrade"
  - Impact: Engineer may sandbag by marking everything CRITICAL to avoid Reviewer upgrade. Or: Engineer marks LOW, Reviewer upgrades to HIGH, but Engineer was RIGHT. User must arbitrate every severity disagreement.
  - Suggestion: Allow Reviewer to downgrade with EXPLICIT justification: "Reviewer may downgrade severity only by citing specific evidence that Engineer's assessment is incorrect. Downgrades are flagged to user for confirmation."

- **ISSUE-R2-012**: "user-acknowledged" and "user-warned" in termination table lack operational definition
  - Location: Termination Criteria table, values "user-acknowledged" and "user-warned"
  - Impact: What makes a HIGH gap "user-acknowledged"? Saying "I know about GAP-X"? Recording in Known Limitations? There's no verification mechanism.
  - Suggestion: Define operationally:
    - `user-acknowledged` = User answered "Yes" to: "GAP-X (HIGH) will remain unresolved. Document in Known Limitations? [Yes/No]"
    - `user-warned` = User saw warning dialog and clicked "Proceed anyway"

    Add: These acknowledgments are recorded in decisions.md with timestamp.

### Medium Priority

- **ISSUE-R2-013**: Classification rubric uses "common (>10%)" without defining the denominator
  - Location: "Is it HIGH? ... will a common (>10%) use case fail?"
  - Impact: 10% of what? Use cases in the spec? Files in the system? User interactions? Without a denominator, this is subjective.
  - Suggestion: Define: "10% of documented use cases in the spec being refined." If spec has 5 use cases, 1 failing = 20% = HIGH. If spec has 50 use cases, 5 failing = 10% = HIGH threshold.

- **ISSUE-R2-014**: Severity disagreement example doesn't show resolution
  - Location: Example 1 shows conflict flagged but ends with "User can override"
  - Impact: What happens if user DOESN'T override? Does Mediator use higher severity (as stated "Using higher severity (HIGH)")? Is this automatic or requires confirmation?
  - Suggestion: Complete the example:
    ```
    **Resolution:** User did not override. Gap proceeds with HIGH severity (Reviewer's assessment).
    decisions.md entry:
    - GAP-FLOW-001 severity: HIGH (Reviewer, auto-accepted)
    ```

- **ISSUE-R2-015**: Weighted convergence example shows "0" as STALLED but earlier STALL definition is different
  - Location: Example 3 shows "Net: 0 (STALLED)"
  - Impact: GAP-FLOW-002 (from Round 1) defined STALLED as "N rounds of zero net progress" where N=2 by default. A single round of net=0 is not STALLED by that definition, it's just FLAT.
  - Suggestion: Align terminology. Either: (1) Weighted net=0 is "FLAT" not "STALLED", or (2) Update GAP-FLOW-002 to define "weighted STALLED" differently from "unweighted STALLED."

- **ISSUE-R2-016**: No severity for "Open Questions" (OQ-XXX items)
  - Location: Entire proposal addresses gaps but status.md also tracks Open Questions
  - Impact: Open Questions (OQ-001, OQ-002, etc.) have no severity. Are they all LOW? Some OQs can be critical: "Should we use sync or async?" affects entire architecture.
  - Suggestion: Define: "Open Questions inherit severity based on impact. OQs affecting architecture are HIGH. OQs affecting implementation details are MEDIUM. OQs affecting documentation are LOW."

### Low Priority / Nits

- **ISSUE-R2-017**: Example concrete examples reference nonexistent issues
  - Location: "ISSUE-R1-031: Conflict presentation table underspecified" cited as MEDIUM
  - Impact: Minor - this is illustrative. But verifying ISSUE-R1-031 exists in Round 1 reviewer.md would strengthen the example. (Note: Verified it does exist.)
  - Suggestion: Acknowledge: "Examples reference actual issues from Round 1 review."

- **ISSUE-R2-018**: "Cons: May slow process if every severity is debated" lacks mitigation
  - Location: Trade-offs section
  - Impact: This is a real concern. Every gap getting a severity debate is wasteful.
  - Suggestion: Add mitigation: "Default acceptance: If Engineer and Reviewer severities differ by at most one level (e.g., MEDIUM vs HIGH), use higher severity without user arbitration. User arbitration only for 2+ level disagreements."

---

## Cross-Cutting Consistency Checks

### Consistency Between GAP-FLOW-010 and GAP-FLOW-012

- [x] **PASS**: Severity integrates with lifecycle - termination criteria reference gap states
- [x] **PASS**: Weighted convergence uses gap counts from lifecycle
- [ ] **PARTIAL**: GAP-FLOW-010 counting rules use unweighted formula as default, but GAP-FLOW-012 says weighted should be used for "stall/divergence detection, termination decisions." Specify which formula is authoritative for each use case.
- [ ] **PARTIAL**: GAP-FLOW-010 ACCEPTED transition requires "Reviewer no CRITICAL/HIGH issues" - this is a severity reference but GAP-FLOW-012 wasn't defined when GAP-FLOW-010 was written. Verify integration is intentional.

### Consistency with Round 1 Issues

- [x] **ADDRESSES ISSUE-R1-008**: Gap lifecycle now defined with states and transitions
- [x] **ADDRESSES ISSUE-R1-009**: Severity now factored into weighted convergence
- [x] **ADDRESSES ISSUE-R1-025**: Severity definitions (CRITICAL/HIGH/MEDIUM/LOW) now complete
- [ ] **PARTIAL**: ISSUE-R1-028 (implicit disagreement) still needs GAP-FLOW-013, but these foundations enable it

### Unblocking Assessment

- [x] GAP-FLOW-002 (Stall Detection): Can now count gaps using defined lifecycle
- [x] GAP-FLOW-005 (Termination): Can now use severity for "good enough" criteria
- [x] GAP-FLOW-006 (Conflict Resolution): Can now prioritize by severity
- [ ] GAP-FLOW-001 (Error Recovery): Still needs GAP-FLOW-008 (example attachment) - NOT unblocked by this round

---

## Summary

| Gap ID | Verdict | Critical Issues | High Issues | Medium Issues | Low Issues |
|--------|---------|-----------------|-------------|---------------|------------|
| GAP-FLOW-010 | **APPROVE** | 0 | 4 | 3 | 2 |
| GAP-FLOW-012 | **APPROVE** | 0 | 3 | 4 | 2 |

### Verdict: GAP-FLOW-010 - APPROVE

The gap lifecycle definition is fundamentally sound. The 8-state FSM covers all practical scenarios. The HIGH issues (ISSUE-R2-001 through R2-004) are refinements that should be addressed but don't block the definition from being usable. Round 1 gaps are now unblocked.

**Must address before applying:**
- ISSUE-R2-003: Clarify ACCEPTED -> RESOLVED trigger (ambiguity causes confusion)
- ISSUE-R2-004: Strengthen NEEDS_REVISION -> PROPOSED validation (prevents implicit disagreement bypass)

### Verdict: GAP-FLOW-012 - APPROVE

The severity definitions are clear, concrete, and actionable. The classification rubric with yes/no questions is excellent - this is how severity definitions should look. The weighted convergence formula catches false progress. HIGH issues are edge cases and terminology alignment, not fundamental flaws.

**Must address before applying:**
- ISSUE-R2-010: Consider stronger CRITICAL weighting (8:4 ratio may undervalue CRITICAL)
- ISSUE-R2-012: Operationally define "user-acknowledged" and "user-warned"

---

## Recommendation for Round 3

These foundational definitions are approved and unblock significant work. Engineer should:

1. **Apply these definitions** - Update GAP-FLOW-002, 005, 006 proposals to use the now-defined lifecycle and severity
2. **Address HIGH issues above** - Particularly R2-003 (auto-confirm clarity) and R2-012 (acknowledgment definitions)
3. **Proceed to GAP-FLOW-008** (example attachment) - This unblocks GAP-FLOW-001 (error recovery)
4. **Defer GAP-FLOW-013** (implicit disagreement) - Can use ISSUE-R2-004's strengthened transition rules as partial solution

**Net Assessment:** Round 2 is a success. Two foundational definitions are now usable. The circular dependency is broken. Remaining issues are refinements, not blockers.

---

## Revision

| Date | Changes |
|------|---------|
| 2026-01-12 | Initial review of Round 2 foundational proposals |
