# Engineer Proposals - Round 1

**Date:** 2026-01-09
**Focus:** Process Flow Gaps (GAP-FLOW-001 through GAP-FLOW-007)
**Priority:** Tier 1 - Blocking

---

## Gap Resolution: GAP-FLOW-001

**Gap:** No error recovery - What if Engineer produces invalid/malformed output?

**Confidence:** HIGH

### Proposed Solution

Define a **validation gate** between each phase. The Mediator (which orchestrates via Task tool) validates output structure before proceeding.

**Validation Rules:**

1. **Structural Validation:**
   - File exists at expected path (`round_N/engineer.md`, `round_N/reviewer.md`)
   - File is non-empty (minimum 100 characters)
   - File contains expected headers (`## Gap Resolution:` for Engineer, `## Review:` for Reviewer)

2. **Completeness Validation:**
   - Engineer output addresses at least one gap from `status.md`
   - Reviewer output references at least one section from Engineer output

3. **Recovery Actions:**
   | Failure Type | Recovery Action |
   |--------------|-----------------|
   | File missing | Retry Task spawn with clearer prompt (max 2 retries) |
   | File empty | Retry with explicit "produce output even if uncertain" |
   | Wrong format | Retry with example output attached |
   | All retries exhausted | Log error, notify user, proceed with partial round |

4. **Error State Recording:**
   Append to `status.md`:
   ```markdown
   ## Round N Errors
   - [TIMESTAMP] Engineer: File missing - Retry 1
   - [TIMESTAMP] Engineer: Retry successful
   ```

### Examples

**Mediator validation pseudocode:**
```python
def validate_engineer_output(round_n):
    path = f"round_{round_n:03d}/engineer.md"

    # Check existence
    if not exists(path):
        return ValidationError("FILE_MISSING", retry=True)

    content = read(path)

    # Check minimum content
    if len(content) < 100:
        return ValidationError("EMPTY_OUTPUT", retry=True)

    # Check structure
    if "## Gap Resolution:" not in content:
        return ValidationError("WRONG_FORMAT", retry=True, attach_example=True)

    # Check relevance
    gaps_addressed = extract_gap_ids(content)
    if len(gaps_addressed) == 0:
        return ValidationError("NO_GAPS_ADDRESSED", retry=True)

    return ValidationSuccess(gaps_addressed=gaps_addressed)
```

### Trade-offs

**Pros:**
- Clear recovery path for common failures
- Retries are bounded (won't loop forever)
- Errors are visible in status.md for debugging
- Graceful degradation (partial round better than no round)

**Cons:**
- Adds complexity to Mediator role
- Retry logic may mask systemic prompt issues
- "Minimum 100 chars" is arbitrary

### New Gaps Introduced

- GAP-FLOW-008: Need to define what "attach_example=True" means in retry context
- GAP-FLOW-009: What if retries succeed but output is low quality? (semantic validation)

---

## Gap Resolution: GAP-FLOW-002

**Gap:** No stall detection - How to detect process isn't converging?

**Confidence:** HIGH

### Proposed Solution

Implement a **convergence tracker** that measures gap delta per round and triggers warnings or actions.

**Convergence Metrics:**

1. **Net Progress = Gaps Resolved - New Gaps Introduced**
2. **Stall = Net Progress <= 0 for N consecutive rounds** (default N=2)
3. **Divergence = Net Progress < -2 for any single round** (scope explosion)

**State Machine:**

```
CONVERGING ──[Net > 0]──► CONVERGING
     │
     └──[Net <= 0]──► STALLED (round 1)
                           │
                           └──[Net <= 0]──► STALLED (round 2) ──► DIVERGENCE_WARNING
                           │
                           └──[Net > 0]──► CONVERGING (reset counter)
```

**Mediator Actions by State:**

| State | Action |
|-------|--------|
| CONVERGING | Continue normally |
| STALLED (1) | Warn in summary: "No net progress this round" |
| STALLED (2) | Trigger DIVERGENCE_WARNING |
| DIVERGENCE_WARNING | Present user with options via AskUserQuestion |

**User Options on Divergence:**

1. **Narrow scope:** Remove non-blocking gaps from current focus
2. **Accept complexity:** Acknowledge this spec genuinely has more gaps than initially thought
3. **Pause for input:** User provides additional context or constraints
4. **Force complete:** Accept current state as "good enough" with known gaps

### Examples

**status.md convergence section:**
```markdown
## Convergence Tracking

| Round | Gaps Start | Resolved | New | Gaps End | Net | State |
|-------|------------|----------|-----|----------|-----|-------|
| 1     | 25         | 3        | 2   | 24       | +1  | CONVERGING |
| 2     | 24         | 4        | 4   | 24       | 0   | STALLED (1) |
| 3     | 24         | 1        | 5   | 28       | -4  | DIVERGENCE_WARNING |
```

**AskUserQuestion on divergence:**
```
The workflow has not made net progress for 2 rounds.
Round 2: Resolved 4, Introduced 4 (Net: 0)
Round 3: Resolved 1, Introduced 5 (Net: -4)

Options:
1. Narrow scope - Remove non-blocking gaps
2. Accept complexity - Continue with expanded scope
3. Pause for input - Provide additional constraints
4. Force complete - Accept current state with known gaps
```

### Trade-offs

**Pros:**
- Objective measurement of progress
- Early warning before runaway divergence
- User remains in control of response
- Stall detection is cheap to compute

**Cons:**
- "Resolved" vs "New" requires judgment (is sub-gap new or clarification?)
- 2-round threshold may be too aggressive for complex specs
- Doesn't account for gap severity (closing 3 minor, opening 1 critical = bad trade)

### New Gaps Introduced

- GAP-FLOW-010: How to count "resolved" vs "new" consistently? Need gap lifecycle definition.
- GAP-FLOW-011: Should severity affect convergence calculation?

---

## Gap Resolution: GAP-FLOW-003

**Gap:** Handoff mechanics vague - How do instances know it's their turn?

**Confidence:** HIGH

### Proposed Solution

Instances do NOT know it's "their turn." The Mediator orchestrates via sequential Task tool invocations. Each Task is fire-and-forget from the instance perspective.

**Orchestration Model:**

```
Mediator (persistent context)
    │
    ├── Task(Engineer) ──► writes engineer.md ──► returns
    │
    ├── [Mediator validates]
    │
    ├── Task(Reviewer) ──► reads engineer.md, writes reviewer.md ──► returns
    │
    ├── [Mediator validates]
    │
    ├── [Mediator synthesizes summary.md]
    │
    ├── [Mediator presents decisions via AskUserQuestion]
    │
    └── [Loop or terminate]
```

**Key Points:**

1. **No coordination needed** - Mediator is single-threaded, controls order
2. **No "I'm done" signal** - Task tool returns when instance finishes
3. **No state between Tasks** - Each instance reads fresh from documents
4. **Mediator is the clock** - Rounds advance when Mediator says so

**Task Tool Behavior:**

- `Task(prompt)` spawns a new Claude instance
- Instance executes prompt, writes output files
- Instance terminates, returning control to Mediator
- Mediator reads output files, decides next action

### Examples

**Mediator pseudocode for one round:**
```python
def run_round(round_n):
    # Phase 1: Engineer
    engineer_prompt = build_engineer_prompt(round_n)
    Task(engineer_prompt)  # Blocks until complete

    validation = validate_engineer_output(round_n)
    if not validation.success:
        retry_or_skip(validation)

    # Phase 2: Reviewer
    reviewer_prompt = build_reviewer_prompt(round_n)
    Task(reviewer_prompt)  # Blocks until complete

    validation = validate_reviewer_output(round_n)
    if not validation.success:
        retry_or_skip(validation)

    # Phase 3: Synthesis
    synthesize_summary(round_n)
    update_status(round_n)

    # Phase 4: User decisions
    if decisions_needed(round_n):
        decisions = AskUserQuestion(build_questions(round_n))
        record_decisions(decisions)

    # Check termination
    if should_terminate():
        return COMPLETE
    else:
        return run_round(round_n + 1)
```

### Trade-offs

**Pros:**
- Dead simple - no distributed coordination
- Instances are stateless, replaceable
- Mediator has full control and visibility
- Natural fit for Task tool semantics

**Cons:**
- Mediator is single point of failure
- No parallelism (Engineer and Reviewer are sequential)
- Long rounds if instances are slow

### New Gaps Introduced

None. This clarifies existing design intent.

---

## Gap Resolution: GAP-FLOW-004

**Gap:** No partial round handling - What if Reviewer finds nothing?

**Confidence:** MEDIUM

### Proposed Solution

Define explicit handling for "empty" outputs from each role:

**Reviewer Finds Nothing (No Issues):**

This is a **valid outcome** that should be recorded, not ignored.

```markdown
## Review: Round N - No Issues Found

### Summary
Engineer's proposals for Round N have been reviewed.
No critical, high, medium, or low priority issues identified.

### Completeness Checks
- [x] All addressed gaps have concrete solutions
- [x] Examples are sufficient for implementation
- [x] No obvious edge cases missing
- [x] Consistent with existing architecture

### Recommendation
**APPROVE** - Proposals are ready for user decision or incorporation.
```

**Engineer Has Nothing to Propose:**

This should NOT happen if status.md has open gaps. If it does:

1. Check if all remaining gaps are blocked on user decisions
2. If blocked: Mediator presents decisions immediately
3. If not blocked: Retry with explicit gap assignment ("Address GAP-FLOW-001 specifically")

**Both Roles Empty:**

This signals completion or stall:

| Condition | Interpretation | Action |
|-----------|----------------|--------|
| Zero gaps in status.md | Workflow complete | Terminate successfully |
| Gaps exist, all blocked | Waiting on user | Present all blocking decisions |
| Gaps exist, not blocked | Process failure | Escalate to user |

### Examples

**Reviewer APPROVE format:**
```markdown
## Review: Round 3 - Approval

### Proposals Reviewed
1. GAP-FLOW-001: Error Recovery - **APPROVED**
2. GAP-FLOW-002: Stall Detection - **APPROVED with minor note**
   - Note: Consider making threshold configurable
3. GAP-FLOW-003: Handoff Mechanics - **APPROVED**

### Overall Assessment
All proposals meet implementation readiness criteria.
Ready for user decision phase.

### New Gaps Identified
None.
```

### Trade-offs

**Pros:**
- "Nothing to report" is a valid, recorded state
- Prevents phantom gaps (issues exist but unreported)
- Forces Reviewer to affirm completeness, not just stay silent
- Clear path forward in all cases

**Cons:**
- APPROVE is subjective (Reviewer could be wrong)
- No mechanism to verify Reviewer's diligence
- May slow process if Reviewer must write even for trivial approvals

### New Gaps Introduced

- GAP-ROLE-006: Should we require Reviewer to list specific checks performed?

---

## Gap Resolution: GAP-FLOW-005

**Gap:** Termination criteria incomplete - When is "good enough"?

**Confidence:** HIGH

### Proposed Solution

Define explicit, measurable termination criteria with user override capability.

**Automatic Termination Triggers:**

| Trigger | Condition | Result |
|---------|-----------|--------|
| COMPLETE | Zero open gaps AND Reviewer APPROVE | Success |
| USER_APPROVED | User selects "Accept as complete" | Success with known gaps |
| MAX_ROUNDS | 10 rounds in fully automated mode | Forced stop |
| ABANDONED | User selects "Abandon session" | Terminated |

**Termination Decision Tree:**

```
┌─ Check: Open gaps = 0?
│   └─ YES: Check: Reviewer APPROVE?
│       └─ YES: COMPLETE (auto)
│       └─ NO: Continue round
│   └─ NO: Check: All gaps blocked on decisions?
│       └─ YES: Present decisions, continue
│       └─ NO: Check: User approved early?
│           └─ YES: USER_APPROVED
│           └─ NO: Continue round
```

**"Good Enough" Criteria:**

User can accept early termination if:
1. No **CRITICAL** gaps remain
2. No **HIGH** gaps remain (or user explicitly accepts them)
3. All **blocking** gaps are resolved
4. User has reviewed and understands remaining gaps

**Final Summary Format:**

```markdown
## Session Complete

**Status:** COMPLETE | USER_APPROVED | ABANDONED
**Rounds:** 5
**Duration:** 2 hours

### Gap Summary
| Status | Count |
|--------|-------|
| Resolved | 22 |
| Accepted as-is | 3 |
| Deferred | 0 |
| Total | 25 |

### Known Limitations
- GAP-UX-003: Example session incomplete (accepted - low priority)
- GAP-AUTO-002: No CI/CD integration (accepted - out of scope)

### Output
Final spec written to: `specs/[name]_v1.0.md`
```

### Examples

**AskUserQuestion for early termination:**
```
Session Progress: Round 5 of max 10
Open Gaps: 3 (0 Critical, 0 High, 2 Medium, 1 Low)

Options:
1. Continue - Keep refining (2 Medium gaps remain)
2. Accept as complete - Document known limitations
3. Pause - Save state for later
4. Abandon - Discard session
```

### Trade-offs

**Pros:**
- Clear definition of "done"
- User controls quality threshold
- Prevents infinite loops
- Explicit about known limitations

**Cons:**
- 10-round limit is arbitrary (may be too few for complex specs)
- "No CRITICAL gaps" requires consistent severity rating
- User might accept too early under time pressure

### New Gaps Introduced

- GAP-FLOW-012: Need severity definitions (CRITICAL/HIGH/MEDIUM/LOW)

---

## Gap Resolution: GAP-FLOW-006

**Gap:** No conflict resolution - What if Engineer disagrees with Reviewer?

**Confidence:** MEDIUM

### Proposed Solution

Conflicts are resolved by the **user**, with Mediator presenting both positions fairly.

**Conflict Detection:**

A conflict exists when:
1. Reviewer marks an issue as HIGH/CRITICAL
2. Engineer's next-round response explicitly rejects the feedback
3. The gap remains unresolved after Engineer's response

**Explicit Disagreement Format (Engineer):**

```markdown
## Response to ISSUE-R2-001

**Reviewer Concern:** [quote from reviewer.md]

**Engineer Position:** DISAGREE

**Rationale:** [Why the concern is invalid or overblown]

**Alternative View:** [If there's a middle ground]

**Request:** Escalate to user for decision
```

**Conflict Presentation (Mediator):**

```markdown
### Conflict: ISSUE-R2-001 - Error Threshold

**Reviewer Position:**
Error threshold of 100 chars is too arbitrary. Should be schema-based.

**Engineer Position:**
Schema-based validation adds complexity for marginal benefit.
100 chars catches 99% of empty files.

| Option | Supporter | Trade-off |
|--------|-----------|-----------|
| A: Keep 100 chars | Engineer | Simple but arbitrary |
| B: Schema-based | Reviewer | Accurate but complex |
| C: Configurable | Neither | Flexibility at cost of decision |

**Awaiting user decision.**
```

**Resolution Recording:**

```markdown
## decisions.md

### Round 2: ISSUE-R2-001 - Error Threshold
**Conflict:** Engineer vs Reviewer on validation approach
**Decision:** Option C (Configurable with 100 char default)
**Rationale:** Best of both worlds
**Decided by:** User
**Date:** 2026-01-09
```

### Examples

**Conflict lifecycle:**
```
Round N: Reviewer raises ISSUE-R{N}-001
Round N+1: Engineer writes DISAGREE response
Mediator detects: Proposal references issue but no resolution
Mediator action: Present conflict to user
User decides: Choice recorded in decisions.md
Round N+2: Engineer proceeds with user's choice
```

### Trade-offs

**Pros:**
- User as final arbiter prevents stalemates
- Both positions documented for future reference
- Forces explicit disagreement (can't silently ignore feedback)
- Structured format makes conflicts visible

**Cons:**
- Slows process (adds user decision point)
- Engineer might overuse DISAGREE to avoid work
- Reviewer might feel overruled too often
- Requires Mediator to detect implicit disagreements

### New Gaps Introduced

- GAP-FLOW-013: How to detect implicit disagreement (Engineer ignores feedback without DISAGREE)?

---

## Gap Resolution: GAP-FLOW-007

**Gap:** No rollback mechanism - How to undo a bad round?

**Confidence:** HIGH

### Proposed Solution

Implement **round-level rollback** where Mediator can discard a round's outputs and retry.

**Rollback Scope:**

Rollback is at the round level:
- Delete `round_N/` folder
- Restore `status.md` to state at end of round N-1
- Decrement round counter
- Optionally: Adjust prompts before retry

**Rollback Triggers:**

| Trigger | Initiated By | Example |
|---------|--------------|---------|
| User request | User | "That round was unproductive, redo" |
| Validation failure | Mediator | Both retries failed, user decides |
| Divergence | Mediator | Net progress severely negative |

**Rollback Mechanics:**

```python
def rollback_round(round_n):
    # 1. Archive (don't delete) the bad round
    move(f"round_{round_n:03d}/", f"round_{round_n:03d}_rolled_back/")

    # 2. Restore status.md from before round N
    copy(f"status_backup_round_{round_n-1}.md", "status.md")

    # 3. Append rollback event to status.md
    append_to_status(f"## Round {round_n} Rolled Back\nReason: {reason}")

    # 4. Prompt user for adjustments
    adjustments = AskUserQuestion([
        "What should change in the retry?",
        "Should we narrow scope?",
        "Any additional context?"
    ])

    # 5. Retry round with adjustments
    return run_round(round_n, adjustments)
```

**State Backup:**

Before each round starts, Mediator creates:
- `status_backup_round_N.md` - snapshot of status.md
- `decisions_backup_round_N.md` - snapshot of decisions.md

Backups are overwritten each round (only need N-1 for rollback).

**Rollback Limits:**

- Maximum 2 rollbacks per round (3 attempts total)
- Maximum 5 rollbacks per session (prevents infinite retry loops)
- After limits: Force proceed with best available output

### Examples

**Rollback in status.md:**
```markdown
## Round 3 Rolled Back

**Reason:** User request - "Proposals were too abstract"
**Original Files:** Archived in round_003_rolled_back/
**Retry Adjustments:**
- Engineer: Include concrete code examples
- Scope: Focus only on GAP-FLOW-001 and GAP-FLOW-002
```

**User-initiated rollback flow:**
```
User: The last round wasn't helpful. Can we redo it?

Mediator: Round 3 rollback options:
1. Retry with same prompt (maybe different result)
2. Retry with narrowed scope (fewer gaps)
3. Retry with additional context (you provide hints)
4. Skip round, proceed to Round 4

User selects option 3, provides: "Focus on error recovery, not stall detection"

Mediator: Rolling back Round 3. Retry with focus on error recovery.
[Spawns Engineer Task with adjusted prompt]
```

### Trade-offs

**Pros:**
- Recovery from unproductive rounds
- Preserves bad output for debugging (archived, not deleted)
- User controls when to rollback
- Bounded (can't rollback forever)

**Cons:**
- Adds complexity to state management
- Rollback limits are arbitrary
- Doesn't address WHY round failed (might fail again)
- Backup files add clutter

### New Gaps Introduced

- GAP-FLOW-014: Cleanup strategy for rolled-back round folders
- GAP-FLOW-015: How to analyze rolled-back rounds to improve prompts

---

## Summary

| Gap ID | Resolution Status | Confidence | New Gaps |
|--------|-------------------|------------|----------|
| GAP-FLOW-001 | Proposed | HIGH | 2 (GAP-FLOW-008, 009) |
| GAP-FLOW-002 | Proposed | HIGH | 2 (GAP-FLOW-010, 011) |
| GAP-FLOW-003 | Proposed | HIGH | 0 |
| GAP-FLOW-004 | Proposed | MEDIUM | 1 (GAP-ROLE-006) |
| GAP-FLOW-005 | Proposed | HIGH | 1 (GAP-FLOW-012) |
| GAP-FLOW-006 | Proposed | MEDIUM | 1 (GAP-FLOW-013) |
| GAP-FLOW-007 | Proposed | HIGH | 2 (GAP-FLOW-014, 015) |

**Net This Round:**
- Gaps Addressed: 7
- New Gaps Introduced: 9
- Note: New gaps are refinements/clarifications, not scope expansion

**Ready for Reviewer assessment.**
