# Engineer Proposals - Round 2

**Date:** 2026-01-12
**Focus:** Foundational Definitions (GAP-FLOW-010, GAP-FLOW-012)
**Priority:** Tier 0 - Unblocking

**Context:** Round 1 revealed circular dependencies. GAP-FLOW-002 (Stall Detection), GAP-FLOW-005 (Termination Criteria), and GAP-FLOW-006 (Conflict Resolution) all depend on undefined foundational concepts. This round defines those foundations.

---

## Gap Resolution: GAP-FLOW-010

**Gap:** Gap lifecycle definition - What states can a gap be in? How do gaps transition between states?

**Confidence:** HIGH

### Proposed Solution

Define a **finite state machine** for gap lifecycle with explicit transition rules, actors, and counting implications.

#### Gap States

| State | Definition | Counted as Open? |
|-------|------------|------------------|
| **OPEN** | Gap identified, no work started | Yes |
| **IN_PROGRESS** | Engineer actively working on resolution | Yes |
| **PROPOSED** | Engineer submitted solution, awaiting review | Yes |
| **NEEDS_REVISION** | Reviewer found issues, Engineer must revise | Yes |
| **ACCEPTED** | Reviewer approved, awaiting user confirmation | No |
| **USER_DEFERRED** | User explicitly deferred to future round/session | Yes (weighted 0.5) |
| **RESOLVED** | Incorporated into final spec | No |
| **WONT_FIX** | User explicitly closed as out of scope | No |

**Key Insight:** ACCEPTED counts as closed for convergence purposes, even before user confirms. This prevents false "stall" when waiting on user decisions.

#### State Transition Diagram

```
                                    ┌──────────────────┐
                                    │                  │
                                    ▼                  │
┌──────┐  Engineer   ┌─────────────┐  Reviewer  ┌──────────┐
│ OPEN │────────────►│ IN_PROGRESS │───────────►│ PROPOSED │
└──────┘   claims    └─────────────┘  submits   └──────────┘
    │                                                │
    │                                    ┌───────────┴───────────┐
    │                                    │                       │
    │                                    ▼                       ▼
    │                        ┌────────────────┐         ┌──────────┐
    │                        │ NEEDS_REVISION │         │ ACCEPTED │
    │                        └────────────────┘         └──────────┘
    │                                │                       │
    │                                │ Engineer              │
    │                                │ revises               │
    │                                ▼                       │
    │                        ┌───────────┐                   │
    │                        │ PROPOSED  │◄──────────────────┘
    │                        └───────────┘        │
    │                                             │ User confirms
    │                                             ▼
    │      User defers           ┌───────────────────────────┐
    └───────────────────────────►│        USER_DEFERRED      │
    │                            └───────────────────────────┘
    │
    │      User closes           ┌───────────────────────────┐
    └───────────────────────────►│         WONT_FIX          │
                                 └───────────────────────────┘

                                 ┌───────────────────────────┐
       ACCEPTED + user confirm ──►│         RESOLVED          │
                                 └───────────────────────────┘
```

#### Transition Rules

| Transition | Actor | Trigger | Validation |
|------------|-------|---------|------------|
| OPEN -> IN_PROGRESS | Engineer | Engineer starts work on gap | Gap must be in priority list |
| IN_PROGRESS -> PROPOSED | Engineer | Writes `## Gap Resolution: GAP-XXX` | Proposal must exist in engineer.md |
| PROPOSED -> NEEDS_REVISION | Reviewer | Raises CRITICAL or HIGH issue | Issue must reference gap |
| PROPOSED -> ACCEPTED | Reviewer | No CRITICAL/HIGH issues | Explicit sign-off in reviewer.md |
| NEEDS_REVISION -> PROPOSED | Engineer | Addresses issues | Must reference issue IDs |
| OPEN -> USER_DEFERRED | User | Explicit deferral via AskUserQuestion | Must provide reason |
| OPEN -> WONT_FIX | User | Explicit closure | Must provide reason |
| ACCEPTED -> RESOLVED | Mediator | User confirms OR auto-confirm after 1 round | Gap content in final spec |

#### Counting Rules for Convergence

**Definition of "Open Gaps":**
```
open_gap_count = count(OPEN) + count(IN_PROGRESS) + count(PROPOSED)
                 + count(NEEDS_REVISION) + (0.5 * count(USER_DEFERRED))
```

**Definition of "Resolved This Round":**
```
resolved_this_round = gaps that transitioned TO (ACCEPTED | RESOLVED | WONT_FIX) this round
```

**Definition of "New Gaps This Round":**
```
new_this_round = gaps created this round in OPEN state
```

**Net Progress:**
```
net_progress = resolved_this_round - new_this_round
```

#### State Recording in status.md

```markdown
## Gap Status (Round 3)

| Gap ID | State | Round Entered | Last Updated | Blocking? |
|--------|-------|---------------|--------------|-----------|
| GAP-FLOW-001 | ACCEPTED | 1 | 3 | No |
| GAP-FLOW-002 | NEEDS_REVISION | 1 | 3 | Yes (on GAP-FLOW-010) |
| GAP-FLOW-010 | PROPOSED | 2 | 2 | No |
| GAP-FLOW-015 | OPEN | 3 | 3 | No |
```

### Examples

**Example 1: Normal Resolution Flow**
```
Round 1:
  - GAP-FLOW-001: OPEN -> PROPOSED (Engineer submits)
Round 2:
  - GAP-FLOW-001: PROPOSED -> NEEDS_REVISION (Reviewer finds issue)
Round 3:
  - GAP-FLOW-001: NEEDS_REVISION -> PROPOSED -> ACCEPTED (Engineer fixes, Reviewer approves)
Round 4:
  - GAP-FLOW-001: ACCEPTED -> RESOLVED (User confirms, added to final spec)
```

**Example 2: Fast-Track Resolution**
```
Round 1:
  - GAP-FLOW-003: OPEN -> PROPOSED -> ACCEPTED (Clean proposal, immediate approval)
Round 2:
  - GAP-FLOW-003: ACCEPTED -> RESOLVED (User confirmation)
```

**Example 3: User Deferral**
```
Round 2:
  - GAP-AUTO-002: OPEN -> USER_DEFERRED (User: "CI/CD is out of scope for v1")

Convergence impact: Counts as 0.5 open gap, not 1.0
```

**Example 4: Spawning Sub-Gaps**
```
Round 1:
  - GAP-FLOW-001: OPEN -> PROPOSED
  - GAP-FLOW-008: OPEN (NEW - spawned by GAP-FLOW-001 proposal)
  - GAP-FLOW-009: OPEN (NEW - spawned by GAP-FLOW-001 proposal)

net_progress = 0 - 2 = -2 (but this is expected for complex gaps)
```

### Trade-offs

**Pros:**
- Clear, unambiguous state machine
- Counting rules are objective and computable
- ACCEPTED vs RESOLVED distinction prevents stalls on user latency
- USER_DEFERRED reduces noise without ignoring gaps
- Blocking relationships are explicit

**Cons:**
- 8 states may be more than necessary (could collapse IN_PROGRESS into OPEN)
- 0.5 weighting for USER_DEFERRED is arbitrary (but tunable)
- State transitions require Mediator bookkeeping (adds overhead)
- Doesn't capture gap dependencies (gap A blocks gap B)

### Open Question

Should we track gap dependencies explicitly?

**Argument FOR:** GAP-FLOW-002 explicitly blocked on GAP-FLOW-010. Without tracking, Mediator can't auto-route.

**Argument AGAINST:** Adds complexity. Dependencies can be noted in gap descriptions.

**Recommendation:** Add optional `blocked_by: [GAP-XXX, GAP-YYY]` field to status.md gap entries. Mediator uses this to deprioritize blocked gaps.

### New Gaps Introduced

None. This is a foundational definition.

---

## Gap Resolution: GAP-FLOW-012

**Gap:** Severity level definitions - What do CRITICAL/HIGH/MEDIUM/LOW mean? How does severity affect the workflow?

**Confidence:** HIGH

### Proposed Solution

Define severity levels with **concrete criteria**, **examples from this workflow**, and **workflow implications**.

#### Severity Level Definitions

| Severity | Definition | Criteria | Termination Impact |
|----------|------------|----------|-------------------|
| **CRITICAL** | Spec cannot be implemented | Missing core concept, logical contradiction, undefined behavior for common case | Must be zero for completion |
| **HIGH** | Implementation will be incorrect/incomplete | Missing edge case handling, undefined failure mode, inconsistency with existing spec | Must be zero for "good enough" |
| **MEDIUM** | Implementation possible but suboptimal | Performance concern, UX friction, maintainability issue | Can accept with acknowledgment |
| **LOW** | Polish item, nice-to-have | Documentation gap, example incompleteness, naming nitpick | Can ignore |

#### Severity Classification Rubric

**Is it CRITICAL?** Ask:
1. If we implement without addressing this, will the core use case fail?
2. Is there a logical contradiction that makes spec internally inconsistent?
3. Is a fundamental concept undefined (not just underspecified)?

If YES to any: CRITICAL

**Is it HIGH?** Ask:
1. If we implement without addressing this, will a common (>10%) use case fail?
2. Is a failure mode unspecified (system will fail, but spec doesn't say how)?
3. Is the spec inconsistent with a related, approved spec?

If YES to any: HIGH

**Is it MEDIUM?** Ask:
1. If we implement without addressing this, will edge cases (<10%) fail?
2. Is there a performance/scalability concern for large inputs?
3. Is there friction that could be avoided with better design?

If YES to any: MEDIUM

**Otherwise:** LOW

#### Concrete Examples (From This Workflow)

**CRITICAL Examples:**
- "Gap lifecycle undefined" (GAP-FLOW-010 before this round)
  - WHY: Stall detection (GAP-FLOW-002) literally cannot be implemented. Formula references "resolved" but "resolved" is undefined.
- "Handoff mechanics vague" (GAP-FLOW-003 before Round 1)
  - WHY: Without knowing how instances coordinate, nothing else works.

**HIGH Examples:**
- "Retry prompt modification is undefined" (ISSUE-R1-001)
  - WHY: Error recovery will run, but retries may fail identically. 10% of cases involve errors.
- "USER_APPROVED allows premature closure with CRITICAL gaps" (ISSUE-R1-024)
  - WHY: User can bypass safety checks. Not a common case, but when it happens, spec quality suffers.

**MEDIUM Examples:**
- "10-round limit rationale missing" (ISSUE-R1-026)
  - WHY: Workflow will work, but limit may be too aggressive for complex specs.
- "Conflict presentation table underspecified" (ISSUE-R1-031)
  - WHY: Mediator will guess, probably reasonably. Not core functionality.

**LOW Examples:**
- "ValidationError vs ValidationSuccess inconsistent return types" (ISSUE-R1-007)
  - WHY: Pseudocode, not implementation. Doesn't affect actual behavior.
- "Error recording timestamp format undefined" (ISSUE-R1-006)
  - WHY: Any format works. ISO 8601 is obvious default.

#### Severity Impact on Termination

**Termination Criteria (Revised from GAP-FLOW-005):**

| Termination Type | CRITICAL | HIGH | MEDIUM | LOW |
|------------------|----------|------|--------|-----|
| COMPLETE (automatic) | 0 | 0 | any | any |
| GOOD_ENOUGH (user accepts) | 0 | user-acknowledged | any | any |
| USER_APPROVED (force) | user-warned | user-warned | any | any |

**Interpretation:**
- `user-acknowledged`: User explicitly listed gap in "Known Limitations"
- `user-warned`: System shows warning: "Accepting with CRITICAL/HIGH gaps may result in incomplete spec"

**AskUserQuestion on Early Termination (Updated):**
```
Session Progress: Round 5 of max 10
Open Gaps: 3

Severity Breakdown:
- CRITICAL: 0
- HIGH: 1 (GAP-FLOW-013: Implicit disagreement detection)
- MEDIUM: 1 (GAP-ROLE-003: Mediator fairness is subjective)
- LOW: 1 (GAP-UX-003: Example session incomplete)

Options:
1. Continue - Address HIGH gap (recommended)
2. Accept HIGH gap as known limitation - Document and proceed
3. Pause - Save state for later
4. Force complete - Proceed with warnings
```

#### Severity Impact on Convergence (Optional Weighting)

From ISSUE-R1-009: "A round that closes 10 LOW gaps but opens 1 CRITICAL shows '+9' progress but is actually regression."

**Weighted Convergence Formula:**

```
severity_weight = {CRITICAL: 8, HIGH: 4, MEDIUM: 2, LOW: 1}

weighted_resolved = sum(severity_weight[g.severity] for g in resolved_this_round)
weighted_new = sum(severity_weight[g.severity] for g in new_this_round)

weighted_net_progress = weighted_resolved - weighted_new
```

**Example:**
```
Round 3:
  Resolved: 3 LOW gaps (3 * 1 = 3)
  New: 1 CRITICAL gap (1 * 8 = 8)

  Unweighted net: +2 (looks good!)
  Weighted net: -5 (reveals regression)
```

**When to use weighted vs unweighted:**
- **Unweighted:** Quick sanity check, dashboard display
- **Weighted:** Stall/divergence detection, termination decisions

#### Severity Assignment Process

**Who assigns severity?**

| Stage | Actor | Severity Source |
|-------|-------|-----------------|
| Gap creation | Engineer/Reviewer | Initial assessment in proposal |
| Gap review | Reviewer | May upgrade if Engineer underestimated |
| Conflict | Mediator | Presents both assessments to user |
| Final | User | User can override any severity |

**Reviewer can upgrade, not downgrade:**
- If Engineer says MEDIUM, Reviewer can say HIGH
- If Engineer says HIGH, Reviewer cannot say LOW
- Rationale: Reviewer is conservative; only user can accept lower severity

**Recording in status.md:**
```markdown
## Gap Inventory

| Gap ID | Severity | Assigned By | State |
|--------|----------|-------------|-------|
| GAP-FLOW-001 | HIGH | Reviewer (upgraded from MEDIUM) | ACCEPTED |
| GAP-FLOW-010 | CRITICAL | Engineer | RESOLVED |
| GAP-UX-003 | LOW | Engineer | OPEN |
```

### Examples

**Example 1: Severity Disagreement**
```markdown
## Round 2 Summary

### Severity Conflict: GAP-FLOW-001

**Engineer Assessment:** MEDIUM
- "Error recovery is edge case. Most rounds succeed."

**Reviewer Assessment:** HIGH (upgrade)
- "Error recovery is fundamental. When it fails, entire session stalls."

**Mediator Note:** Severity conflict flagged. Using higher severity (HIGH) for termination checks.
User can override via: "Override GAP-FLOW-001 severity to MEDIUM"
```

**Example 2: Termination with HIGH Gap**
```
Round 8 Status:
- CRITICAL: 0
- HIGH: 1 (GAP-FLOW-013)
- MEDIUM: 2
- LOW: 3

User selects: "Accept HIGH gap as known limitation"

decisions.md entry:
### GAP-FLOW-013 - Accepted as Known Limitation
**Severity:** HIGH
**User Decision:** Accept without resolution
**Rationale:** "Implicit disagreement is rare. Manual detection acceptable for v1."
**Date:** 2026-01-12
```

**Example 3: Weighted Convergence Preventing False Progress**
```
Round 4 Progress (Unweighted):
- Resolved: 5
- New: 3
- Net: +2 (CONVERGING)

Round 4 Progress (Weighted):
- Resolved: 2 LOW (2), 2 MEDIUM (4), 1 HIGH (4) = 10
- New: 1 CRITICAL (8), 2 LOW (2) = 10
- Net: 0 (STALLED)

Mediator flags: "Unweighted progress is +2, but weighted is 0. A CRITICAL gap was introduced."
```

### Trade-offs

**Pros:**
- Objective criteria for severity classification
- Concrete examples make decisions easier
- Weighted convergence catches false progress
- Reviewer-can-only-upgrade prevents sandbagging
- User retains final authority

**Cons:**
- Severity weights (8/4/2/1) are arbitrary (but follow 2x pattern)
- Classification rubric still requires judgment
- Weighted convergence adds complexity to status tracking
- May slow process if every severity is debated

### New Gaps Introduced

None. This is a foundational definition.

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-010 | Proposed | HIGH | 0 |
| GAP-FLOW-012 | Proposed | HIGH | 0 |

**Net This Round:**
- Gaps Addressed: 2 (foundational)
- New Gaps Introduced: 0
- Unblocked: GAP-FLOW-001, 002, 004, 005, 006, 007 (all dependent on foundations)

**Dependencies Resolved:**
- GAP-FLOW-002 (Stall Detection): Now has gap lifecycle definition to count against
- GAP-FLOW-005 (Termination): Now has severity definitions for "good enough" criteria
- GAP-FLOW-006 (Conflict Resolution): Now has severity for prioritization

**Ready for Reviewer assessment.**

---

## Appendix: Quick Reference

### Gap States Summary
```
OPEN -> IN_PROGRESS -> PROPOSED -> ACCEPTED -> RESOLVED
                           |
                           v
                    NEEDS_REVISION
                           |
                           v
                       PROPOSED (loop)

USER_DEFERRED (from OPEN)
WONT_FIX (from OPEN)
```

### Severity Quick Guide
```
CRITICAL: "Spec literally cannot be implemented"
HIGH:     "Implementation will be wrong for common cases"
MEDIUM:   "Implementation will be suboptimal"
LOW:      "Polish and documentation"
```

### Counting Quick Reference
```
Open = OPEN + IN_PROGRESS + PROPOSED + NEEDS_REVISION + (0.5 * USER_DEFERRED)
Net = Resolved - New
Weighted Net = (8*CRIT + 4*HIGH + 2*MED + 1*LOW)resolved - (same)new
```
