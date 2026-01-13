# Engineer Proposals - Round 5

**Date:** 2026-01-12
**Focus:** Partial Round Handling Revision (GAP-FLOW-004)
**Priority:** Tier 1 - Completing Flow Gaps

**Context:** GAP-FLOW-010 (Gap Lifecycle), GAP-FLOW-012 (Severity Definitions), GAP-FLOW-002 (Stall Detection), and GAP-FLOW-005 (Termination Criteria) are now RESOLVED. This proposal revises GAP-FLOW-004 (Partial Round Handling) to address Reviewer issues from Round 1.

---

## Gap Resolution: GAP-FLOW-004 (Revised)

**Gap:** No partial round handling - What if Reviewer finds nothing? What if Engineer has nothing?

**Confidence:** HIGH

### Changes from Round 1

| Round 1 Issue | Resolution in This Revision |
|---------------|----------------------------|
| ISSUE-R1-018 (Reviewer "APPROVE" creates false confidence) | Renamed to NO_ISSUES_FOUND with explicit semantics |
| ISSUE-R1-019 ("Engineer Has Nothing" retry may loop infinitely) | Added bounded retry with BLOCKED_NEEDS_INPUT exit condition |
| ISSUE-R1-020 (Completeness checks are subjective) | Replaced with objective, countable criteria |
| ISSUE-R1-021 ("Process failure: Escalate to user" is vague) | Defined concrete escalation options |
| ISSUE-R1-022 (GAP-ROLE-006 miscategorized) | Recategorized as GAP-QA-002 (verification concern) |

---

### Revised Proposal

#### Scenario 1: Reviewer Finds No Issues

**Renamed:** "APPROVE" -> "NO_ISSUES_FOUND"

**Rationale (Addressing ISSUE-R1-018):** "APPROVE" implies endorsement of correctness, which Reviewer cannot guarantee. "NO_ISSUES_FOUND" accurately describes a negative result - the absence of detected problems, not the presence of verified quality.

**Semantics:**

| Term | Meaning | What It Guarantees | What It Does NOT Guarantee |
|------|---------|-------------------|---------------------------|
| NO_ISSUES_FOUND | Reviewer completed review without finding problems | Review was performed | Proposals are correct |
| | | Reviewer looked at specified areas | All edge cases covered |
| | | No CRITICAL/HIGH/MEDIUM/LOW issues detected | No issues exist |

**Reviewer Output Format (Revised):**

```markdown
## Review: Round N - No Issues Found

### Review Summary
Engineer's proposals for Round N have been reviewed.
No issues identified at any severity level.

### Review Scope
| Gap ID | Sections Reviewed | Lines Examined |
|--------|-------------------|----------------|
| GAP-FLOW-001 | Proposed Solution, Examples, Trade-offs | 45-120 |
| GAP-FLOW-002 | State Machine, Transition Rules | 95-180 |

### Objective Completeness Criteria (Checklist)

#### Structural Criteria
- [x] Each gap resolution contains: Solution, Examples, Trade-offs sections
- [x] Each gap has explicit Confidence level (HIGH/MEDIUM/LOW)
- [x] All referenced gap IDs exist in status.md
- [x] No orphaned cross-references

#### Example Criteria
- [x] At least 1 example per gap resolution
- [x] At least 1 example shows success path
- [x] At least 1 example shows error/edge case (if applicable)
- [x] Examples use concrete values, not placeholders

#### Consistency Criteria
- [x] No contradictions with previously RESOLVED gaps
- [x] No duplicate definitions for same concept
- [x] Terminology matches glossary/existing definitions

### Verdict
**NO_ISSUES_FOUND** - Review complete. No issues detected.

Note: This is a negative result (absence of detected issues), not a guarantee of correctness.
```

**Key Distinction from Round 1:**
- NO_ISSUES_FOUND is NOT endorsement
- Reviewer explicitly states what was examined
- Checklist items are objective (count-based, existence-based)
- Disclaimer clarifies limitations

---

#### Scenario 2: Engineer Has Nothing to Propose

**Problem Statement:** Engineer may produce empty or near-empty output when:
1. All remaining gaps are genuinely blocked on external input
2. Remaining gaps are outside Engineer's domain expertise
3. Prompt is unclear about which gaps to address
4. Engineer misunderstands the task

**Revised Handling (Addressing ISSUE-R1-019):**

**Bounded Retry Protocol:**

```
Step 1: Detect Empty Output
─────────────────────────────────────────────────────────
Validation detects: Engineer output addresses 0 gaps
(per GAP-FLOW-001 validation rules)

Step 2: Classify Cause (Mediator Analysis)
─────────────────────────────────────────────────────────
Mediator checks engineer.md for:

IF contains "BLOCKED_NEEDS_INPUT: [gap_id]" statements:
  -> Cause: EXPLICIT_BLOCK
  -> Action: Proceed to Step 3a

ELIF contains "UNCERTAINTY: [description]" statements:
  -> Cause: DOMAIN_GAP
  -> Action: Proceed to Step 3b

ELIF engineer.md is empty or malformed:
  -> Cause: OUTPUT_FAILURE
  -> Action: Retry per GAP-FLOW-001 (max 2 retries)

ELSE:
  -> Cause: UNCLEAR_ASSIGNMENT
  -> Action: Proceed to Step 3c

Step 3a: Explicit Block Handling
─────────────────────────────────────────────────────────
Engineer explicitly stated: "BLOCKED_NEEDS_INPUT: GAP-XXX"

Mediator:
1. Validates gap exists in status.md
2. Transitions gap state: OPEN -> BLOCKED_EXTERNAL (new state, subset of USER_DEFERRED)
3. Records blocking reason in status.md
4. Presents to user via AskUserQuestion:

"Engineer reports GAP-XXX is blocked on external input.

Gap: GAP-XXX - [description]
Engineer's reason: [quoted from engineer.md]

Options:
1. Provide input now - [text field]
2. Defer gap - Mark as USER_DEFERRED
3. Assign different gap - Reassign Engineer to [next priority gap]
4. End round - Proceed without Engineer proposals"

Step 3b: Domain Gap Handling
─────────────────────────────────────────────────────────
Engineer explicitly stated: "UNCERTAINTY: [description]"

Mediator:
1. Records uncertainty in status.md
2. Retry once with enhanced prompt including:
   - Relevant excerpts from source.md
   - Similar resolved gaps as examples
   - Explicit: "If still uncertain, write partial solution with CONFIDENCE: LOW"

IF second attempt still shows UNCERTAINTY:
  -> Proceed to Step 4 (Escalation)

Step 3c: Unclear Assignment Handling
─────────────────────────────────────────────────────────
Engineer produced output but didn't address expected gaps.

Mediator:
1. Retry once with explicit gap assignment:
   "You must address exactly these gaps in priority order:
    1. GAP-XXX: [full description from status.md]
    2. GAP-YYY: [full description from status.md]

    Write output to round_{N}/engineer.md using standard format."

IF second attempt still misses gaps:
  -> Proceed to Step 4 (Escalation)

Step 4: Escalation to User (Addressing ISSUE-R1-021)
─────────────────────────────────────────────────────────
After max retries exhausted (2 retries per GAP-FLOW-001):

AskUserQuestion:
"Engineer could not produce proposals after 2 attempts.

=== Situation ===
Round: {N}
Assigned gaps: {list}
Attempts: 2 (both unsuccessful)
Cause: {EXPLICIT_BLOCK | DOMAIN_GAP | UNCLEAR_ASSIGNMENT}

=== Failure Details ===
Attempt 1: [brief description of output/failure]
Attempt 2: [brief description of output/failure]

=== Options ===
1. Skip round - Proceed to Reviewer with empty Engineer output
   Effect: Round {N} recorded as "Engineer: SKIP"
   Reviewer will review prior round's proposals

2. Reassign gaps - Choose different gaps for this round
   Effect: Select from non-blocked gaps in status.md
   New prompt generated with selected gaps

3. Provide context - Add information Engineer may be missing
   Effect: Text input added to Engineer prompt
   One additional retry with context

4. Narrow scope - Address only the simplest gap
   Effect: Reassign to single lowest-complexity gap
   Reduces cognitive load on Engineer

5. Pause session - Save state for later
   Effect: Session saved, can resume with fresh context

Choose an option (1-5): ___"
```

**Exit Conditions (Bounded, Not Infinite):**

| Condition | Max Attempts | Exit Action |
|-----------|--------------|-------------|
| Output failure (empty/malformed) | 2 | User escalation |
| Explicit block (BLOCKED_NEEDS_INPUT) | 0 (immediate) | User provides input or defers |
| Domain gap (UNCERTAINTY) | 2 | User escalation |
| Unclear assignment | 2 | User escalation |

**Total maximum iterations before escalation: 2**

---

#### Scenario 3: Both Engineer and Reviewer Empty

**Decision Table (Revised from Round 1, Addressing ISSUE-R1-021):**

| Condition | Interpretation | Concrete Action |
|-----------|----------------|-----------------|
| Zero open gaps | Workflow complete | Trigger COMPLETE termination (per GAP-FLOW-005) |
| All open gaps BLOCKED_EXTERNAL | Waiting on user | Present ALL blocked gaps to user for input/deferral |
| Gaps exist, Engineer blocked, Reviewer has nothing | Stalled on input | Same as all blocked (present to user) |
| Gaps exist, neither blocked nor addressed | Process failure | Escalation per Step 4 above |

**Process Failure Escalation (Concrete Options):**

When gaps exist and are not blocked, but neither Engineer nor Reviewer produces output:

```
AskUserQuestion:
"Neither Engineer nor Reviewer produced output this round.

=== Situation ===
Round: {N}
Open gaps: {count}
Blocked gaps: 0
Engineer output: Empty/Failed
Reviewer output: Empty/Failed (nothing to review)

This may indicate:
- Remaining gaps are genuinely out of scope for this workflow
- Session has diverged from spec refinement objectives
- Systemic issue with prompts or context

=== Options ===
1. Retry round with fresh context
   Effect: Re-initialize round with condensed prior context
   Risk: May fail again

2. Manually mark gaps as WONT_FIX
   Effect: User selects gaps to close as out-of-scope
   Enables termination

3. Manually mark gaps as USER_DEFERRED
   Effect: User selects gaps to defer to future session
   Enables progress toward termination

4. Abandon session
   Effect: Session terminated, no output generated
   Prior rounds archived

5. Request human review
   Effect: Output current state for human Engineer review
   Human can provide proposals directly

Choose an option (1-5): ___"
```

---

#### Objective Completeness Criteria (Addressing ISSUE-R1-020)

**Problem:** Round 1 criteria like "Examples are sufficient for implementation" are subjective.

**Solution:** Replace with countable, verifiable criteria.

**Objective Completeness Checklist:**

| Criterion | Metric | Threshold | Verification Method |
|-----------|--------|-----------|---------------------|
| Examples exist | count(examples per gap) | >= 1 | Regex: `### Example` or `**Example` |
| Happy path covered | count(success examples) | >= 1 per gap | Manual tag or inference |
| Edge case covered | count(edge/error examples) | >= 1 if gap mentions errors | Manual tag or inference |
| Confidence stated | presence of `**Confidence:**` | Required | Regex match |
| Trade-offs stated | presence of `### Trade-offs` | Required | Regex match |
| New gaps declared | presence of `### New Gaps Introduced` | Required | Regex match |
| Cross-refs valid | all `GAP-XXX` references | Must exist in status.md | Automated validation |
| Min content length | character count | >= 200 per gap | Automated count |

**Automated vs Manual Checks:**

| Check Type | Automation Level | Validator |
|------------|------------------|-----------|
| Structural (headers, sections) | Automated | Mediator regex |
| Reference validity | Automated | Mediator lookup |
| Content length | Automated | Mediator count |
| Example type classification | Semi-automated | Keywords + Mediator judgment |
| Consistency with prior gaps | Manual | Reviewer review |
| Correctness of solution | Manual | Reviewer review (NO_ISSUES_FOUND) |

**Reviewer Checklist Format (Objective Version):**

```markdown
### Completeness Check (Objective Criteria)

#### Automated Checks (pass/fail)
| Check | GAP-FLOW-001 | GAP-FLOW-002 | GAP-FLOW-003 |
|-------|--------------|--------------|--------------|
| Has examples | PASS | PASS | PASS |
| Has confidence | PASS | PASS | PASS |
| Has trade-offs | PASS | PASS | PASS |
| Has new gaps | PASS | PASS | PASS |
| Valid cross-refs | PASS | PASS | FAIL |
| Min length (200) | PASS | PASS | PASS |

#### Manual Checks (reviewer judgment)
| Check | GAP-FLOW-001 | GAP-FLOW-002 | GAP-FLOW-003 |
|-------|--------------|--------------|--------------|
| Success example present | YES | YES | YES |
| Edge case example present | YES | N/A | YES |
| Consistent with prior resolutions | YES | YES | ISSUE (see below) |
| No obvious gaps in logic | YES | YES | YES |

#### Issues Found
- GAP-FLOW-003: Invalid cross-reference to GAP-FLOW-099 (does not exist)
- GAP-FLOW-003: Inconsistent with GAP-FLOW-002 state machine (see ISSUE-R5-001)
```

---

#### BLOCKED_EXTERNAL State Addition

**Addition to GAP-FLOW-010 (Gap Lifecycle):**

| State | Definition | Counted as Open? |
|-------|------------|------------------|
| **BLOCKED_EXTERNAL** | Gap requires external input to proceed | No (0.25 weight) |

This is a sub-state of USER_DEFERRED but with different semantics:
- USER_DEFERRED: User chose to defer (intentional)
- BLOCKED_EXTERNAL: Engineer cannot proceed without input (blocking)

**State Transition:**
```
OPEN -> BLOCKED_EXTERNAL (when Engineer reports BLOCKED_NEEDS_INPUT)
BLOCKED_EXTERNAL -> OPEN (when user provides input)
BLOCKED_EXTERNAL -> USER_DEFERRED (when user explicitly defers)
```

**Counting Impact:**
```
# Updated from GAP-FLOW-010
open_gap_count = count(OPEN) + count(IN_PROGRESS) + count(PROPOSED)
                 + count(NEEDS_REVISION) + (0.5 * count(USER_DEFERRED))
                 + (0.25 * count(BLOCKED_EXTERNAL))
```

**Rationale for 0.25 weight:** BLOCKED_EXTERNAL gaps are "pending user action" but not "deferred by choice." Lower weight prevents false stall detection when waiting on legitimate external input.

---

#### Recording in status.md

**Round Status When Partial:**

```markdown
## Round N Summary

### Completion Status
| Role | Status | Details |
|------|--------|---------|
| Engineer | PARTIAL | 2 of 4 assigned gaps addressed |
| Reviewer | COMPLETE | All proposals reviewed |

### Engineer Partial Report
| Gap ID | Status | Reason |
|--------|--------|--------|
| GAP-FLOW-001 | PROPOSED | Addressed |
| GAP-FLOW-002 | PROPOSED | Addressed |
| GAP-COMM-001 | BLOCKED_EXTERNAL | "Requires API documentation for external system" |
| GAP-AUTO-002 | SKIPPED | Reassigned by user to Round N+1 |

### Gap State Changes
| Gap ID | Previous State | New State | Actor |
|--------|----------------|-----------|-------|
| GAP-FLOW-001 | OPEN | PROPOSED | Engineer |
| GAP-FLOW-002 | OPEN | PROPOSED | Engineer |
| GAP-COMM-001 | OPEN | BLOCKED_EXTERNAL | Engineer |
| GAP-AUTO-002 | OPEN | OPEN (reassigned) | User |
```

---

### Examples

**Example 1: Reviewer NO_ISSUES_FOUND**
```markdown
## Review: Round 5 - No Issues Found

### Review Summary
Engineer's proposals for Round 5 have been reviewed.
No issues identified at any severity level.

### Review Scope
| Gap ID | Sections Reviewed | Lines Examined |
|--------|-------------------|----------------|
| GAP-FLOW-004 | Solution, Examples, Trade-offs | 35-290 |

### Objective Completeness Criteria (Checklist)

#### Structural Criteria
- [x] Each gap resolution contains: Solution, Examples, Trade-offs sections
- [x] Each gap has explicit Confidence level (HIGH/MEDIUM/LOW)
- [x] All referenced gap IDs exist in status.md
- [x] No orphaned cross-references

#### Example Criteria
- [x] At least 1 example per gap resolution (found: 3)
- [x] At least 1 example shows success path
- [x] At least 1 example shows error/edge case

#### Consistency Criteria
- [x] No contradictions with previously RESOLVED gaps
- [x] No duplicate definitions for same concept
- [x] Terminology matches existing definitions

### Verdict
**NO_ISSUES_FOUND** - Review complete. No issues detected.

Note: This is a negative result (absence of detected issues), not a guarantee of correctness.
```

**Example 2: Engineer Blocked on External Input**
```markdown
## Gap Resolution: GAP-COMM-001

**BLOCKED_NEEDS_INPUT: GAP-COMM-001**

### Blocking Reason
GAP-COMM-001 (Document versioning) requires knowledge of:
1. Whether Claude Code Task tool preserves file timestamps
2. Whether concurrent file access is possible in this environment
3. Expected network latency characteristics

Without this information, I cannot propose a concrete versioning strategy.

### Partial Analysis
Based on available information:
- If single-writer (Mediator only writes status.md): No versioning needed
- If multi-writer possible: Need CAS or optimistic locking
- Unknown: Task tool file semantics

### Request
Please provide:
1. Task tool file handling semantics
2. Or: Confirm single-writer assumption is safe
```

**Example 3: Escalation After Failed Retries**
```
AskUserQuestion:
"Engineer could not produce proposals after 2 attempts.

=== Situation ===
Round: 6
Assigned gaps: GAP-AUTO-001, GAP-AUTO-002
Attempts: 2 (both unsuccessful)
Cause: DOMAIN_GAP

=== Failure Details ===
Attempt 1: Engineer output "UNCERTAINTY: CI/CD integration requires
           knowledge of specific CI system (GitHub Actions, Jenkins, etc.)"
Attempt 2: Engineer output "CONFIDENCE: LOW - Generic proposal without
           specific CI system may be too vague to implement"

=== Options ===
1. Skip round - Proceed to Reviewer with empty Engineer output
2. Reassign gaps - Choose different gaps for this round
3. Provide context - Add information Engineer may be missing
4. Narrow scope - Address only the simplest gap
5. Pause session - Save state for later

Choose an option (1-5): ___"

User selects: 3
User provides: "Target CI system is GitHub Actions. Use workflow files."

[Mediator retries with context]

Engineer successfully proposes GAP-AUTO-001, GAP-AUTO-002 with GitHub Actions specifics.
```

**Example 4: Process Failure with Both Empty**
```
Round 8:
- Open gaps: 3 (GAP-QA-003, GAP-QA-004, GAP-UX-004)
- Engineer output: Empty after 2 retries
- Reviewer output: Empty (nothing to review)

AskUserQuestion:
"Neither Engineer nor Reviewer produced output this round.

=== Situation ===
Round: 8
Open gaps: 3
Blocked gaps: 0
Engineer output: Empty/Failed
Reviewer output: Empty/Failed (nothing to review)

=== Options ===
1. Retry round with fresh context
2. Manually mark gaps as WONT_FIX
3. Manually mark gaps as USER_DEFERRED
4. Abandon session
5. Request human review

Choose an option (1-5): ___"

User selects: 2
User marks: GAP-QA-003, GAP-QA-004 as WONT_FIX ("Out of scope for v1")
User marks: GAP-UX-004 as USER_DEFERRED ("Will address in v1.1")

Result: Zero open gaps. Termination check triggered.
```

---

### Trade-offs

**Pros:**
- NO_ISSUES_FOUND accurately represents what Reviewer can guarantee
- Bounded retry prevents infinite loops (max 2 retries per GAP-FLOW-001)
- Explicit BLOCKED_EXTERNAL state distinguishes "can't proceed" from "chose to defer"
- Objective criteria enable automated validation
- Concrete escalation options give user clear choices
- BLOCKED_EXTERNAL weight (0.25) prevents false stall on legitimate blocks

**Cons:**
- Objective criteria may miss subtle issues (but subjective criteria had same risk)
- BLOCKED_EXTERNAL adds complexity to state machine (minimal - 3 transitions)
- Escalation flow is verbose (intentional - ensures user understands situation)
- 0.25 weight for BLOCKED_EXTERNAL is somewhat arbitrary (but tunable)
- Some criteria still require Reviewer judgment (manual checks)

---

### Alignment with Foundations

| Foundation | Integration |
|------------|-------------|
| GAP-FLOW-010 (Lifecycle) | BLOCKED_EXTERNAL added as sub-state with 0.25 weight |
| GAP-FLOW-010 (Lifecycle) | State transitions from OPEN -> BLOCKED_EXTERNAL defined |
| GAP-FLOW-001 (Error Recovery) | Uses max 2 retries for consistency |
| GAP-FLOW-005 (Termination) | Process failure escalation can lead to WONT_FIX or USER_DEFERRED |
| GAP-FLOW-002 (Stall Detection) | BLOCKED_EXTERNAL weight affects convergence calculation |

---

### Response to Reviewer Issues

| Issue | Resolution |
|-------|------------|
| ISSUE-R1-018 | "APPROVE" renamed to "NO_ISSUES_FOUND" with explicit semantics table. Disclaimer clarifies this is negative result, not endorsement. |
| ISSUE-R1-019 | Bounded retry: max 2 attempts. Exit conditions defined: EXPLICIT_BLOCK (immediate), DOMAIN_GAP (2 tries), UNCLEAR_ASSIGNMENT (2 tries), OUTPUT_FAILURE (2 tries per FLOW-001). Escalation to user after limits. |
| ISSUE-R1-020 | Subjective criteria replaced with objective metrics: count(examples) >= 1, presence of required sections via regex, cross-ref validation against status.md. Manual checks remain for consistency/correctness but are clearly labeled. |
| ISSUE-R1-021 | "Escalate to user" now has 5 concrete options: Skip round, Reassign gaps, Provide context, Narrow scope, Pause session. Each option has defined effect. |
| ISSUE-R1-022 | GAP-ROLE-006 recategorized. Original was about requiring Reviewer to list checks - this is verification/QA concern, so renamed to GAP-QA-002 ("Reviewer verification requirements"). |

---

### Recategorization: GAP-ROLE-006 -> GAP-QA-002

**Original:** GAP-ROLE-006 - "Should we require Reviewer to list specific checks performed?"

**Issue:** This gap is about verification and quality assurance, not role definition.

**Recategorization:**
- New ID: GAP-QA-002
- Category: Quality Assurance
- Description: "Should Reviewer be required to list specific checks performed?"
- Status: PARTIALLY_ADDRESSED (this proposal includes objective checklist)
- Remaining: Whether checklist should be mandatory vs optional

**Note:** This proposal provides a checklist format. GAP-QA-002 remains open for decision on whether it should be mandatory.

---

### New Gaps Introduced

- **GAP-FLOW-016**: BLOCKED_EXTERNAL weight (0.25) may need calibration based on real usage patterns

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-004 (Revised) | Proposed | HIGH | 1 (GAP-FLOW-016) |

**Changes from Round 1:**
- Renamed APPROVE to NO_ISSUES_FOUND with explicit semantics
- Defined bounded retry protocol (max 2 attempts per cause)
- Replaced subjective criteria with objective, countable metrics
- Defined concrete escalation options (5 choices with effects)
- Added BLOCKED_EXTERNAL state to lifecycle
- Recategorized GAP-ROLE-006 as GAP-QA-002

**Dependencies:**
- Extends GAP-FLOW-010 (adds BLOCKED_EXTERNAL state)
- Consistent with GAP-FLOW-001 (retry limits)
- Feeds into GAP-FLOW-005 (escalation can trigger WONT_FIX)
- Affects GAP-FLOW-002 (BLOCKED_EXTERNAL weight in convergence)

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### Scenario Handling Summary
```
Reviewer finds nothing  -> NO_ISSUES_FOUND (not APPROVE)
Engineer blocked        -> BLOCKED_NEEDS_INPUT -> BLOCKED_EXTERNAL state
Engineer unclear        -> Max 2 retries -> Escalation
Both empty              -> Escalation with 5 options
```

### Objective Criteria Quick Reference
```
Required per gap:
- >= 1 example
- Confidence statement
- Trade-offs section
- New Gaps section
- >= 200 characters
- Valid cross-references

Manual checks (Reviewer):
- Consistency with prior gaps
- Success/edge case coverage
- Logical completeness
```

### BLOCKED_EXTERNAL State
```
Transition: OPEN -> BLOCKED_EXTERNAL (Engineer reports)
Weight: 0.25 in open gap count
Exit: User provides input -> OPEN, or User defers -> USER_DEFERRED
```

### Escalation Options
```
1. Skip round
2. Reassign gaps
3. Provide context (1 more retry)
4. Narrow scope
5. Pause session
```
