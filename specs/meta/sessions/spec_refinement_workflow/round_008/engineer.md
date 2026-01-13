# Engineer Proposals - Round 8

**Date:** 2026-01-12
**Focus:** Implicit Disagreement Detection (GAP-FLOW-013)
**Priority:** Tier 1 - Unblocking GAP-FLOW-006

**Context:** GAP-FLOW-006 (Conflict Resolution) was proposed in Round 1 but remains blocked on GAP-FLOW-013. The Round 1 proposal included explicit DISAGREE handling but, as the Reviewer noted in ISSUE-R1-028: "Explicit DISAGREE is rare. Most conflicts are implicit: Engineer ignores feedback, rephrases slightly, or addresses a different interpretation."

**Dependencies Resolved:**
- GAP-FLOW-010 (Gap Lifecycle): 8 states defined
- GAP-FLOW-012 (Severity): CRITICAL/HIGH/MEDIUM/LOW with weights
- GAP-FLOW-008 (Example Attachment): Retry mechanism defined
- GAP-FLOW-007 (Rollback): Pattern detection heuristics available

---

## Gap Resolution: GAP-FLOW-013

**Gap:** Implicit disagreement detection - How to detect when Engineer ignores or bypasses Reviewer feedback without explicit DISAGREE?

**Confidence:** HIGH

### Problem Statement

From Round 1 Reviewer feedback (ISSUE-R1-028):

> "Implicit disagreement detection is hand-waved. Explicit DISAGREE is rare. Most conflicts are implicit: Engineer ignores feedback, rephrases slightly, or addresses a different interpretation. Without detection, conflicts fester."

Implicit disagreement is more common and more insidious than explicit disagreement:

| Type | Frequency | Detection | Risk |
|------|-----------|-----------|------|
| Explicit DISAGREE | Rare (~5%) | Trivial (keyword match) | Low (forces resolution) |
| Implicit ignore | Common (~60%) | Hard (requires tracking) | High (festering conflict) |
| Implicit rephrase | Common (~35%) | Medium (semantic analysis) | Medium (talking past each other) |

---

### Proposed Solution

Define a **multi-signal detection system** that identifies implicit disagreement through structural, reference, and semantic analysis. When detected, the system flags potential disagreement for user review rather than auto-escalating.

---

#### 1. Definition: What Constitutes Implicit Disagreement

**Implicit disagreement occurs when:**

1. **Issue Ignored:** Reviewer raised HIGH/CRITICAL issue in round N, and Engineer's round N+1 output contains no explicit reference to it
2. **Issue Rephrased Away:** Engineer claims to address issue but fundamentally changes the concern
3. **Interpretation Mismatch:** Engineer addresses a different interpretation than Reviewer intended
4. **Partial Address:** Engineer addresses only the low-severity parts of a multi-part issue

**NOT implicit disagreement:**

- Engineer addresses issue with different approach than Reviewer suggested (valid disagreement on solution, not on problem)
- Issue was LOW/MEDIUM severity and got deprioritized (acceptable triage)
- Issue was addressed in substance but without explicit ID reference (detection miss, not disagreement)

---

#### 2. Detection Heuristics

##### Heuristic 1: Unaddressed HIGH/CRITICAL Issues

**Signal:** Issue tracking gap

**Rule:**
```
FOR each issue I in round N where I.severity IN (HIGH, CRITICAL):
  IF round N+1 engineer.md does not contain:
    - "Response to ISSUE-{I.id}" OR
    - "Addressing ISSUE-{I.id}" OR
    - "DISAGREE: ISSUE-{I.id}"
  AND gap G referenced by I is still open (not RESOLVED, WONT_FIX)
  THEN flag as POTENTIAL_IMPLICIT_DISAGREE(I)
```

**Detection Confidence:** HIGH (structural, objective)

**False Positive Risk:** MEDIUM
- Engineer may have addressed substance without using ID
- Mitigation: Semantic cross-check (Heuristic 4)

##### Heuristic 2: Repeated Rollback on Same Gap

**Signal:** Pattern from GAP-FLOW-007 rollback analysis

**Rule:**
```
IF gap G has been:
  - Rolled back >= 2 times on same round
  - With different failure patterns each time
AND Reviewer raised issues on G in round N
AND those issues remain unaddressed
THEN flag as POTENTIAL_IMPLICIT_DISAGREE(G.issues)
```

**Detection Confidence:** MEDIUM (behavioral, inferential)

**Rationale:** Repeated failures with pattern changes suggest Engineer is avoiding rather than addressing core feedback.

##### Heuristic 3: Severity Downgrade Without Justification

**Signal:** Implicit minimization

**Rule:**
```
IF Reviewer marked issue I as HIGH/CRITICAL in round N
AND Engineer's round N+1:
  - Proposes same gap as RESOLVED
  - Without addressing I
  - OR addresses with solution that ignores I's core concern
THEN flag as POTENTIAL_IMPLICIT_DISAGREE(I)
```

**Detection Confidence:** MEDIUM (requires semantic judgment)

**Implementation:** Mediator compares Reviewer's stated concern with Engineer's solution scope.

##### Heuristic 4: Semantic Non-Response Check

**Signal:** Content analysis showing no substantive engagement

**Rule:**
```
FOR each HIGH/CRITICAL issue I:
  1. Extract I.core_concern (the "Impact" field from Reviewer)
  2. Extract I.location (the specific text/section criticized)
  3. In Engineer's round N+1:
     - Check if I.location was modified
     - Check if modification addresses I.core_concern
  4. IF location unchanged OR modification orthogonal to concern
     THEN flag as POTENTIAL_IMPLICIT_DISAGREE(I)
```

**Detection Confidence:** MEDIUM (depends on semantic analysis quality)

**Implementation:**

```python
def check_semantic_response(issue, engineer_output, previous_proposal):
    """
    Determine if Engineer substantively engaged with issue.
    """
    # Extract the section Reviewer criticized
    location_text = extract_section(previous_proposal, issue.location)

    # Find corresponding section in new output
    new_text = extract_section(engineer_output, issue.location)

    if new_text is None:
        # Section removed entirely - could be address or avoidance
        return SemanticCheck(
            addressed=UNKNOWN,
            reason="Section removed - manual review needed"
        )

    if new_text == location_text:
        # No change at all
        return SemanticCheck(
            addressed=NO,
            reason="Criticized section unchanged"
        )

    # Text changed - check if change relates to concern
    concern_keywords = extract_keywords(issue.impact)
    change_keywords = extract_keywords(diff(location_text, new_text))

    overlap = len(concern_keywords & change_keywords) / len(concern_keywords)

    if overlap < 0.3:
        return SemanticCheck(
            addressed=PARTIAL,
            reason=f"Changes don't overlap with concern keywords ({overlap:.0%})"
        )

    return SemanticCheck(
        addressed=YES,
        reason=f"Changes address concern keywords ({overlap:.0%})"
    )
```

##### Heuristic 5: Response That Attacks Reviewer Rather Than Issue

**Signal:** Deflection pattern

**Rule:**
```
IF Engineer's response to ISSUE-{I.id} contains:
  - "Reviewer misunderstood..."
  - "This is not a real issue..."
  - "Reviewer's suggestion would break..."
WITHOUT providing alternative solution
THEN flag as POTENTIAL_IMPLICIT_DISAGREE(I) with DEFLECTION marker
```

**Detection Confidence:** HIGH (keyword pattern)

**Distinction:** Disagreeing with Reviewer's SOLUTION is fine. Disagreeing with the PROBLEM without providing alternative framing is implicit disagreement.

---

#### 3. Distinguishing "Forgot" from "Intentionally Ignored"

Critical design question: How do we tell accidental omission from intentional bypass?

**Signals for "Forgot":**
- Issue was LOW/MEDIUM severity
- Engineer addressed related issues from same round
- Engineer output shows time/space constraints (shorter than usual)
- No pattern of ignoring similar issues previously

**Signals for "Intentionally Ignored":**
- Issue was HIGH/CRITICAL severity
- Engineer addressed other issues from same Reviewer section but skipped this one
- Engineer output is normal length but avoids the topic
- Engineer has history of ignoring similar feedback (pattern detection)

**Resolution:** Don't try to determine intent. Present to user as:

```
POTENTIAL IMPLICIT DISAGREEMENT DETECTED

Issue: ISSUE-R5-003 (HIGH)
"Rollback does not restore decisions.md"

Evidence:
- Round 6 Engineer output contains no reference to ISSUE-R5-003
- Engineer addressed 4 other issues from Round 5 Reviewer
- Issue severity is HIGH
- Gap GAP-FLOW-007 is still PROPOSED (not resolved)

This could be:
1. Oversight - Engineer missed this issue
2. Implicit disagreement - Engineer chose not to address

Options:
1. Ask Engineer to explicitly address this issue
2. Ask Engineer if they DISAGREE (with rationale)
3. Let user decide if issue should be waived
4. Ignore this flag (false positive)
```

---

#### 4. Escalation Thresholds

**When to warn vs when to escalate:**

| Signal Strength | Severity | Action |
|-----------------|----------|--------|
| Single unaddressed issue | HIGH | Log + include in round summary |
| Single unaddressed issue | CRITICAL | Warn user before proceeding |
| Multiple unaddressed issues (2+) | HIGH | Warn user, recommend intervention |
| Multiple unaddressed issues (2+) | CRITICAL | Block round completion, require user decision |
| Pattern across rounds (same issue type ignored 3+ times) | Any | Escalate to user as systematic issue |
| Deflection detected | Any | Flag for user review |

**Threshold Configuration:**

```markdown
## Implicit Disagreement Settings

| Setting | Default | Description |
|---------|---------|-------------|
| unaddressed_high_threshold | 2 | Warn after N unaddressed HIGH issues |
| unaddressed_critical_threshold | 1 | Warn after N unaddressed CRITICAL issues |
| pattern_threshold | 3 | Flag as systematic after N occurrences |
| semantic_overlap_minimum | 0.3 | Keyword overlap to count as "addressed" |
| enable_semantic_check | true | Use semantic analysis (slower, more accurate) |
```

---

#### 5. Presentation to User

**In Round Summary (status.md):**

```markdown
## Round 6 Summary

### Implicit Disagreement Flags

| Issue | Severity | Detection | Confidence |
|-------|----------|-----------|------------|
| ISSUE-R5-003 | HIGH | Unaddressed (no ID reference) | HIGH |
| ISSUE-R5-007 | CRITICAL | Semantic non-response | MEDIUM |

### Details

#### ISSUE-R5-003 (HIGH)
**Original concern:** "Rollback does not restore decisions.md"
**Reviewer location:** GAP-FLOW-007, Section "Restore Protocol"
**Engineer response:** None found
**Section status:** Unchanged from Round 5

#### ISSUE-R5-007 (CRITICAL)
**Original concern:** "Backup files create inconsistency - only N-1 kept"
**Reviewer location:** GAP-FLOW-007, Section "Backup Retention"
**Engineer response:** Added paragraph about backup rotation
**Analysis:** Changes address "rotation" but not "inconsistency risk"
**Keyword overlap:** 22% (below 30% threshold)

### Recommended Actions

1. **For ISSUE-R5-003:** Request explicit response from Engineer
2. **For ISSUE-R5-007:** Clarify if Engineer's changes intended to address this
```

**Interactive Prompt (when blocking):**

```
IMPLICIT DISAGREEMENT REQUIRES RESOLUTION

Round 6 cannot complete until the following CRITICAL issue is addressed:

ISSUE-R5-007: Backup files create inconsistency
Severity: CRITICAL
Original: "Only keeping N-1 backup makes multi-round rollback impossible"

Engineer's Round 6 output appears to not address this concern.

Options:
1. Request Engineer revision - Send back with explicit instruction to address
2. Convert to explicit DISAGREE - Create formal disagreement for conflict resolution
3. Waive issue - Downgrade to HIGH and accept current output
4. User provides clarification - You explain what Engineer should do
5. Mark as false positive - Detection was wrong, issue WAS addressed
```

---

#### 6. Actions Available

When implicit disagreement is detected, Mediator can:

**Action 1: Request Explicit Address**

```python
def request_explicit_address(issue_id, engineer_context):
    """
    Send clarification request to Engineer.
    """
    return Task(f"""
    CLARIFICATION REQUIRED

    The Reviewer raised {issue_id} as a {issue.severity} issue in Round {issue.round}.
    Your Round {current_round} output does not appear to address it.

    Original issue:
    {issue.full_text}

    Please either:
    1. Add a section "## Response to {issue_id}" with your proposed solution
    2. Add "DISAGREE: {issue_id}" with rationale if you believe this is not a valid concern
    3. Explain why your existing content already addresses this (quote the relevant section)

    This is required before the round can proceed.
    """)
```

**Action 2: Convert to Explicit Disagreement**

```python
def convert_to_explicit_disagree(issue_id):
    """
    Create formal disagreement entry for conflict resolution (GAP-FLOW-006).
    """
    conflict = Conflict(
        issue_id=issue_id,
        engineer_position="Implicit - did not address",
        reviewer_position=issue.suggestion,
        detection="Implicit disagreement converted to explicit",
        timestamp=now()
    )

    # Feed into GAP-FLOW-006 conflict resolution
    return add_to_conflict_queue(conflict)
```

**Action 3: Auto-Retry with Specific Guidance**

```python
def auto_retry_for_implicit_disagree(round_n, issues):
    """
    Retry round with explicit instructions about missing issues.
    """
    issue_list = "\n".join([
        f"- {i.id} ({i.severity}): {i.summary}"
        for i in issues
    ])

    adjustment = f"""
    REQUIRED: Address the following issues from previous Reviewer feedback:

    {issue_list}

    For EACH issue above, your output MUST contain ONE of:
    1. "## Response to [ISSUE-ID]" with your solution
    2. "DISAGREE: [ISSUE-ID]" with your rationale

    Do not proceed without explicitly addressing each issue.
    """

    return retry_round(round_n, adjustments=[adjustment])
```

**Action 4: User Waive**

```python
def user_waive_issue(issue_id, rationale):
    """
    User explicitly accepts that issue will not be addressed.
    """
    decisions.append(Decision(
        round=current_round,
        type="ISSUE_WAIVED",
        issue_id=issue_id,
        rationale=rationale,
        timestamp=now()
    ))

    # Remove from implicit disagreement tracking
    remove_from_implicit_disagree_queue(issue_id)

    # Log in status.md
    log_waived_issue(issue_id, rationale)
```

**Action 5: Mark False Positive**

```python
def mark_false_positive(issue_id, explanation):
    """
    User indicates detection was incorrect - issue WAS addressed.
    """
    detection_log.append(FalsePositive(
        issue_id=issue_id,
        detection_method=issue.detection_method,
        explanation=explanation,
        timestamp=now()
    ))

    # Use for tuning detection heuristics
    update_detection_weights(issue.detection_method, penalty=-0.1)
```

---

#### 7. Feedback Loop for Detection Accuracy

Track detection accuracy to tune heuristics:

```markdown
## Implicit Disagreement Detection Metrics

### Session: spec_refinement_workflow

| Round | Flags Raised | True Positives | False Positives | Missed |
|-------|--------------|----------------|-----------------|--------|
| 6 | 2 | 1 | 1 | 0 |
| 7 | 1 | 1 | 0 | 1 |
| Total | 3 | 2 | 1 | 1 |

**Precision:** 67% (2/3)
**Recall:** 67% (2/3)

### Heuristic Performance

| Heuristic | True Pos | False Pos | Notes |
|-----------|----------|-----------|-------|
| Unaddressed ID | 2 | 0 | Working well |
| Semantic non-response | 0 | 1 | Keyword overlap threshold too aggressive |
| Deflection pattern | 0 | 0 | No cases this session |

### Tuning Actions

- Semantic overlap threshold: 30% -> 25% (reduce false positives)
- Add exception: If section was rewritten >50%, count as addressed
```

---

#### 8. Integration with GAP-FLOW-006 (Conflict Resolution)

GAP-FLOW-006 defines explicit conflict resolution. GAP-FLOW-013 feeds into it:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    CONFLICT DETECTION & RESOLUTION FLOW                     │
└─────────────────────────────────────────────────────────────────────────────┘

                        ┌─────────────────────────────┐
                        │   Reviewer Round N Output   │
                        │  (Issues with severity)     │
                        └─────────────────┬───────────┘
                                          │
                                          ▼
                        ┌─────────────────────────────┐
                        │   Engineer Round N+1 Output │
                        │  (Proposals, responses)     │
                        └─────────────────┬───────────┘
                                          │
                    ┌─────────────────────┴─────────────────────┐
                    │                                           │
                    ▼                                           ▼
        ┌───────────────────────┐               ┌───────────────────────┐
        │  Explicit DISAGREE?   │               │  GAP-FLOW-013         │
        │  (keyword match)      │               │  Implicit Detection   │
        └───────────┬───────────┘               └───────────┬───────────┘
                    │                                       │
                    │ YES                                   │ FLAGS
                    │                                       │
                    ▼                                       ▼
        ┌───────────────────────┐               ┌───────────────────────┐
        │  Direct to           │               │  User Decision:       │
        │  GAP-FLOW-006        │               │  - Request address    │
        │  Conflict Resolution │               │  - Convert to DISAGREE│
        └───────────┬───────────┘               │  - Waive              │
                    │                           │  - False positive     │
                    │                           └───────────┬───────────┘
                    │                                       │
                    │               ┌───────────────────────┘
                    │               │ (if convert)
                    ▼               ▼
        ┌───────────────────────────────────────────────────────────────┐
        │                    GAP-FLOW-006                                │
        │                 Conflict Resolution                            │
        │                                                                │
        │  1. Present Engineer vs Reviewer positions                     │
        │  2. Generate options (A: Engineer, B: Reviewer, C: Synthesis)  │
        │  3. User decides                                               │
        │  4. Record in decisions.md                                     │
        └───────────────────────────────────────────────────────────────┘
```

---

### Examples

**Example 1: Clean Detection and Resolution**

```
Round 5 Reviewer:
- ISSUE-R5-003 (HIGH): "decisions.md not restored on rollback"

Round 6 Engineer output:
- Contains "## Response to ISSUE-R5-001" (different issue)
- Contains "## Response to ISSUE-R5-002" (different issue)
- No mention of ISSUE-R5-003
- GAP-FLOW-007 section unchanged from Round 5

Detection:
- Heuristic 1 (Unaddressed ID): TRIGGER
- Heuristic 4 (Semantic): Section unchanged, TRIGGER
- Confidence: HIGH

Mediator action:
- Include in Round 6 summary
- Prompt user (HIGH severity, single issue)

User selects: "Request Engineer revision"

Round 6 retry prompt includes:
"""
You must address ISSUE-R5-003 (HIGH): "decisions.md not restored on rollback"
Include either "## Response to ISSUE-R5-003" or "DISAGREE: ISSUE-R5-003"
"""

Engineer retry output:
- Contains "## Response to ISSUE-R5-003" with solution

Detection: Issue now addressed, flag cleared
```

**Example 2: Semantic Non-Response**

```
Round 4 Reviewer:
- ISSUE-R4-001 (CRITICAL): "Token optimization contradiction - proposals describe
  fresh Task spawns but user chose 'Resume agents'"

Round 5 Engineer output:
- Contains "## Response to ISSUE-R4-001"
- Content: "We now use Task tool for each phase. The prompt includes context
  from previous rounds to maintain continuity."

Detection:
- Heuristic 1 (Unaddressed ID): NO TRIGGER (ID referenced)
- Heuristic 4 (Semantic): TRIGGER
  - Concern keywords: "resume", "agents", "context window", "warm"
  - Response keywords: "Task tool", "prompt", "includes", "continuity"
  - Overlap: 15% (below 30% threshold)

Mediator:
"""
POTENTIAL SEMANTIC NON-RESPONSE

Issue: ISSUE-R4-001 (CRITICAL)
Concern: Token optimization contradiction - fresh Tasks vs Resume agents

Engineer referenced issue but response may not address core concern.
- Concern about: Keeping agents warm across rounds
- Response about: Including context in prompts

These may be different interpretations of "Resume agents."

Options:
1. Request clarification from Engineer
2. Escalate to user for interpretation
3. Accept as addressed (Engineer's interpretation valid)
"""

User selects: "Escalate to user"

User decision: "Including context in prompts IS the intended meaning of 'Resume agents.'
This is not a contradiction. Accept as addressed."

Detection: Logged as false positive (interpretation difference, not disagreement)
```

**Example 3: Pattern Detection Across Rounds**

```
Round 3: ISSUE-R3-005 (HIGH) - "Retry mechanism undefined"
  - Engineer Round 4: No response

Round 4: ISSUE-R4-007 (HIGH) - "Same issue: retry still undefined"
  - Engineer Round 5: Partial response (mentions retry exists)

Round 5: ISSUE-R5-002 (HIGH) - "Retry mechanism STILL undefined"
  - Engineer Round 6: No response

Pattern Detection triggers:
"""
SYSTEMATIC IMPLICIT DISAGREEMENT DETECTED

The same concern has been raised 3 times across rounds:
- ISSUE-R3-005: Retry mechanism undefined
- ISSUE-R4-007: Retry mechanism undefined
- ISSUE-R5-002: Retry mechanism undefined

Engineer has not provided substantive response in any round.

This suggests systematic avoidance rather than oversight.

Recommended: Escalate to user for intervention
Options:
1. Reassign gap to different approach
2. User provides specification for Engineer to follow
3. Mark as WONT_FIX (out of scope)
4. Request Engineer explicit DISAGREE with rationale
"""
```

**Example 4: Deflection Detection**

```
Round 7 Engineer output includes:
"""
## Response to ISSUE-R7-001

The Reviewer misunderstands the purpose of this section. The rollback
mechanism as described is complete. Adding more backup retention would
create unnecessary complexity. The Reviewer's suggestion would actually
break the existing workflow.
"""

Detection:
- Heuristic 5 (Deflection): TRIGGER
  - "Reviewer misunderstands" - deflection keyword
  - "Reviewer's suggestion would break" - attacks solution without alternative
  - No alternative proposal provided

Mediator:
"""
DEFLECTION DETECTED

Engineer's response to ISSUE-R7-001 appears to reject the concern without
providing an alternative solution.

Phrases detected:
- "Reviewer misunderstands..."
- "...would actually break..."

This is different from legitimate disagreement, which would include:
- Acknowledgment of the underlying concern
- Alternative approach to address it
- Specific reasons why Reviewer's approach is problematic

Options:
1. Request Engineer provide alternative solution
2. Convert to explicit DISAGREE for conflict resolution
3. User reviews and decides
"""
```

---

### Trade-offs

**Pros:**
- Multiple detection signals catch different types of implicit disagreement
- Distinction between "forgot" and "ignored" presented to user, not auto-determined
- Configurable thresholds allow tuning per project complexity
- Feedback loop improves detection accuracy over time
- Integration with GAP-FLOW-006 provides resolution path
- User retains control over escalation decisions

**Cons:**
- Semantic analysis is computationally expensive and imperfect
- Keyword overlap threshold (30%) is heuristic, may need tuning
- False positives could frustrate Engineer (addressed by feedback loop)
- Adds Mediator bookkeeping overhead
- Detection relies on structured issue IDs (breaks if Reviewer doesn't use IDs)
- Pattern detection requires multiple rounds to trigger (slow for short sessions)

---

### Alignment with Foundations

| Foundation | Integration |
|------------|-------------|
| GAP-FLOW-010 (Lifecycle) | Issues tracked against gap states; gaps can't reach ACCEPTED with unresolved HIGH/CRITICAL issues |
| GAP-FLOW-012 (Severity) | Detection thresholds tied to severity levels; HIGH/CRITICAL get immediate attention |
| GAP-FLOW-006 (Conflict) | Detected implicit disagreements can be converted to explicit conflicts |
| GAP-FLOW-007 (Rollback) | Pattern detection integrates with rollback analysis heuristics |
| GAP-FLOW-001 (Error Recovery) | Auto-retry action uses retry mechanism from FLOW-001 |

---

### Response to Round 1 Issue (ISSUE-R1-028)

Round 1 Reviewer stated:

> "Implicit disagreement detection is hand-waved. Explicit DISAGREE is rare. Most conflicts are implicit: Engineer ignores feedback, rephrases slightly, or addresses a different interpretation."

This proposal provides:

1. **Heuristics:** 5 concrete detection signals with confidence levels
2. **Forgot vs Ignored:** Present evidence to user without auto-determining intent
3. **Thresholds:** Configurable escalation triggers by severity and frequency
4. **Presentation:** Structured flags in round summary + interactive prompts when blocking
5. **Actions:** 5 resolution options from request-address to mark-false-positive

**GAP-FLOW-006 is now unblocked.** With implicit disagreement detection defined, conflict resolution can handle both explicit and implicit conflicts.

---

### New Gaps Introduced

- **GAP-FLOW-019**: Semantic analysis quality - Keyword extraction and overlap calculation may need more sophisticated NLP for complex technical specs

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-013 | Proposed | HIGH | 1 (GAP-FLOW-019) |

**This Round:**
- Defined 5 detection heuristics with confidence levels
- Distinguished "forgot" from "ignored" via evidence presentation
- Specified escalation thresholds by severity and pattern
- Defined 5 user actions for handling detected disagreements
- Integrated with GAP-FLOW-006 conflict resolution flow
- Created feedback loop for detection accuracy tuning

**Unblocked:**
- GAP-FLOW-006 (Conflict Resolution) - can now handle implicit conflicts
- GAP-FLOW-001 (Error Recovery) - remaining blocker GAP-FLOW-008 was resolved in Round 7

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### Detection Heuristics
```
H1: Unaddressed HIGH/CRITICAL ID     (confidence: HIGH)
H2: Repeated rollback on same gap    (confidence: MEDIUM)
H3: Severity downgrade w/o rationale (confidence: MEDIUM)
H4: Semantic non-response            (confidence: MEDIUM)
H5: Deflection pattern               (confidence: HIGH)
```

### Escalation Thresholds
```
1 unaddressed CRITICAL  -> Warn user
2+ unaddressed HIGH     -> Warn user
2+ unaddressed CRITICAL -> Block round
3+ pattern across rounds -> Flag as systematic
```

### User Actions
```
1. Request explicit address (retry with instruction)
2. Convert to explicit DISAGREE (feed to FLOW-006)
3. Auto-retry with guidance
4. User waive issue
5. Mark as false positive
```

### Semantic Overlap Threshold
```
>= 30%: Counted as addressed
< 30%:  Flagged as potential non-response
```

### Deflection Keywords
```
"Reviewer misunderstood..."
"This is not a real issue..."
"Reviewer's suggestion would break..."
[Without alternative solution provided]
```
