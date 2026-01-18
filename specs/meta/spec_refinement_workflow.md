# Automated Spec Refinement Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 2.5
**Category:** Analysis workflow (per workflow_manager.md Section 3.3.1)
**Purpose:** Multi-instance Claude system for iterative specification refinement
**Self-Refined:** 11 rounds, 14 gaps resolved (v2.4 additions refined in Round 11)

---

## 1. Overview

This document defines a **3-instance Claude workflow** for refining partial specifications into complete, implementable specs. The system uses structured markdown documents for inter-instance communication.

### 1.1 Design Principles

1. **Separation of Concerns** - Each instance has a distinct role
2. **True Isolation** - Instances run as separate processes (via Task tool), sharing only documents
3. **User as Final Authority** - User approves all significant decisions via interactive prompts
4. **Transparent Progress** - All gaps, issues, and decisions are visible
5. **Convergent Refinement** - Each round should reduce total gaps
6. **Foundations First** - Define lifecycle/severity before dependent gaps
7. **Single-Gap Focus (Default)** - One gap per round by default for clean resolution; independent gaps may be processed in parallel (Section 11.3)
8. **Semantic Compression** - Extract common patterns only after 2+ instances; concrete before abstract

### 1.2 Implementation Architecture

**Instance Spawning:** Engineer and Reviewer run as separate Task tool invocations:
- No context bleed between roles
- Reviewer sees only Engineer's output, not reasoning
- Each instance operates on explicit document inputs only

**Token Optimization (Context-Primed Deltas):**
- Round 1: Spawn fresh instances, they read full context
- Round 2+: Pass delta summaries, not full re-reads
- Deltas include: Reviewer feedback, user decisions, status changes

**User Interaction:** Mediator uses `AskUserQuestion` tool:
- Interactive, immediate feedback
- Structured options with descriptions
- Decisions recorded automatically to `decisions.md`

---

## 2. Gap Lifecycle

Gaps progress through defined states. This enables accurate convergence tracking.

### 2.1 States

| State | Counted as Open? | Description |
|-------|------------------|-------------|
| OPEN | Yes | Initial state, not yet addressed |
| IN_PROGRESS | Yes | Engineer is working on it |
| BLOCKED | Yes | Gap cannot proceed; waiting on blocker resolution |
| PROPOSED | Yes | Solution submitted, awaiting review |
| NEEDS_REVISION | Yes | Reviewer found issues |
| ACCEPTED | No | Reviewer approved |
| USER_DEFERRED | Yes (0.5 weight) | User chose to defer |
| RESOLVED | No | Incorporated into final spec |
| WONT_FIX | No | Explicitly rejected |

### 2.2 Transitions

```
OPEN ──[Engineer starts]──► IN_PROGRESS
IN_PROGRESS ──[Engineer submits]──► PROPOSED
IN_PROGRESS ──[Blocker discovered]──► BLOCKED
BLOCKED ──[Blocker resolved]──► IN_PROGRESS
PROPOSED ──[Reviewer approves]──► ACCEPTED
PROPOSED ──[Reviewer rejects]──► NEEDS_REVISION
NEEDS_REVISION ──[Engineer revises]──► PROPOSED
NEEDS_REVISION ──[Blocker discovered]──► BLOCKED
ACCEPTED ──[User confirms]──► RESOLVED
OPEN ──[User defers]──► USER_DEFERRED
USER_DEFERRED ──[User resumes]──► OPEN
ANY ──[User rejects]──► WONT_FIX
```

### 2.3 Counting Formula

```
open_gap_count = count(OPEN) + count(IN_PROGRESS) + count(BLOCKED)
              + count(PROPOSED) + count(NEEDS_REVISION)
              + (0.5 * count(USER_DEFERRED))

resolved_this_round = transitions TO (ACCEPTED | RESOLVED | WONT_FIX)
new_this_round = gaps created this round in OPEN state
net_progress = resolved_this_round - new_this_round
```

### 2.4 Blocking Mechanism

Gaps transition to BLOCKED when progress is impossible due to external factors.

#### 2.4.1 Blocker Types

| Type | Source | Example |
|------|--------|---------|
| DIVERGENCE | Section 10.5.5 | Gap assumes field exists, but codebase differs |
| DEPENDENCY | Section 11.3.1 | Gap A requires Gap B's resolution first |
| USER_INPUT | Section 8 | Conflict requires user decision |
| EXTERNAL | - | Waiting on external API/resource |

#### 2.4.2 Blocker Format

```markdown
**Blocked By:** {BLOCKER_TYPE}: {BLOCKER_ID}
**Description:** [Human-readable explanation]
**Since:** {ISO8601 timestamp}
**Unblock Action:** [What needs to happen]
```

#### 2.4.3 Unblocking Rules

| Blocker Type | Unblock Trigger |
|--------------|-----------------|
| DIVERGENCE | DIVERGE-XXX resolved to SPEC_STALE/SPEC_ASPIRATIONAL/etc. |
| DEPENDENCY | Dependent gap reaches ACCEPTED or RESOLVED |
| USER_INPUT | User makes decision via AskUserQuestion |
| EXTERNAL | Mediator confirms external condition met |

#### 2.4.4 Force-Unblock

User may force a gap to IN_PROGRESS despite unresolved blocker:
- Requires explicit acknowledgment: "I accept risk that {blocker} may invalidate this work"
- Recorded in decisions.md
- Gap marked with `FORCED_UNBLOCK` flag for traceability

---

## 3. Severity Levels

Issues and gaps are classified by severity, affecting prioritization and termination.

### 3.1 Definitions

| Severity | Weight | Definition | Termination Impact |
|----------|--------|------------|-------------------|
| CRITICAL | 16 | Spec cannot be implemented without this | Must be zero |
| HIGH | 4 | Implementation will be incorrect without this | Must be zero (or user-acknowledged) |
| MEDIUM | 2 | Implementation possible but suboptimal | Can accept with acknowledgment |
| LOW | 1 | Polish item, nice-to-have | Can ignore |

### 3.2 Classification Rubric

Ask these questions in order:
1. Can the spec be implemented at all without this? No → CRITICAL
2. Will the implementation be wrong without this? Yes → HIGH
3. Will the implementation be suboptimal? Yes → MEDIUM
4. Otherwise → LOW

### 3.3 Weighted Convergence

```
weighted_open = (16 * CRITICAL) + (4 * HIGH) + (2 * MEDIUM) + (1 * LOW)
weighted_resolved = sum of weights of gaps resolved this round
weighted_new = sum of weights of new gaps
weighted_net = weighted_resolved - weighted_new
```

**CRITICAL Override:** Any new CRITICAL gap triggers DIVERGENCE_WARNING regardless of weighted_net.

---

## 4. Stall Detection

Track convergence state to detect when the process isn't making progress.

### 4.1 States

| State | Condition | Action |
|-------|-----------|--------|
| CONVERGING | weighted_net > 0 | Continue normally |
| FLAT | weighted_net = 0 for 1 round | Monitor, continue |
| STALLED | weighted_net <= 0 for 2 rounds | Warn user |
| DIVERGENCE_WARNING | weighted_net < -8 OR new CRITICAL | Present user options |
| PAUSED | User requested pause | Persist state for resume |
| COMPLETE | Termination criteria met | End session |

### 4.2 User Options on Divergence

When DIVERGENCE_WARNING triggers, present via AskUserQuestion:
1. **Narrow scope** - Remove non-blocking gaps from focus
2. **Accept complexity** - Acknowledge expanded scope, continue
3. **Pause for input** - User provides additional context
4. **Force complete** - Accept current state with known gaps

---

## 5. Termination Criteria

### 5.1 Termination Types

| Type | Trigger | Result |
|------|---------|--------|
| COMPLETE | Zero open gaps AND Reviewer NO_ISSUES_FOUND | Success |
| GOOD_ENOUGH | Zero CRITICAL/HIGH, MEDIUM/LOW acknowledged | Success with known gaps |
| USER_APPROVED | User explicitly accepts with warning | Success with known gaps |
| MAX_ROUNDS | `max(10, ceil(initial_gaps * 0.6))` reached | Forced stop |
| STALL_EXIT | User exits via DIVERGENCE_WARNING | Partial success |
| ABANDONED | User cancels session | No output |

### 5.2 Warning Gates

If user selects USER_APPROVED with remaining gaps:
- **CRITICAL remaining:** Require typing "I accept CRITICAL gaps knowing the spec may be unusable"
- **HIGH remaining:** Require typing "I acknowledge" plus rationale
- **MEDIUM/LOW remaining:** Simple confirmation

### 5.3 Final Summary

```markdown
## Session Complete

**Status:** COMPLETE | GOOD_ENOUGH | USER_APPROVED | ABANDONED
**Rounds:** N
**Duration:** X hours

### Gap Summary
| Status | Count |
|--------|-------|
| Resolved | X |
| Accepted as-is | Y |
| Deferred | Z |

### Known Limitations
- [List any accepted but unresolved gaps]
```

---

## 6. Error Recovery

Handle invalid or malformed output from Engineer or Reviewer.

### 6.1 Validation Tiers

**Tier 1 (Structural):**
- File exists at expected path
- File is non-empty (>100 chars)
- Required headers present (`## Gap Resolution:` or `## Review:`)

**Tier 2 (Content):**
- At least one gap addressed
- Gap ID format valid: `GAP-[A-Z]{2,10}-\d{3}`
- Cross-references to status.md valid

### 6.2 Retry Protocol

| Failure Type | Retry Action | Max Retries |
|--------------|--------------|-------------|
| FILE_MISSING | Re-prompt with explicit path | 2 |
| EMPTY_OUTPUT | Re-prompt with minimum template | 2 |
| WRONG_FORMAT | Re-prompt with example attached | 2 |
| NO_GAPS_ADDRESSED | Re-prompt with explicit gap assignment | 2 |

### 6.3 Example Attachment

When retry needs example (WRONG_FORMAT), select from:
1. **Tier 1 (Canonical):** Example from Section 7 of this spec
2. **Tier 2 (Session):** Successful output from previous round
3. **Tier 3 (Template):** Minimal structural template

Selection criteria: Same role, same gap type, most recent, passed validation.

### 6.4 Escalation

After 2 failed retries:
1. Skip this phase, proceed with partial round
2. Mark round as incomplete in status.md
3. Present user with options: retry, skip, or pause

---

## 7. Partial Round Handling

### 7.1 Reviewer Finds Nothing

Valid outcome. Reviewer writes:

```markdown
## Review: Round N - NO_ISSUES_FOUND

### Summary
Engineer's proposals reviewed. No issues identified.

### Completeness Checks
- [x] All gaps have concrete solutions
- [x] Examples provided
- [x] No obvious edge cases missing
- [x] Consistent with architecture

### Recommendation
Proposals ready for integration.
```

### 7.2 Engineer Has Nothing

If Engineer cannot address any gaps:
1. Check if all gaps blocked on user decisions
2. If blocked: Present decisions immediately
3. If not blocked: Retry with explicit gap assignment
4. After 2 retries: Mark as BLOCKED_EXTERNAL, notify user

### 7.3 Escalation Options

When max retries exhausted, present:
1. Skip round, proceed
2. Reassign gaps to different focus
3. Provide additional context (one more retry)
4. Narrow scope
5. Pause session

---

## 8. Conflict Resolution

Handle disagreements between Engineer and Reviewer.

### 8.1 Explicit Disagreement

Engineer format:
```markdown
## DISAGREE: ISSUE-R{N}-{XXX}

**Reviewer Concern:** [quote]
**Engineer Position:** [rationale]
**Alternative:** [middle ground if any]
**Request:** Escalate to user
```

### 8.2 Implicit Disagreement Detection

Heuristics (checked by Mediator):
1. HIGH/CRITICAL issue with no explicit reference in next round
2. Same gap rolled back 2+ times
3. Content changes don't overlap with concern keywords
4. Deflection pattern (attacks Reviewer without alternative)

When detected: Present to user as potential implicit conflict.

### 8.3 DISAGREE Rate Limiting

| Threshold | Action |
|-----------|--------|
| >50% per round | Warning to user |
| >5 absolute per round | Block, require justification |
| >40% over 3 rounds | Flag as systematic issue |

### 8.4 Conflict Presentation

Mediator generates options:
- **Option A:** Reviewer position (always)
- **Option B:** Engineer position (always)
- **Option C:** Synthesis if obvious middle ground
- **Option D:** User input (for CRITICAL only)

Minimum 2 options, maximum 4.

### 8.5 Resolution Recording

```markdown
## decisions.md

### Round N: ISSUE-R{N}-{XXX}
**Conflict:** [summary]
**Decision:** Option [X]
**Rationale:** [user's reason]
**Decided by:** User
**Date:** YYYY-MM-DD
```

Decisions feed back to Engineer in next round prompt.

---

## 9. Rollback Mechanism

Recover from unproductive rounds.

### 9.1 Scope

Rollback operates at round level:
- Archive (don't delete) round folder
- Restore status.md and decisions.md from backup
- Decrement round counter

### 9.2 Triggers

| Trigger | Initiated By |
|---------|--------------|
| User request | User |
| Validation failure after retries | Mediator |
| Severe divergence | Mediator |

### 9.3 Backup Retention

Keep backups for last 3 rounds:
- `status_backup_round_{N-2}.md`
- `status_backup_round_{N-1}.md`
- `status_backup_round_{N}.md`

### 9.4 Rollback Modes

| Mode | Behavior |
|------|----------|
| AUTO_RETRY | First rollback auto-adjusts, no prompt |
| INTERACTIVE | Always prompt user |
| STRICT | Rollback only on explicit user request |

Default: AUTO_RETRY

### 9.5 Limits

- Max 2 rollbacks per round (3 attempts total)
- Max 7 rollbacks per session
- After limits: Force proceed with best output

### 9.6 Root Cause Analysis

After 2 failures of same round, analyze:
- ABSTRACT_OUTPUT: Too vague, needs concrete examples
- MISSING_SECTIONS: Required format not followed
- SCOPE_DRIFT: Addressed wrong gaps
- DOMAIN_CONFUSION: Misunderstood problem space

Present analysis to user with recommended action.

---

## 10. Instance Roles

### 10.1 Engineer

**Role:** Spec implementer and detail resolver

**Context Requirements:**
- MUST explore relevant codebase before proposing solutions (use Glob, Grep, Read)
- MUST align proposals with existing patterns in the codebase
- MUST reference specific files/lines when proposing code changes
- SHOULD read `strategies/*.md` for product context and business rationale
- SHOULD check `CLAUDE.md` and `ARCHITECTURE.md` for system constraints

**Output Format:**
```markdown
## Gap Resolution: [GAP-ID]

**Confidence:** HIGH | MEDIUM | LOW

### Codebase Analysis
[What existing code was examined, key patterns found]

### Proposed Solution
[Detailed specification, aligned with codebase patterns]

### Examples
[Concrete code/YAML/SQL referencing actual file paths]

### Trade-offs
[Pros and cons]

### New Gaps Introduced
- [New gaps or "None"]
```

### 10.2 Reviewer

**Role:** Critical evaluator and gap finder

**Context Requirements:**
- MUST verify Engineer's proposals against actual codebase (use Glob, Grep, Read)
- MUST check for conflicts with existing implementations
- MUST validate that proposed patterns match codebase conventions
- SHOULD cross-reference `strategies/*.md` to ensure business alignment
- SHOULD verify architectural consistency with `ARCHITECTURE.md`

**Output Format:**
```markdown
## Review: [GAP-ID]

### Codebase Verification
[What was checked against actual code, any conflicts found]

### Critical Issues
- **[ISSUE-ID]**: [Description]
  - Location: [Exact text]
  - Codebase Reference: [file:line if applicable]
  - Impact: [What breaks]
  - Suggestion: [How to fix]

### High Priority
[Same format]

### Medium Priority
[Same format]

### Low Priority / Nits
[Same format]

### Compression Opportunities (optional)
- **COMPRESS-001**: [Redundancy description]
  - Locations: [file:line, file:line]
  - Suggested extraction: [target location]
```

When no issues found, use NO_ISSUES_FOUND format (Section 7.1).

**Note:** Compression opportunities are LOW severity and never block approval. See Section 14.6.

### 10.3 Mediator

**Role:** Orchestration, synthesis, user interface

**Context Requirements:**
- MUST read `strategies/*.md` to understand product vision before starting session
- MUST include relevant codebase paths in Engineer/Reviewer prompts
- SHOULD identify which codebase areas are relevant to each gap
- SHOULD provide file path hints to spawned instances

**Responsibilities:**
- Spawn Engineer/Reviewer via Task tool
- Validate outputs
- Update status.md
- Present decisions via AskUserQuestion
- Track convergence state
- Handle rollbacks
- Build dependency graph for parallel execution (Section 11.3)

### 10.4 Required Context Sources

All instances MUST ground their work in these sources:

| Source | Purpose | When to Read |
|--------|---------|--------------|
| `CLAUDE.md` | System architecture, coding standards | Session start |
| `ARCHITECTURE.md` | Detailed system design | When proposing structural changes |
| `strategies/*.md` | Product vision, market context, business rationale | Session start |
| `specs/*.md` | Related specifications | When gap touches other components |
| Codebase (`crates/`, `src/`) | Actual implementation patterns | Before every proposal/review |

**Grounding Protocol:**
1. **Before proposing:** Engineer MUST search codebase for similar patterns
2. **Before reviewing:** Reviewer MUST verify claims against actual code
3. **Before deciding:** Mediator MUST check if decision conflicts with strategy

**Anti-Pattern:** Proposing solutions that ignore existing codebase patterns or contradict product strategy.

### 10.5 Spec-Codebase Divergence

When the spec describes behavior that differs from the actual codebase, this divergence must be addressed before continuing with normal gap processing.

#### 10.5.1 Source of Truth Hierarchy

```
1. CODEBASE (highest authority)
   └── What the code actually does

2. SPEC (describes intent)
   └── What we want the code to do

3. EXCEPTIONS (spec takes precedence)
   └── Future roadmap specs (not yet implemented)
   └── Specs marked "PLANNED" or "PROPOSED"
   └── Architecture decision records (ADRs)
```

**Default Rule:** Code is truth. If spec says X but code does Y, the spec is wrong unless explicitly marked as future/planned.

#### 10.5.2 Divergence Types

| Type | Definition | Action |
|------|------------|--------|
| **SPEC_STALE** | Spec describes old behavior, code has moved on | Update spec to match code |
| **SPEC_ASPIRATIONAL** | Spec describes future state not yet built | Mark spec as PLANNED, continue |
| **CODE_BUGGY** | Code deviates from clear spec intent | File as implementation bug, update code |
| **AMBIGUOUS** | Unclear which is correct | Escalate to user |

#### 10.5.3 Divergence Detection (Pre-Round Scan)

Before starting normal gap processing, Mediator runs a divergence scan:

```markdown
## Divergence Report

### DIVERGE-001: State machine mismatch
- **Spec says:** Jobs view has 5 states (specs/views/jobs.md:45)
- **Code has:** 3 states (app.rs:820 JobsViewState enum)
- **Delta:** Missing LogViewer, FilterDialog states
- **Type:** SPEC_ASPIRATIONAL | SPEC_STALE | CODE_BUGGY | AMBIGUOUS
- **Recommendation:** [action]

### DIVERGE-002: ...
```

**Output Location:** `{session}/divergence_report.md` (see Section 12)

**Handling:** This report is overwritten in place each time the pre-round scan runs. No versioning is needed because it represents current session state, not a discrete round artifact.

**Regeneration Triggers:**
- Session starts
- User requests re-scan
- Scope changes (files added/removed)

#### 10.5.4 Divergence Resolution Priority

**Large divergences block normal processing.** Resolve in this order:

1. **AMBIGUOUS** - Ask user immediately (can't proceed without clarity)
2. **CODE_BUGGY** - Critical if spec is approved/locked
3. **SPEC_STALE** - Update spec before adding more gaps
4. **SPEC_ASPIRATIONAL** - Mark and continue (doesn't block)

**Threshold:** If >30% of spec sections have SPEC_STALE divergences, consider the spec a rewrite rather than refinement.

#### 10.5.5 Divergence in Gap Context

When Engineer/Reviewer find divergence during normal rounds:

```markdown
### Codebase Analysis

**DIVERGENCE FOUND:**
- Gap assumes: Schema has `file_id` field
- Code reality: Field doesn't exist (schema.sql:62)
- Impact: Proposed solution won't work
- Recommendation: Add DIVERGE-XXX to status.md, resolve before this gap
```

This transitions the gap to BLOCKED state (Section 2.4) with the divergence as the blocker. The divergence is added to status.md as DIVERGE-XXX and must be resolved before the gap can proceed.

#### 10.5.6 Exception Markers

Specs that describe future state should be clearly marked:

```markdown
# Feature X Specification

**Status:** PLANNED | APPROVED | IMPLEMENTED
**Codebase Alignment:** FUTURE (not yet implemented)

> ⚠️ This spec describes intended behavior. Implementation pending.
```

When `FUTURE` marker present, spec takes precedence over (missing) code.

---

## 11. Integration with Claude Code

### 11.1 Task Tool Spawning

```
Mediator (main context)
    │
    ├── Task(Engineer prompt) ──► engineer.md
    │
    ├── [Validate]
    │
    ├── Task(Reviewer prompt) ──► reviewer.md
    │
    ├── [Validate]
    │
    ├── [Synthesize summary.md]
    │
    ├── [AskUserQuestion if decisions needed]
    │
    └── [Update status.md, loop or terminate]
```

### 11.2 Context-Primed Delta Prompts

**Round 2+ Engineer:**
```
## Round {N} Delta

**What happened last round:**
- You proposed: [summary]
- Reviewer said: [key feedback]
- User decided: [decisions]

**Your task this round:**
Address [specific gap] incorporating feedback.
```

This is more token-efficient than re-reading full documents.

### 11.3 Parallel Gap Processing

> **Design Note:** This section extends Design Principle 7 (Single-Gap Focus). Parallel execution is permitted only when dependency analysis proves gaps are independent, preserving the core intent of clean, isolated resolution.

When multiple gaps are independent, process them in parallel to reduce total refinement time.

#### 11.3.1 Dependency Analysis

Before each round, Mediator builds a dependency graph:

```
GAP-A ──depends-on──► GAP-B     (A must wait for B)
GAP-C ◄──conflicts──► GAP-D     (C and D touch same code)
GAP-E                           (independent)
GAP-F                           (independent)
```

**Dependency Types:**

| Type | Definition | Example |
|------|------------|---------|
| `depends-on` | Gap A's solution requires Gap B's resolution | Data model gap blocks API gap |
| `conflicts` | Gaps touch overlapping code sections | Two UI gaps for same panel |
| `references` | Gap A mentions Gap B but doesn't block | Cross-reference for context |

#### 11.3.2 Parallel Execution Rules

1. **Independent gaps MAY run in parallel** - Spawn multiple Engineer tasks simultaneously
2. **Dependent gaps MUST run sequentially** - Wait for dependency to resolve
3. **Conflicting gaps SHOULD run sequentially** - Avoid merge conflicts
4. **Maximum parallelism: 4 gaps** - Balance speed vs. context coherence

#### 11.3.3 Parallel Round Structure

```
Mediator (main context)
    │
    ├── [Analyze dependencies]
    │
    ├── Task(Engineer GAP-A) ──┐
    ├── Task(Engineer GAP-B) ──┼──► [Run in parallel]
    ├── Task(Engineer GAP-C) ──┘
    │
    ├── [Wait for all, validate each]
    │
    ├── Task(Reviewer ALL) ──► reviewer.md (reviews all proposals)
    │
    ├── [Synthesize, detect conflicts between parallel proposals]
    │
    └── [Update status.md, loop or terminate]
```

#### 11.3.4 Conflict Detection Post-Parallel

After parallel Engineer runs, check for:

| Conflict Type | Detection | Resolution |
|---------------|-----------|------------|
| **Overlapping edits** | Same file:line referenced | Mediator merges or asks user |
| **Contradictory patterns** | Proposal A uses X, Proposal B uses Y | Standardize in next round |
| **Duplicate work** | Both proposals add same abstraction | Keep one, mark other resolved |

#### 11.3.5 Parallel Reviewer Strategy

Two approaches (Mediator chooses based on gap count):

**Approach A: Single Reviewer (≤4 gaps)**
- One Reviewer task reviews all parallel proposals
- More coherent, catches cross-proposal issues
- Longer single task

**Approach B: Parallel Reviewers (>4 gaps)**
- Multiple Reviewer tasks, one per proposal
- Faster, but may miss cross-proposal conflicts
- Requires Mediator to synthesize reviews

#### 11.3.6 Status Tracking for Parallel Rounds

```markdown
## status.md

### Round 3 (Parallel)

| Gap ID | Worker | State | Dependencies |
|--------|--------|-------|--------------|
| GAP-STORE-001 | Engineer-1 | PROPOSED | None |
| GAP-DERIVE-001 | Engineer-2 | PROPOSED | None |
| GAP-GC-001 | Engineer-3 | IN_PROGRESS | GAP-STORE-001 (waiting) |
| GAP-CLI-001 | - | BLOCKED | GAP-DERIVE-001 |

**Parallelism:** 2 of 4 gaps run in parallel
**Blocked:** 2 gaps waiting on dependencies
```

---

## 12. Document Structure

```
specs/meta/
├── spec_refinement_workflow.md    # THIS FILE
├── sessions/
│   └── {spec_name}/
│       ├── source.md              # Original partial spec
│       ├── status.md              # Gap inventory, progress
│       ├── decisions.md           # User decisions
│       ├── compression_candidates.md  # Cross-spec redundancy tracking (Section 14.5)
│       ├── divergence_report.md   # Pre-round divergence scan (Section 10.5.3)
│       ├── round_001/
│       │   ├── engineer.md
│       │   ├── reviewer.md
│       │   └── summary.md
│       ├── round_002/
│       │   └── ...
│       └── status_backup_round_*.md
```

---

## 13. Validated Patterns

From self-refinement (10 rounds):

| Pattern | Evidence |
|---------|----------|
| Foundations-first | Round 2 defined lifecycle/severity, unblocked Rounds 3-10 |
| Single-gap focus (default) | Self-refinement used 1 gap/round; parallel available for independent gaps (Section 11.3) |
| Context-primed deltas | Minimal context, quality output |
| Separate instances | True isolation via Task tool |
| AskUserQuestion | Interactive decisions, no manual file editing |

### Convergence Graph (Self-Refinement)

```
Round:  1    2    3    4    5    6    7    8    9   10
Net:   -3   +2   +1   +1    0    0    0    0   +1   +1
State: STL  CNV  CNV  CNV  FLT  FLT  FLT  FLT  CNV  CNV
```

---

## 14. Semantic Compression

Semantic compression reduces redundancy across specs by extracting common patterns—but only when justified.

### 14.1 The 2+ Instance Rule

**Never extract a pattern from a single instance.**

```
❌ WRONG: "This config block looks generalizable, let me create a template"
✅ RIGHT: "I've seen this exact pattern in 3 specs, extracting now"
```

Why? Premature abstraction creates complexity without evidence of reuse. Wait until you see the same concept appear **at least twice** with meaningful similarity.

### 14.2 Concrete Before Abstract

During refinement, prefer concrete examples over abstract descriptions:

| Phase | Approach |
|-------|----------|
| Round 1-3 | Write concrete, specific solutions. Copy-paste is fine. |
| Round 4+ | If patterns repeat, consider extraction |
| Final pass | Compress only what's truly duplicated |

"Make your spec usable before you try to make it reusable."

### 14.3 Compression Triggers

When to compress (all conditions must be met):

1. **2+ instances** - Same concept appears in multiple places
2. **Meaningful similarity** - Not just superficial resemblance
3. **Stable pattern** - Instances aren't still changing
4. **Clear benefit** - Compression reduces complexity, not just lines

### 14.4 Compression Operations

| Operation | When to Use | Example |
|-----------|-------------|---------|
| **Extract constant** | Same literal value in 2+ places | `MAX_RETRIES = 3` |
| **Extract pattern** | Same structure with variations | YAML templates |
| **Extract section** | Same prose in 2+ specs | Common keybindings table |
| **Cross-reference** | Same concept defined elsewhere | "See specs/tui.md for layout patterns" |

### 14.5 Cross-Spec Compression

During multi-spec refinement sessions, track potential compression targets:

```markdown
## compression_candidates.md

### Candidate: Dialog Layout Pattern
**Instances:**
- specs/views/sources.md:89 - Add source dialog
- archive/specs/views/extraction_rules.md:102 - Create rule dialog
- specs/views/jobs.md:76 - Retry confirmation dialog

**Similarity:** All use same 3-row layout (header, content, buttons)
**Action:** Extract to specs/tui.md Section 4.2 (Dialogs)
**Status:** EXTRACTED | PENDING | REJECTED
```

### 14.6 Reviewer Compression Check

Reviewer should flag during review:

```markdown
### Compression Opportunities
- **COMPRESS-001**: [Description of redundancy]
  - Locations: [list]
  - Suggested extraction: [where to put common definition]
  - Severity: LOW (compression is never blocking)
```

### 14.7 Anti-Patterns

| Anti-Pattern | Why It's Wrong |
|--------------|----------------|
| **Premature generalization** | Abstracting from n=1 creates speculative complexity |
| **DRY absolutism** | Some duplication is clearer than indirection |
| **Template sprawl** | Too many small templates increase cognitive load |
| **Cross-reference maze** | Reader shouldn't need 5 files to understand one concept |

### 14.8 Bottom-Up Architecture Emergence

Let spec structure emerge from concrete content:

```
Round 1: Write specific solutions (messy, duplicated)
Round 2: Notice patterns ("these 3 views have same layout")
Round 3: Extract pattern ("all views use 3-pane layout")
Round 4: Document pattern in master spec
```

Don't pre-design the master spec structure. Let it crystallize from actual specs.

---

## 15. State Machine Requirement

Specifications describing user flows, views, or interactive components **MUST** include state machine documentation.

### 15.1 When Required

| Spec Type | State Machine Required? | Example |
|-----------|------------------------|---------|
| TUI View/Mode | YES | Discover, Parser Bench, Jobs |
| User Workflow | YES | Glob Explorer, Rule Creation |
| CLI Command | Only if multi-step | `casparian backfill` with stages |
| Data Model | NO | Schema definitions |
| API Endpoint | Only if stateful | WebSocket sessions |

### 15.2 State Machine Format

**Required Elements:**

```markdown
## State Machine

### N.1 State Diagram
[ASCII art showing states and transitions]

### N.2 State Definitions
| State | Entry | Exit | Behavior |
|-------|-------|------|----------|
| StateA | How to enter | How to exit | What user can do |

### N.3 Transitions
| From | To | Trigger | Guard (optional) |
|------|----|---------| ----------------|
| StateA | StateB | Key/Event | Condition if any |
```

**Example (minimal):**

```
┌─────────────┐     Enter      ┌─────────────┐
│   BROWSE    │───────────────►│   DETAIL    │
│  (default)  │◄───────────────│   (modal)   │
└─────────────┘     Esc        └─────────────┘
```

### 15.3 Validation Rules

| Rule | Check |
|------|-------|
| REACHABILITY | All states reachable from initial state |
| ESCAPABILITY | Every non-terminal state has exit path |
| DETERMINISM | Same trigger never goes to multiple states |
| COMPLETENESS | All keybindings mapped to transitions |
| CONSISTENCY | Esc always means "go back" or "cancel" |

### 15.4 Gap Categories for State Machines

| Gap ID Pattern | Description |
|----------------|-------------|
| `GAP-STATE-XXX` | Missing state definition |
| `GAP-TRANS-XXX` | Missing or ambiguous transition |
| `GAP-REACH-XXX` | Unreachable state detected |
| `GAP-ESCAPE-XXX` | No escape from state |
| `GAP-CONFLICT-XXX` | Key triggers multiple transitions |

### 15.5 Engineer State Machine Checklist

When proposing state machine additions:
- [ ] Diagram included with all states
- [ ] Entry/exit conditions documented for each state
- [ ] All keybindings appear in transition table
- [ ] Esc behavior is consistent
- [ ] No orphan states

### 15.6 Reviewer State Machine Checklist

When reviewing state machines:
- [ ] Can trace path from initial state to every other state
- [ ] Can trace path from every state back to initial (or exit)
- [ ] No two transitions from same state triggered by same key
- [ ] Terminal states are explicitly marked
- [ ] State names are semantic (not just "State1")

---

## 16. Cross-Cutting Review (Optional Extension)

When refining multiple related specs (e.g., TUI views, API endpoints, data models), per-file rounds may miss integration issues. This optional extension adds a cross-cutting review phase.

### 16.1 When to Use

Trigger cross-cutting review when:
- 3+ related specs have been refined in the same session
- Specs reference each other (navigation, data flow, shared patterns)
- Session gap categories include REF, SCOPE, or COMP
- Per-file rounds are complete (all STUB gaps resolved)

### 16.2 What It Validates

| Dimension | Questions |
|-----------|-----------|
| **Navigation Flow** | Can users reach all views? Are entry/exit paths consistent? Do breadcrumbs match hierarchy? |
| **Data Flow** | When View A links to View B, does B accept that context? Are IDs/parameters passed correctly? |
| **Pattern Consistency** | Are similar interactions (dialogs, confirmations, test modes) implemented the same way? |
| **State Handoff** | Does `on_enter()` handle context from all callers? Does `on_leave()` preserve expected state? |
| **Keybinding Coherence** | Same key = same action across views? Conflicts with global bindings? |

### 16.3 Cross-Cutting Round Structure

**Phase 1: Navigation Graph**
Engineer produces a navigation diagram:

```markdown
## Navigation Graph

```
Home (0, H)
├── [1] Discover
│   ├── [e] → Extraction Rules (child)
│   └── [Enter on file] → File Details (modal)
├── [2] Parser Bench
│   └── [Enter on parser] → Parser Editor (child)
├── [3] Jobs
│   └── [Enter on job] → Job Details (panel)
└── [4] Sources
    └── [c] → Class Manager (dialog)
```

### Entry/Exit Matrix
| View | Entry From | Entry Context | Exit To | Exit Context |
|------|------------|---------------|---------|--------------|
| Extraction | Discover | source_id? | Discover | selected_rule? |
```

**Phase 2: Data Flow Validation**
For each navigation edge, verify:
1. Caller passes expected context
2. Callee's `on_enter()` handles that context
3. Data types match

**Phase 3: Pattern Audit**
Identify patterns that appear 2+ times:
```markdown
## Patterns Found

### Dialog Pattern (4 instances)
- home.md: ScanDialog, TestDialog
- sources.md: AddSourceDialog, ClassManager
- extraction.md: WizardDialog, DeleteConfirm, ConflictDialog
- jobs.md: RetryConfirm

**Consistency Check:**
- ✓ All use Esc to cancel
- ✓ All use Enter to confirm
- ✗ Some use Tab for fields, others use arrow keys
```

### 16.4 Cross-Cutting Reviewer Output

```markdown
## Cross-Cutting Review

### Navigation Issues
- **XCUT-NAV-001**: home.md shows `[5] Extraction` but extraction.md says drill-down from Discover
  - Impact: User confusion, navigation mismatch
  - Fix: Align on single navigation model

### Data Flow Issues
- **XCUT-DATA-001**: jobs.md passes `parser_name` but parser_bench.md expects `parser_id`
  - Impact: Navigation will fail
  - Fix: Standardize on parser_id

### Pattern Inconsistencies
- **XCUT-PAT-001**: Dialog focus handling varies
  - extraction.md: Tab cycles fields
  - sources.md: Arrow keys navigate list
  - Fix: Document pattern in tui.md Section 4.2

### Compression Opportunities
[From per-file reviews, now with 2+ instances confirmed]
```

### 16.5 Cross-Cutting Gap Format

```
XCUT-{TYPE}-{NUMBER}
```

Types:
- NAV: Navigation/routing issues
- DATA: Data flow/context issues
- PAT: Pattern inconsistency
- KEY: Keybinding conflicts

### 16.6 Integration with Main Workflow

Cross-cutting review runs as a special round:

```
Rounds 1-N: Per-file refinement (normal)
Round N+1: Cross-cutting review (this extension)
Rounds N+2+: Fix cross-cutting issues (normal rounds)
```

The Mediator spawns a single Engineer task that reads ALL specs simultaneously, then a Reviewer that validates the integration.

### 16.7 When to Skip

Skip cross-cutting review if:
- Only 1-2 specs refined
- Specs are independent (no cross-references)
- All REF/SCOPE gaps already resolved in per-file rounds
- User explicitly declines ("specs are self-contained")

---

## 17. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-09 | 1.0 | Initial specification |
| 2026-01-09 | 1.1 | Task tool spawning, AskUserQuestion |
| 2026-01-12 | 2.0 | Self-refined: Gap lifecycle, severity, stall detection, termination, error recovery, partial rounds, conflict resolution, rollback, example attachment, implicit disagreement |
| 2026-01-12 | 2.1 | Semantic compression: 2+ instance rule, concrete-before-abstract, cross-spec compression tracking, bottom-up architecture emergence |
| 2026-01-12 | 2.2 | Cross-cutting review: optional extension for multi-spec sessions, validates navigation flow, data handoff, pattern consistency across related specs |
| 2026-01-13 | 2.3 | **State machine requirement (Section 15)**: Specs with user flows/views MUST include state machines. Validation rules, gap categories, Engineer/Reviewer checklists |
| 2026-01-14 | 2.4 | **Parallel gap processing (Section 11.3)**: Independent gaps run in parallel via multiple Task spawns. Dependency analysis, conflict detection, parallel reviewer strategies. **Codebase grounding (Section 10.4)**: All contributors MUST verify against actual codebase and product strategy. Context requirements added to Engineer/Reviewer/Mediator roles. **Spec-codebase divergence (Section 10.5)**: Code is source of truth (with exceptions for future/planned specs). Divergence detection in Round 0, resolution priority, blocking rules for large divergences. |
| 2026-01-14 | 2.5 | **Self-refinement of v2.4 additions**: Resolved internal contradictions. (1) GAP-LIFECYCLE-001: Added BLOCKED state to Section 2 gap lifecycle with blocker types, format, and force-unblock. (2) GAP-PRINCIPLE-001: Reframed "Single-Gap Focus" as default with parallel extension reference. (3) GAP-ROUND0-001: Added divergence_report.md to document structure, renamed "Round 0" to "Pre-Round Scan". |

---

## Appendix A: Gap ID Format

```
GAP-{CATEGORY}-{NUMBER}
```

- CATEGORY: 2-10 uppercase letters (e.g., FLOW, ROLE, COMM)
- NUMBER: 3 digits, zero-padded (e.g., 001, 042)

Regex: `GAP-[A-Z]{2,10}-\d{3}`

## Appendix B: Timestamp Format

ISO 8601: `YYYY-MM-DDTHH:MM:SSZ`

Example: `2026-01-12T14:30:00Z`
