# Automated Spec Refinement Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 1.1
**Purpose:** Multi-instance Claude system for iterative specification refinement

---

## 1. Overview

This document defines a **3-instance Claude workflow** for refining partial specifications into complete, implementable specs. The system uses structured markdown documents for inter-instance communication.

### 1.1 Design Principles

1. **Separation of Concerns** - Each instance has a distinct role
2. **True Isolation** - Instances run as separate processes (via Task tool), sharing only documents
3. **User as Final Authority** - User approves all significant decisions via interactive prompts
4. **Transparent Progress** - All gaps, issues, and decisions are visible
5. **Convergent Refinement** - Each round should reduce total gaps

### 1.2 Implementation Architecture

**Instance Spawning:** Engineer and Reviewer run as separate Task tool invocations. This ensures:
- No context bleed between roles
- Reviewer sees only Engineer's output, not reasoning
- Each instance operates on explicit document inputs only

**User Interaction:** Mediator uses `AskUserQuestion` tool instead of requiring manual file edits. Benefits:
- Interactive, immediate feedback
- Structured options with descriptions
- Decisions recorded automatically to `decisions.md`

**Round Limits:**
- Hard limit (10 rounds) only in fully automated mode (no human questions)
- When human is in the loop, user controls termination
- Divergence warning after 2 consecutive rounds with negative progress

**Async Support:** Workflow is designed for sync (complete in one session) but state persists in markdown files, enabling async resumption if needed.

---

## 2. Instance Roles

### 2.1 Engineer Instance

**Role:** Spec implementer and detail resolver

**Responsibilities:**
- Read partial specs and gap lists
- Propose concrete solutions for identified gaps
- Write detailed specifications with examples
- Apply data-oriented design principles
- Generate implementation-ready content

**Persona Prompt:**
```
You are a Staff Engineer specializing in data-oriented design. Your role is to
transform partial specifications into complete, implementable specs. You:

- Prefer concrete examples over abstract descriptions
- Design for the common case, handle edge cases explicitly
- Avoid premature abstraction
- Consider performance and resource constraints
- Write specs that a mid-level engineer could implement

When addressing gaps:
1. State the gap being addressed
2. Propose a specific solution
3. Provide concrete examples
4. Note any new gaps your solution introduces
5. Mark confidence level (HIGH/MEDIUM/LOW)
```

**Output Format:**
```markdown
## Gap Resolution: [GAP-ID]

**Confidence:** HIGH | MEDIUM | LOW

### Proposed Solution
[Detailed specification text]

### Examples
[Concrete code/YAML/SQL examples]

### Trade-offs
[Pros and cons of this approach]

### New Gaps Introduced
- [Any new gaps this creates, or "None"]
```

---

### 2.2 Reviewer Instance

**Role:** Critical evaluator and gap finder

**Responsibilities:**
- Review Engineer's proposals for completeness
- Identify handwaving, ambiguity, and missing details
- Find edge cases and failure modes
- Check consistency with existing specs
- Evaluate higher-order consequences

**Persona Prompt:**
```
You are a Principal Engineer known for thorough code reviews. Your role is to
find gaps, issues, and inconsistencies in specifications. You:

- Question every assumption
- Find edge cases the author didn't consider
- Check for consistency with existing architecture
- Identify performance and scalability concerns
- Look for security and data integrity issues
- Consider migration and backwards compatibility

Your review should be:
- Specific (cite exact text being questioned)
- Constructive (explain WHY something is a problem)
- Prioritized (Critical > High > Medium > Low)

Never rubber-stamp. If a proposal seems complete, look harder.
```

**Output Format:**
```markdown
## Review: [Section/Gap ID]

### Critical Issues
- **[ISSUE-ID]**: [Description]
  - Location: [Exact text or section]
  - Impact: [What breaks if not addressed]
  - Suggestion: [How to fix, if known]

### High Priority
[Same format]

### Medium Priority
[Same format]

### Low Priority / Nits
[Same format]

### Consistency Checks
- [ ] Consistent with [related spec]?
- [ ] Migration path from current state?
- [ ] Performance implications addressed?
```

---

### 2.3 Mediator Instance

**Role:** Synthesis and user interface

**Responsibilities:**
- Summarize Engineer + Reviewer outputs
- Identify decision points requiring user input
- Present options with trade-offs
- Track overall progress toward completion
- Prepare user-facing status reports

**Persona Prompt:**
```
You are a Technical Program Manager facilitating spec refinement. Your role is to:

- Synthesize Engineer proposals and Reviewer feedback
- Identify where user decisions are needed
- Present clear options with trade-offs
- Track progress toward spec completion
- Ensure the process converges (gaps decrease each round)

Your outputs should be:
- Concise (users are busy)
- Actionable (clear next steps)
- Balanced (present multiple perspectives fairly)
- Progress-focused (show gap count trending down)

Never make decisions for the user. Present options and defer.
```

**Output Format:**
```markdown
## Round [N] Summary

### Progress
- Gaps Resolved: X
- New Gaps Introduced: Y
- Net Progress: X - Y
- Remaining Gaps: Z

### Decisions Needed

#### Decision 1: [Title]
**Context:** [Brief background]

| Option | Pros | Cons |
|--------|------|------|
| A: [Description] | [Benefits] | [Drawbacks] |
| B: [Description] | [Benefits] | [Drawbacks] |

**Engineer Recommends:** [Option]
**Reviewer Concerns:** [Summary]

---

### Resolved This Round
- GAP-001: [One-line summary of resolution]
- GAP-002: [One-line summary of resolution]

### Blocked
- GAP-003: Waiting on Decision 1
- GAP-004: Requires external input

### Next Round Focus
[What Engineer should work on next]
```

---

## 3. Document Structure

### 3.1 Shared Documents

```
specs/meta/
├── spec_refinement_workflow.md    # THIS FILE (read-only reference)
├── sessions/
│   └── pathspec/                  # One folder per spec being refined
│       ├── round_001/
│       │   ├── engineer.md        # Engineer's proposals
│       │   ├── reviewer.md        # Reviewer's feedback
│       │   └── summary.md         # Mediator's synthesis
│       ├── round_002/
│       │   └── ...
│       ├── decisions.md           # User's decisions (cumulative)
│       └── status.md              # Current gap counts, blocking items
```

### 3.2 Read/Write Permissions

| Instance | Own Doc | Other Docs | decisions.md | status.md |
|----------|---------|------------|--------------|-----------|
| Engineer | WRITE | READ | READ | READ |
| Reviewer | WRITE | READ | READ | READ |
| Mediator | WRITE | READ | READ | WRITE |
| User | - | READ | WRITE | READ |

**Key constraints:**
- Instances only write to their own document
- Instances read all documents from previous rounds
- User writes to `decisions.md` only
- Mediator maintains `status.md`

---

## 4. Process Flow

### 4.1 Round Structure

```
┌─────────────────────────────────────────────────────────────────────┐
│                         ROUND N FLOW                                │
│                                                                     │
│  ┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐   │
│  │ Engineer │ ──► │ Reviewer │ ──► │ Mediator │ ──► │   User   │   │
│  │          │     │          │     │          │     │          │   │
│  │ Propose  │     │ Critique │     │ Summarize│     │ Decide   │   │
│  │ Solutions│     │ Find Gaps│     │ Present  │     │ Approve  │   │
│  └──────────┘     └──────────┘     └──────────┘     └──────────┘   │
│       │                                                   │         │
│       └───────────────── ROUND N+1 ◄─────────────────────┘         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 4.2 Step-by-Step Process

**Step 1: Engineer Phase**
1. Read `decisions.md` for user choices from previous round
2. Read previous `reviewer.md` for issues to address
3. Read `status.md` for current gap list
4. Write `round_N/engineer.md` with proposals

**Step 2: Reviewer Phase**
1. Read `round_N/engineer.md`
2. Read original partial spec for context
3. Read `decisions.md` for constraints
4. Write `round_N/reviewer.md` with critiques

**Step 3: Mediator Phase**
1. Read `round_N/engineer.md` and `round_N/reviewer.md`
2. Synthesize into `round_N/summary.md`
3. Update `status.md` with current gap counts
4. Identify decisions needed from user

**Step 4: User Phase**
1. Read `round_N/summary.md`
2. Make decisions, append to `decisions.md`
3. Optionally provide additional context
4. Signal ready for next round

### 4.3 Convergence Criteria

The process completes when:
1. **Zero Critical/High gaps** remaining
2. **All Open Questions resolved** by user
3. **Reviewer signs off** with no new issues found
4. **User approves** final spec

**Divergence Detection:**
If `New Gaps Introduced > Gaps Resolved` for 2 consecutive rounds:
- Mediator flags potential scope creep
- User decides: narrow scope, accept complexity, or pause

---

## 5. Initial Setup

### 5.1 Starting a Refinement Session

1. Create session folder: `specs/meta/sessions/{spec_name}/`

2. Copy partial spec to session:
   ```bash
   cp specs/pathspec_partial.md specs/meta/sessions/pathspec/source.md
   ```

3. Initialize `status.md`:
   ```markdown
   # PathSpec Refinement Status

   **Started:** 2026-01-09
   **Source:** pathspec_partial.md

   ## Gap Inventory (from source)
   - [ ] GAP-GRAMMAR-001: Optional propagation
   - [ ] GAP-GRAMMAR-002: OneOf ambiguity
   ... (copy all gaps from partial spec)

   ## Open Questions
   - [ ] OQ-001: PathSpec per-source or global?
   ... (copy all OQs)

   ## Round Progress
   | Round | Gaps In | Resolved | New | Gaps Out |
   |-------|---------|----------|-----|----------|
   ```

4. Initialize `decisions.md`:
   ```markdown
   # User Decisions

   ## Round 0 (Initial)
   - Priority order for gaps: [user specifies]
   - Any constraints: [user specifies]
   ```

5. Run first Engineer phase

---

## 6. Prompt Templates

### 6.1 Engineer Prompt (Round Start)

```
You are the Engineer instance in a spec refinement workflow.

## Context
- Spec being refined: {spec_name}
- Current round: {round_number}
- Previous round summary: [attached]
- User decisions: [attached]
- Current status: [attached]

## Your Task
Address the following gaps in priority order:
1. {gap_id_1}: {gap_description}
2. {gap_id_2}: {gap_description}
...

Write your proposals to `round_{N}/engineer.md` following the output format
specified in the workflow document.

Focus on:
- Gaps marked as blocking
- Issues raised by Reviewer in previous round
- Any user decisions that unlock progress
```

### 6.2 Reviewer Prompt (After Engineer)

```
You are the Reviewer instance in a spec refinement workflow.

## Context
- Spec being refined: {spec_name}
- Current round: {round_number}
- Engineer proposals: [attached]
- Original partial spec: [attached]
- Related specs: {list}

## Your Task
Review the Engineer's proposals in `round_{N}/engineer.md`.

For each proposal:
1. Check completeness (all cases covered?)
2. Check consistency (matches existing architecture?)
3. Find edge cases (what breaks this?)
4. Assess confidence (is HIGH confidence justified?)

Write your review to `round_{N}/reviewer.md` following the output format.

Be thorough. If you find no issues, look harder.
```

### 6.3 Mediator Prompt (After Reviewer)

```
You are the Mediator instance in a spec refinement workflow.

## Context
- Spec being refined: {spec_name}
- Current round: {round_number}
- Engineer proposals: [attached]
- Reviewer feedback: [attached]
- Previous status: [attached]

## Your Task
1. Synthesize Engineer + Reviewer into `round_{N}/summary.md`
2. Update `status.md` with gap counts
3. Identify decisions requiring user input
4. Check for convergence/divergence

Present options fairly. Do not make decisions for the user.
```

---

## 7. Example Session

### 7.1 Round 1 Artifacts

**engineer.md** (excerpt):
```markdown
## Gap Resolution: GAP-GRAMMAR-001 (Optional Propagation)

**Confidence:** MEDIUM

### Proposed Solution
When a `Static` node has `optional: true`, its children are conditionally
evaluated only if the node exists in the physical path.

Semantics:
- If optional parent missing → children not evaluated, no anomalies
- If optional parent present → children evaluated normally
- Optional does not cascade (children can be mandatory within optional parent)

### Examples
```yaml
- name: archive           # optional: true
  optional: true
  children:
    - name: 2024          # mandatory within archive/
      children: ...
```

Physical: `/data/` (no archive folder)
Result: No anomaly. Archive is optional.

Physical: `/data/archive/` (archive exists, no 2024)
Result: Anomaly! 2024 is mandatory when archive exists.

### Trade-offs
- Pro: Clear semantics, easy to implement
- Con: Cannot express "if archive exists, everything below is optional"

### New Gaps Introduced
- GAP-GRAMMAR-006: Need `optional_cascade: true` for deep optional trees
```

**reviewer.md** (excerpt):
```markdown
## Review: GAP-GRAMMAR-001 Resolution

### High Priority
- **ISSUE-R1-001**: Cascade behavior insufficient
  - Location: "Optional does not cascade"
  - Impact: Mission folders with variable depth can't be expressed
  - Suggestion: Add `optional_cascade` or `optional_depth: N`

### Medium Priority
- **ISSUE-R1-002**: What if optional node matches but fails type validation?
  - Location: Not addressed
  - Impact: Ambiguous whether this is "node missing" or "node malformed"
```

**summary.md** (excerpt):
```markdown
## Round 1 Summary

### Progress
- Gaps Resolved: 1 (GAP-GRAMMAR-001 partially)
- New Gaps Introduced: 1 (GAP-GRAMMAR-006)
- Net Progress: 0
- Remaining Gaps: 18

### Decisions Needed

#### Decision 1: Optional Cascade Behavior
**Context:** Optional folders may contain deeply nested optional content.

| Option | Pros | Cons |
|--------|------|------|
| A: No cascade (current) | Simple, explicit | Verbose for deep trees |
| B: Add `optional_cascade` | Flexible | More complex grammar |
| C: `optional_depth: N` | Bounded flexibility | Magic numbers |

**Engineer Recommends:** A (simplicity)
**Reviewer Concerns:** Insufficient for real-world mission data structures
```

---

## 8. Integration with Claude Code

### 8.1 Task Tool Spawning

Each instance runs as a separate Task tool invocation with `subagent_type: "general-purpose"`:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    MEDIATOR (Main Context)                          │
│                                                                     │
│  1. Spawn Engineer ──► Task(prompt: engineer_prompt)                │
│                              │                                      │
│                              ▼                                      │
│                        engineer.md written                          │
│                              │                                      │
│  2. Spawn Reviewer ──► Task(prompt: reviewer_prompt)                │
│                              │                                      │
│                              ▼                                      │
│                        reviewer.md written                          │
│                              │                                      │
│  3. Read both, synthesize, AskUserQuestion                          │
│                              │                                      │
│                              ▼                                      │
│                        decisions.md updated                         │
│                              │                                      │
│  4. Loop or terminate                                               │
└─────────────────────────────────────────────────────────────────────┘
```

**Engineer Task Prompt Template:**
```
You are the Engineer instance. Read these files:
- specs/meta/sessions/{name}/source.md (original spec)
- specs/meta/sessions/{name}/status.md (gap list)
- specs/meta/sessions/{name}/decisions.md (user decisions)

Address gaps in priority order. Write output to:
specs/meta/sessions/{name}/round_{N}/engineer.md

Follow the output format in the workflow spec.
```

**Reviewer Task Prompt Template:**
```
You are the Reviewer instance. Read these files:
- specs/meta/sessions/{name}/source.md (original spec)
- specs/meta/sessions/{name}/round_{N}/engineer.md (proposals to review)
- specs/meta/sessions/{name}/decisions.md (constraints)

Find gaps, issues, edge cases. Write output to:
specs/meta/sessions/{name}/round_{N}/reviewer.md

Follow the output format in the workflow spec. Be thorough.
```

### 8.2 User Decisions via AskUserQuestion

Instead of manual file editing, Mediator presents decisions interactively:

```python
# Mediator synthesizes, then asks
AskUserQuestion(questions=[
    {
        "question": "How should optional cascade work?",
        "header": "Cascade",
        "options": [
            {"label": "No cascade", "description": "Explicit per-node"},
            {"label": "Add optional_cascade", "description": "Boolean flag"},
            {"label": "optional_depth: N", "description": "Numeric depth"}
        ]
    }
])
```

Responses are automatically recorded to `decisions.md` by Mediator.

### 8.3 Parallel Sessions

For independent specs, multiple refinement sessions can run in parallel:
- `sessions/pathspec/` - PathSpec refinement
- `sessions/extractors/` - Extractor refinement
- `sessions/logical_view/` - Logical View refinement

Cross-cutting issues flagged by Mediator when specs interact.

---

## 9. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-09 | 1.0 | Initial workflow specification |
| 2026-01-09 | 1.1 | Added: Task tool spawning, AskUserQuestion pattern, round limits, async support |
