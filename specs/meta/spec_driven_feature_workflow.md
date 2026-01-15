# Spec-Driven Feature Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 1.0
**Category:** Implementation workflow (per workflow_manager.md Section 3.3.2)
**Purpose:** Orchestrate spec-first feature development from proposal through validated implementation
**Related:** feature_workflow.md, spec_refinement_workflow.md, spec_maintenance_workflow.md

---

## 1. Overview

This workflow formalizes the pattern of developing features by first updating specifications, refining them through structured review, and then implementing the validated changes. It orchestrates multiple sub-workflows to achieve "right first time" implementation.

### 1.1 When to Use This Workflow

| Trigger | Example |
|---------|---------|
| Feature involves spec changes | "Consolidate AI wizards into Rule Builder" |
| UI/UX redesign | "Change Discover view state machine" |
| Architectural change | "Refactor parser execution model" |
| Complex feature (>5 files) | "Add plugin marketplace" |
| User explicitly requests | "workflow: spec-driven feature for X" |

### 1.2 When NOT to Use

| Situation | Use Instead |
|-----------|-------------|
| Simple bug fix | `feature_workflow` |
| Code-only change (no spec) | `feature_workflow` |
| Spec review only | `spec_refinement_workflow` |
| Corpus audit | `spec_maintenance_workflow` |

### 1.3 Design Principles

1. **Spec is Source of Truth** - Implementation follows spec, not the other way around
2. **Validate Before Implement** - Catch design issues in spec phase, not code phase
3. **Diff-Based Implementation** - Implement changes, not the entire feature
4. **Parallel Validation** - Run independent validations concurrently
5. **Fail Fast** - Stop early if spec refinement doesn't converge

### 1.4 Instance Model

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    SPEC-DRIVEN FEATURE WORKFLOW                          │
│                        (1 orchestrator instance)                         │
│                                                                          │
│  Phase 1   Phase 2   Phase 3          Phase 4      Phase 5    Phase 6   │
│  ┌─────┐   ┌─────┐   ┌─────────────┐  ┌────────┐   ┌───────┐  ┌──────┐  │
│  │Prop.│──▶│Spec │──▶│Refinement   │──▶│Corpus  │──▶│Implmt.│──▶│Valid.│  │
│  │Capt.│   │Updt.│   │(sub-workflow)│  │Align.  │   │       │  │Suite │  │
│  └─────┘   └─────┘   └─────────────┘  └────────┘   └───────┘  └──────┘  │
│                             │                           │         │      │
│                             ▼                           ▼         ▼      │
│                      ┌─────────────┐            ┌─────────┐ ┌─────────┐  │
│                      │spec_refine  │            │code edits│ │tui_test │  │
│                      │ _workflow   │            │per diff  │ │memory   │  │
│                      │(3-instance) │            └─────────┘ │audit    │  │
│                      └─────────────┘                        └─────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Phase Definitions

### Phase 1: Proposal Capture

**Goal:** Understand what the user wants to change and why.

**Inputs:**
- User's feature request/proposal
- Relevant context (files mentioned, prior conversation)

**Process:**
```
1. EXTRACT core intent from user request
2. IDENTIFY affected specs:
   - TUI change → specs/views/*.md
   - Architecture change → CLAUDE.md, ARCHITECTURE.md
   - Workflow change → specs/meta/*_workflow.md
   - Feature → specs/*.md
3. CAPTURE "before" state:
   - Read current spec content
   - Note current version numbers
   - Record key sections that will change
4. CLARIFY ambiguities with user (AskUserQuestion)
5. OUTPUT: ProposalSummary document
```

**ProposalSummary Schema:**
```markdown
## Proposal Summary

**Request:** {user's request, paraphrased}
**Intent:** {what this achieves}
**Affected Specs:**
- {spec_file_1} - Section X, Y
- {spec_file_2} - Section Z

**Key Changes:**
1. {change_1}
2. {change_2}

**Open Questions:** (if any, ask user before proceeding)
```

**Exit Criteria:**
- User confirms proposal summary is accurate
- All affected specs identified
- No blocking ambiguities

---

### Phase 2: Spec Update

**Goal:** Update the specification file(s) to reflect the proposed changes.

**Inputs:**
- ProposalSummary from Phase 1
- Current spec file(s)

**Process:**
```
1. FOR each affected spec:
   a. Read current content
   b. Identify sections to modify
   c. Draft changes following spec conventions:
      - Maintain existing structure
      - Update version number
      - Add revision history entry
   d. Apply edits
   e. Record diff for Phase 5

2. CREATE session directory:
   specs/meta/sessions/{feature_name}_{date}/
   - proposal.md (from Phase 1)
   - spec_diffs/ (before/after for each spec)

3. OUTPUT: Updated spec files + diff records
```

**Diff Record Schema:**
```markdown
## Spec Diff: {spec_file}

**Version:** {old} → {new}

### Sections Changed
| Section | Change Type | Summary |
|---------|-------------|---------|
| 3.2 | MODIFIED | Updated state machine |
| 4.5 | ADDED | New RuleBuilder phase |
| 6.1 | DEPRECATED | WizardMenu removed |

### Implementation Implications
- New struct: RuleBuilderState
- Removed states: WizardMenu, PathfinderAnalyzing, ...
- New key handlers: 'n' → RuleBuilder
```

**Exit Criteria:**
- All spec files updated
- Diffs recorded for each change
- Version numbers incremented

---

### Phase 3: Spec Refinement

**Goal:** Validate and refine the spec changes through structured review.

**Inputs:**
- Updated spec file(s) from Phase 2
- Diff records

**Process:**
```
1. FOR each updated spec:
   a. INVOKE spec_refinement_workflow:
      - Target: updated spec file
      - Context: diff record, proposal summary
      - Max rounds: 5 (configurable)

   b. MONITOR convergence:
      - Track gaps resolved vs new gaps
      - Detect stalls (3 rounds no progress)

   c. IF stall detected:
      - PAUSE and escalate to user
      - Options: [Continue] [Adjust scope] [Accept current state]

2. COLLECT refinement outputs:
   - Final gap counts per spec
   - Key decisions made
   - Reviewer concerns addressed

3. OUTPUT: Refined specs + refinement_summary.md
```

**Refinement Summary Schema:**
```markdown
## Refinement Summary

| Spec | Rounds | Gaps Found | Gaps Resolved | Final State |
|------|--------|------------|---------------|-------------|
| discover.md | 3 | 7 | 7 | CONVERGED |
| ai_wizards.md | 2 | 3 | 3 | CONVERGED |

### Key Decisions
1. {decision_1} - Rationale: {why}
2. {decision_2} - Rationale: {why}

### Deferred Items
- {item} - Reason: {why deferred}
```

**Exit Criteria:**
- All specs converged (or user accepted partial state)
- No CRITICAL gaps remaining
- Key design decisions documented

---

### Phase 4: Corpus Alignment (Conditional)

**Goal:** Ensure spec changes don't break cross-references or introduce inconsistencies.

**Trigger Conditions:**
- Spec references other specs
- State machine changes affect multiple views
- Terminology changes (renamed concepts)
- New spec file created

**Process:**
```
1. CHECK trigger conditions:
   IF no cross-refs affected:
      SKIP to Phase 5

2. INVOKE spec_maintenance_workflow:
   - Scope: affected specs + their references
   - Mode: alignment_check (not full audit)
   - Focus: cross-references, terminology, state consistency

3. HANDLE findings:
   - CRITICAL: Pause, fix before continuing
   - MEDIUM: Auto-fix or queue for Phase 5
   - LOW: Log for future cleanup

4. OUTPUT: alignment_report.md
```

**Exit Criteria:**
- No broken cross-references
- Terminology consistent across specs
- State machines aligned (if applicable)

---

### Phase 5: Implementation

**Goal:** Implement the code changes based on spec diffs.

**Inputs:**
- Refined specs from Phase 3
- Diff records from Phase 2
- Alignment report from Phase 4 (if any)

**Process:**
```
1. EXTRACT implementation tasks from diffs:
   FOR each diff_record:
     FOR each section_change:
       IF change.type == ADDED:
         task = "Implement {section.name}"
       IF change.type == MODIFIED:
         task = "Update {section.name} to match spec"
       IF change.type == DEPRECATED:
         task = "Remove {section.name}"

       ADD task with:
         - spec_reference: section location
         - affected_files: code files to change
         - dependencies: other tasks that must complete first

2. ORDER tasks by dependency graph

3. FOR each task IN order:
   a. READ relevant code files
   b. IMPLEMENT change per spec
   c. RUN incremental check (cargo check)
   d. IF check fails:
      - Diagnose
      - Fix or rollback
      - Log issue
   e. MARK task complete

4. RUN full verification:
   - cargo check (all crates)
   - cargo test (affected tests)
   - cargo clippy (no new warnings)

5. OUTPUT: implementation_log.md
```

**Implementation Log Schema:**
```markdown
## Implementation Log

### Tasks Completed
| Task | Files Changed | Lines | Status |
|------|---------------|-------|--------|
| Add RuleBuilderState | app.rs | +45 | OK |
| Remove WizardMenu | app.rs, ui.rs | -120 | OK |
| Add key handler 'n' | app.rs | +15 | OK |

### Verification
- cargo check: PASS
- cargo test: PASS (47 tests)
- cargo clippy: PASS (no new warnings)

### Issues Encountered
- {issue_1}: {resolution}
```

**Exit Criteria:**
- All implementation tasks complete
- Full verification passes
- No regressions introduced

---

### Phase 6: Validation Suite

**Goal:** Run relevant validation workflows to ensure implementation matches spec.

**Inputs:**
- Implementation from Phase 5
- Refined specs
- Change categories (TUI, memory, data model, etc.)

**Process:**
```
1. DETERMINE required validations:
   IF TUI changed:
     queue tui_testing_workflow
   IF data paths changed:
     queue memory_audit_workflow
   IF data models changed:
     queue data_model_maintenance_workflow (analysis only)
   ALWAYS:
     queue cargo test (full suite)

2. RUN validations IN PARALLEL:
   - Each validation is independent
   - Collect results as they complete
   - Don't wait for all to start next

3. COLLECT results:
   - Pass/fail per validation
   - Findings from each
   - Actionable items

4. HANDLE failures:
   IF critical_failure:
     PAUSE, present to user
     Options: [Fix] [Skip validation] [Rollback to Phase 5]
   IF minor_issues:
     Log for follow-up
     Continue

5. OUTPUT: validation_report.md
```

**Validation Report Schema:**
```markdown
## Validation Report

### Results Summary
| Validation | Status | Findings | Actionable |
|------------|--------|----------|------------|
| tui_testing | PASS | 12 scenarios | 0 |
| memory_audit | PASS | 2 findings | 1 (LOW) |
| cargo test | PASS | 47 tests | 0 |

### Detailed Results

#### TUI Testing
- Scenarios run: 12
- Passed: 12
- Failed: 0

#### Memory Audit
- Finding 1: {description} - Severity: LOW - Deferred

### Actionable Items for Follow-up
- [ ] {item from memory audit}
```

**Exit Criteria:**
- All critical validations pass
- Findings documented
- User informed of any deferred items

---

### Phase 7: Commit & Summary

**Goal:** Package all changes and provide final summary.

**Inputs:**
- All changes from Phases 2-6
- Session artifacts

**Process:**
```
1. PREPARE commit:
   - Group logically:
     Option A: Single commit (small change)
     Option B: Spec commit + implementation commit
     Option C: Per-feature commits

   - Draft message:
     "feat({scope}): {description}

     - Updated {spec} to v{version}
     - Implemented {key_changes}
     - Validated with {validations}

     Closes #{issue} (if applicable)

     Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"

2. PRESENT options to user:
   - Show diff summary
   - Show commit grouping options
   - [Commit] [Adjust] [Don't commit yet]

3. GENERATE final summary:
   - What was proposed
   - What was implemented
   - Validation results
   - Any deferred items

4. OUTPUT: SUMMARY.md in session directory
```

**Final Summary Schema:**
```markdown
## Feature Complete: {feature_name}

**Date:** {date}
**Duration:** {total_time}

### What Changed

**Specs:**
- {spec_1}: v{old} → v{new}
- {spec_2}: v{old} → v{new}

**Code:**
- {files_changed} files changed
- +{lines_added} / -{lines_removed} lines

### Validation Results
- All {n} validations passed
- {m} deferred items logged

### Commits Created
1. {commit_hash}: {message}

### Session Artifacts
- specs/meta/sessions/{session}/
  - proposal.md
  - spec_diffs/
  - refinement_summary.md
  - implementation_log.md
  - validation_report.md
  - SUMMARY.md
```

---

## 3. State Machine

```
┌──────────────┐
│   PENDING    │ ← User invokes workflow
└──────┬───────┘
       │ User confirms proposal
       ▼
┌──────────────┐
│ SPEC_UPDATE  │ ← Updating spec files
└──────┬───────┘
       │ Specs updated
       ▼
┌──────────────┐
│  REFINING    │ ← spec_refinement_workflow running
└──────┬───────┘
       │ Converged (or user accepted)
       ▼
┌──────────────┐
│  ALIGNING    │ ← spec_maintenance_workflow (if triggered)
└──────┬───────┘
       │ Aligned (or skipped)
       ▼
┌──────────────┐
│IMPLEMENTING  │ ← Code changes in progress
└──────┬───────┘
       │ Implementation complete
       ▼
┌──────────────┐
│  VALIDATING  │ ← Validation workflows running
└──────┬───────┘
       │ Validations pass
       ▼
┌──────────────┐
│  COMMITTING  │ ← Preparing commit
└──────┬───────┘
       │ User confirms
       ▼
┌──────────────┐
│   COMPLETE   │
└──────────────┘

Error States:
┌──────────────┐
│ STALLED      │ ← Refinement not converging
├──────────────┤
│ BLOCKED      │ ← Critical validation failure
├──────────────┤
│ ABORTED      │ ← User cancelled
└──────────────┘
```

---

## 4. Escalation & Recovery

### 4.1 Refinement Stall

**Detection:** 3 rounds without gap reduction

**Options:**
1. **Continue** - Keep trying (may resolve)
2. **Reduce Scope** - Focus on subset of changes
3. **Accept Partial** - Proceed with current state, log gaps
4. **Abort** - Cancel workflow

### 4.2 Implementation Failure

**Detection:** cargo check fails after change

**Options:**
1. **Diagnose & Fix** - Attempt automatic fix
2. **Rollback Change** - Revert specific change, continue
3. **Pause for User** - User fixes manually
4. **Abort Phase** - Skip remaining implementation

### 4.3 Validation Failure

**Detection:** Critical validation fails

**Options:**
1. **Fix & Retry** - Attempt fix, re-run validation
2. **Skip Validation** - Accept risk, continue
3. **Rollback to Phase 5** - Fix implementation
4. **Rollback to Phase 3** - Fix spec

---

## 5. Configuration

### 5.1 Workflow Config

```yaml
spec_driven_feature:
  refinement:
    max_rounds: 5
    stall_threshold: 3
    auto_accept_partial: false

  alignment:
    trigger_on_cross_refs: true
    trigger_on_new_spec: true
    mode: alignment_check  # vs full_audit

  implementation:
    incremental_check: true
    rollback_on_fail: true
    parallel_independent_tasks: false  # safer sequential

  validation:
    parallel: true
    required:
      - cargo_test
    conditional:
      - tui_testing: "specs/views/*.md changed"
      - memory_audit: "data path code changed"

  commit:
    default_grouping: single  # single | by_phase | by_file
    require_user_confirm: true
```

### 5.2 Per-Invocation Overrides

```
User: "workflow: spec-driven feature for Rule Builder --skip-alignment --validation=minimal"
```

---

## 6. Session Artifacts

All artifacts stored in: `specs/meta/sessions/{feature_name}_{date}/`

```
{session}/
├── proposal.md              # Phase 1 output
├── spec_diffs/              # Phase 2 output
│   ├── discover.md.diff
│   └── ai_wizards.md.diff
├── refinement_summary.md    # Phase 3 output
├── alignment_report.md      # Phase 4 output (if run)
├── implementation_log.md    # Phase 5 output
├── validation_report.md     # Phase 6 output
└── SUMMARY.md               # Phase 7 output
```

---

## 7. Example Invocation

### User Request
```
"I want to consolidate the AI wizards into the Rule Builder.
The GlobExplorer and RuleCreation should merge, AI should be
a Tab key helper not separate wizards."
```

### Workflow Execution

**Phase 1: Proposal Capture**
```
Proposal Summary:
- Request: Consolidate 5 AI interfaces into 1 Rule Builder
- Affected Specs: discover.md, ai_wizards.md
- Key Changes:
  1. Merge GlobExplorer + RuleCreation → RuleBuilder
  2. Make AI a Tab-key helper
  3. Deprecate wizard states
```

**Phase 2: Spec Update**
```
Updated discover.md: v2.3 → v3.0
Updated ai_wizards.md: v0.5 → v0.6
Created: design.md for Rule Builder details
```

**Phase 3: Refinement**
```
spec_refinement on discover.md: 3 rounds, 7 gaps resolved
spec_refinement on design.md: 2 rounds, 4 gaps resolved
All specs CONVERGED
```

**Phase 4: Alignment**
```
Cross-ref check: discover.md ↔ ai_wizards.md
Result: Terminology aligned, no broken refs
```

**Phase 5: Implementation**
```
Tasks extracted: 13
- Add RuleBuilderState struct
- Add RuleBuilderFocus enum
- Remove WizardMenu state
- ... (10 more)

All tasks completed. Verification: PASS
```

**Phase 6: Validation**
```
Running in parallel:
- tui_testing: 12 scenarios, PASS
- cargo test: 47 tests, PASS

No failures.
```

**Phase 7: Commit**
```
Commit created:
  feat(tui): consolidate AI wizards into Rule Builder

  - Updated discover.md to v3.0
  - Updated ai_wizards.md to v0.6
  - Implemented unified Rule Builder interface
  - Deprecated WizardMenu, Pathfinder, Labeling, SemanticPath states

  Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
```

---

## 8. Integration with Workflow Manager

### 8.1 Routing

The Workflow Manager routes to this workflow when:

```
Signals:
- "spec-driven" OR "update spec and implement" in request
- Feature request + spec files mentioned
- Complex feature (>5 files estimated)
- User explicitly requests: "workflow: spec-driven"

Category: FEATURE_WORK.SPEC_DRIVEN
```

### 8.2 Sub-Workflow Invocation

This workflow invokes:
- `spec_refinement_workflow` (Phase 3)
- `spec_maintenance_workflow` (Phase 4, conditional)
- `tui_testing_workflow` (Phase 6, conditional)
- `memory_audit_workflow` (Phase 6, conditional)

### 8.3 Output for Manager Learning

```rust
struct SpecDrivenOutcome {
    proposal_clarity: f32,        // 0-1, how clear was initial request
    refinement_rounds: u32,       // Total rounds across specs
    alignment_triggered: bool,
    implementation_success_rate: f32,
    validation_pass_rate: f32,
    total_duration_mins: u32,
    deferred_items: u32,
}
```

---

## 9. Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-14 | Initial specification |
