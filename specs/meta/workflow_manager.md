# Workflow Manager Specification

**Version:** 1.6.0
**Status:** DRAFT
**Created:** 2026-01-14

---

## 1. Overview

The Workflow Manager is a meta-orchestration system that coordinates execution of specialized workflows, manages context propagation between them, and continuously improves the workflow corpus through pattern recognition and gap analysis.

### 1.1 Design Philosophy

```
User Request → Manager Analysis → Workflow Sequence → Execution → Learning
                    ↑                                      │
                    └──────── Feedback Loop ───────────────┘
```

The Manager operates as a **single persistent instance** with:
- Long-running context across sessions
- Memory of workflow executions and outcomes
- Pattern library accumulated from observations
- Ability to propose workflow amendments

---

## 2. Core Responsibilities

| Responsibility | Description |
|----------------|-------------|
| **Routing** | Map user requests to appropriate workflow(s) |
| **Sequencing** | Determine optimal workflow order and parallelization |
| **Context Bridging** | Transform outputs of one workflow into inputs for another |
| **Monitoring** | Track execution health, detect failures early |
| **Learning** | Identify patterns, gaps, and improvement opportunities |
| **Proposing** | Generate workflow amendments based on observations |

---

## 3. Architecture

### 3.1 Instance Model

```
┌─────────────────────────────────────────────────────────────────┐
│                     WORKFLOW MANAGER                             │
│                   (Persistent Context)                           │
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │  Routing    │  │  Execution  │  │  Learning   │              │
│  │  Engine     │  │  Monitor    │  │  Engine     │              │
│  └─────────────┘  └─────────────┘  └─────────────┘              │
└─────────────────────────────────────────────────────────────────┘
                              │
    ┌─────────────────────────┼─────────────────────────┐
    ▼                         ▼                         ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ Feature Workflow│  │ Spec Refinement │  │ Spec Maintenance│
│ (1-inst+escal)  │  │   (3-instance)  │  │  (1-instance)   │
└────────┬────────┘  └─────────────────┘  └─────────────────┘
         │ escalates to ▲         │                    │
         └──────────────┘         ▼                    ▼
                        ┌─────────────────┐  ┌─────────────────┐
                        │  Memory Audit   │  │  Data Model     │
                        │  (3-instance)   │  │  Maintenance    │
                        └─────────────────┘  └─────────────────┘
                                  │                    │
                                  ▼                    ▼
                        ┌─────────────────┐  ┌─────────────────┐
                        │  TUI Testing    │  │   [Future]      │
                        │   Workflow      │  │   Workflows     │
                        └─────────────────┘  └─────────────────┘
```

### 3.2 Managed Workflows Registry

| Workflow | Instance Model | Primary Trigger | Typical Duration |
|----------|----------------|-----------------|------------------|
| `feature_workflow` | 1-instance with escalation | New feature, bug fix, refactor | 2-8 prompts |
| `spec_driven_feature_workflow` | 1-instance orchestrator | Spec-first feature development | 7 phases |
| `spec_refinement_workflow` | 3-instance (Engineer, Reviewer, Mediator) | New spec or major feature | 3-7 rounds |
| `spec_maintenance_workflow` | 1-instance | Periodic audit, post-refactor | 5 phases |
| `memory_audit_workflow` | 3-instance (Analyst, Validator, Coordinator) | Performance concerns | 3-5 rounds |
| `data_model_maintenance_workflow` | 1-instance | Schema changes, tech debt | 5 phases |
| `abstraction_audit_workflow` | 3-instance (Analyst, Validator, Coordinator) | Platform coupling audit | 2-4 rounds |
| `tui_testing_workflow` | 1-instance | TUI test procedures | Per-scenario |
| `tui_validation_workflow` | 1-instance orchestrator | TUI validation (context-driven) | 3 phases |
| `code_philosophy_review_workflow` | 1-instance | "blow", "muratori", over-engineering review | 1 prompt |

### 3.3 Workflow Categories

Workflows fall into two categories with different output requirements:

#### 3.3.1 Analysis Workflows

**Purpose:** Find issues in code/specs for later implementation.

| Workflow | Output |
|----------|--------|
| `memory_audit_workflow` | `actionable_findings.json` |
| `spec_maintenance_workflow` | `actionable_findings.json` |
| `data_model_maintenance_workflow` | `actionable_findings.json` |
| `abstraction_audit_workflow` | `actionable_findings.json` |
| `spec_refinement_workflow` | `actionable_findings.json` (IMPL_* gaps) |
| `tui_testing_workflow` | `actionable_findings.json` (divergences) |
| `tui_validation_workflow` | `actionable_findings.json` (TUI issues) |

**Characteristics:**
- Identify issues but don't fix them
- Emit `actionable_findings.json` per Section 13
- User can say "implement the findings" → Manager passes to Implementation Protocol
- Findings are auto-implementable (have `suggested_fix`, `verify_command`)

#### 3.3.2 Implementation Workflows

**Purpose:** Directly make code changes.

| Workflow | Output |
|----------|--------|
| `feature_workflow` | `execution_metrics.json` only |
| `spec_driven_feature_workflow` | `execution_metrics.json` + session artifacts |

**Characteristics:**
- Directly implement features or fixes
- Do NOT emit `actionable_findings.json` (they ARE the implementation)
- Issues found during validation are either:
  - Auto-fixed immediately, or
  - Flagged for human decision (not auto-implementable)
- Emit `execution_metrics.json` for Manager learning

**Note:** `spec_driven_feature_workflow` orchestrates sub-workflows (spec_refinement, validation) but is itself an Implementation workflow because it directly produces code changes.

#### 3.3.3 Why the Distinction Matters

```
Analysis Workflow:
  [Find issues] → actionable_findings.json → "implement these" → Implementation Protocol

Implementation Workflow:
  [Build feature] → [Fix issues inline] → execution_metrics.json
                                              ↓
                                     (no separate "implement" step)
```

Conflating these leads to inappropriate outputs:
- Analysis workflows emitting just metrics → Manager can't route to implementation
- Implementation workflows emitting findings → Nothing to consume them (already fixed)

---

## 4. Routing Engine

### 4.1 Request Classification

The Manager classifies user requests into workflow-triggering categories:

```
REQUEST TAXONOMY
├── FEATURE_WORK
│   ├── NEW_FEATURE       → feature_workflow
│   ├── BUG_FIX           → feature_workflow (simplified)
│   ├── REFACTOR          → feature_workflow
│   ├── COMPLEX_FEATURE   → feature_workflow (escalates to spec_refinement)
│   └── SPEC_DRIVEN       → spec_driven_feature_workflow
│
├── SPEC_WORK
│   ├── NEW_SPEC          → spec_refinement_workflow
│   ├── REFINE_SPEC       → spec_refinement_workflow
│   ├── AUDIT_SPECS       → spec_maintenance_workflow
│   └── CROSS_REF_CHECK   → spec_maintenance_workflow
│
├── CODE_QUALITY
│   ├── MEMORY_CONCERNS   → memory_audit_workflow
│   ├── DATA_MODEL_DEBT   → data_model_maintenance_workflow
│   ├── PLATFORM_COUPLING → abstraction_audit_workflow
│   └── FULL_AUDIT        → [memory + data_model + abstraction in sequence]
│
├── TUI_WORK
│   ├── TUI_CHANGE        → tui_validation_workflow (context-driven)
│   ├── TUI_VALIDATION    → tui_validation_workflow (post-feature)
│   ├── TUI_TEST_ONLY     → tui_testing_workflow (direct procedure)
│   └── TUI_SPEC_IMPL     → spec_driven_feature_workflow (TUI-focused)
│
└── META
    ├── WORKFLOW_REVIEW   → [self-analysis mode]
    └── PATTERN_REPORT    → [learning engine output]
```

### 4.2 Routing Algorithm

```
FUNCTION route_request(user_request):
    # 1. Extract intent signals
    signals = extract_signals(user_request)

    # 2. Match against taxonomy
    category = classify(signals)

    # 3. Check for compound requests
    IF contains_multiple_intents(signals):
        workflows = decompose_to_sequence(category)
    ELSE:
        workflows = [primary_workflow_for(category)]

    # 4. Check dependencies
    FOR workflow IN workflows:
        IF has_prerequisite(workflow) AND NOT prerequisite_satisfied():
            INSERT prerequisite_workflow BEFORE workflow

    # 5. Apply parallelization rules
    execution_plan = optimize_parallelism(workflows)

    RETURN execution_plan
```

### 4.3 Signal Keywords

| Signal Keywords | Likely Category |
|-----------------|-----------------|
| "add", "implement", "build", "create feature" | FEATURE_WORK.NEW_FEATURE |
| "fix", "bug", "broken", "doesn't work" | FEATURE_WORK.BUG_FIX |
| "refactor", "clean up", "reorganize" | FEATURE_WORK.REFACTOR |
| "update spec and implement", "spec-driven", "change feature X" | FEATURE_WORK.SPEC_DRIVEN |
| "consolidate", "redesign", "rearchitect" + spec files | FEATURE_WORK.SPEC_DRIVEN |
| "spec", "specification", "define", "design" | SPEC_WORK |
| "memory", "allocation", "performance", "clone" | CODE_QUALITY.MEMORY |
| "data model", "struct", "type", "schema" | CODE_QUALITY.DATA_MODEL |
| "TUI", "terminal", "UI", "ratatui", "display" | TUI_WORK |
| "audit", "check", "review all", "corpus" | Maintenance workflows |
| "refine", "gaps", "incomplete", "missing" | Refinement workflows |

---

## 5. Context Bridging

### 5.1 Context Object Schema

```rust
struct WorkflowContext {
    // Identity
    session_id: String,
    workflow_chain: Vec<WorkflowExecution>,

    // Accumulated State
    specs_touched: Vec<String>,
    gaps_found: Vec<Gap>,
    recommendations: Vec<Recommendation>,
    decisions_made: Vec<Decision>,

    // Metrics
    total_rounds: u32,
    total_duration_mins: u32,

    // Artifacts
    files_created: Vec<String>,
    files_modified: Vec<String>,

    // Learning Signals
    patterns_observed: Vec<PatternObservation>,
    friction_points: Vec<FrictionPoint>,
}
```

### 5.2 Inter-Workflow Handoff

When workflow A completes and workflow B begins:

```
┌───────────────┐                    ┌───────────────┐
│  Workflow A   │                    │  Workflow B   │
│   Completes   │                    │    Starts     │
└───────┬───────┘                    └───────┬───────┘
        │                                    │
        ▼                                    │
┌───────────────────┐                        │
│ Output Artifacts  │                        │
│ - status.md       │                        │
│ - spec changes    │                        │
│ - recommendations │                        │
└───────┬───────────┘                        │
        │                                    │
        ▼                                    │
┌───────────────────┐                        │
│ Context Transformer│                       │
│ - Extract relevant │                       │
│ - Reformat for B   │                       │
│ - Add continuity   │                       │
└───────┬───────────┘                        │
        │                                    │
        └────────────────────────────────────┘
                         │
                         ▼
              ┌───────────────────┐
              │ B's Initial Prompt │
              │ WITH context from A│
              └───────────────────┘
```

### 5.3 Common Handoff Patterns

| From | To | Context Passed |
|------|-----|----------------|
| `spec_refinement` | `tui_testing` | Spec file path, key sections, expected behaviors |
| `spec_maintenance` | `spec_refinement` | Identified gaps, priority ordering, cross-refs |
| `memory_audit` | `data_model_maintenance` | Type relationships, hot paths, allocation sites |
| `data_model_maintenance` | `spec_maintenance` | Schema changes requiring spec updates |

---

## 6. Execution Monitor

### 6.1 Health Signals

| Signal | Description | Threshold | Action |
|--------|-------------|-----------|--------|
| `ROUND_STALL` | No progress in N rounds | 3 rounds | Escalate to user |
| `GAP_EXPLOSION` | New gaps > resolved gaps | 2x ratio | Review scope |
| `CONVERGENCE_FAIL` | Weighted score not improving | 3 rounds | Suggest pivot |
| `TIMEOUT` | Workflow exceeds time budget | Workflow-specific | Checkpoint & pause |
| `ERROR_CASCADE` | Multiple tool failures | 3 consecutive | Halt & diagnose |

### 6.2 Intervention Protocol

```
WHEN health_signal DETECTED:
    1. CAPTURE current state (save checkpoint)
    2. CLASSIFY intervention type:
       - INFORMATIONAL: Log and continue
       - ADVISORY: Notify user, continue unless stopped
       - BLOCKING: Pause workflow, require user decision
    3. IF user_decision REQUIRED:
       - Present options via AskUserQuestion
       - Options: [Continue, Adjust Scope, Switch Workflow, Abort]
    4. RECORD intervention in learning log
```

---

## 7. Learning Engine

### 7.1 Pattern Categories

```
PATTERN TAXONOMY
├── EFFICIENCY_PATTERNS
│   ├── PARALLELIZABLE_PHASES    # Phases that could run in parallel
│   ├── REDUNDANT_CHECKS         # Duplicate validation across workflows
│   └── SHORTCUT_OPPORTUNITIES   # Common paths that could be optimized
│
├── GAP_PATTERNS
│   ├── RECURRING_GAP_TYPES      # Same gap categories appearing repeatedly
│   ├── MISSING_WORKFLOW_COVERAGE # User requests without matching workflow
│   └── HANDOFF_FAILURES         # Context lost between workflows
│
├── FRICTION_PATTERNS
│   ├── FREQUENT_USER_QUESTIONS  # Points where users consistently ask questions
│   ├── ABORT_POINTS             # Where users commonly abandon workflows
│   └── BACKTRACK_TRIGGERS       # What causes workflow restarts
│
└── SUCCESS_PATTERNS
    ├── OPTIMAL_SEQUENCES        # Workflow combinations that work well together
    ├── EFFECTIVE_PROMPTS        # Prompt formulations with high success rates
    └── IDEAL_SCOPE_SIZES        # Task granularity that leads to completion
```

### 7.2 Observation Collection

During each workflow execution, the Manager collects:

```rust
struct PatternObservation {
    // When
    timestamp: DateTime,
    workflow_id: String,
    phase_or_round: String,

    // What
    pattern_type: PatternCategory,
    description: String,
    evidence: Vec<String>,  // Specific instances

    // Impact
    frequency: u32,         // Times observed
    severity: Severity,     // LOW/MEDIUM/HIGH

    // Proposed Action
    suggested_change: Option<WorkflowAmendment>,
}
```

### 7.3 Pattern Detection Algorithm

```
FUNCTION detect_patterns(execution_history):
    observations = []

    # Check for recurring gaps
    gap_counts = count_by_type(all_gaps_from(execution_history))
    FOR gap_type, count IN gap_counts:
        IF count >= RECURRENCE_THRESHOLD:
            observations.append(PatternObservation{
                pattern_type: RECURRING_GAP_TYPES,
                description: f"{gap_type} appeared {count} times",
                suggested_change: propose_workflow_addition(gap_type)
            })

    # Check for parallelization opportunities
    FOR workflow_execution IN execution_history:
        sequential_phases = find_sequential_independent_phases(workflow_execution)
        IF sequential_phases:
            observations.append(PatternObservation{
                pattern_type: PARALLELIZABLE_PHASES,
                suggested_change: propose_parallelization(sequential_phases)
            })

    # Check for handoff failures
    FOR handoff IN all_handoffs(execution_history):
        IF handoff.context_usage_rate < 0.5:  # Less than half of context used
            observations.append(PatternObservation{
                pattern_type: HANDOFF_FAILURES,
                suggested_change: propose_context_schema_change(handoff)
            })

    RETURN observations
```

### 7.4 Single-Execution Review

Beyond aggregate pattern detection, the Manager performs **immediate retrospective review** after each workflow completes. This treats the workflow itself as an artifact to be reviewed, similar to how the Reviewer role analyzes specs.

#### 7.4.1 Workflow Output Schemas

Each workflow produces structured outputs the Manager can inspect:

| Workflow | Key Outputs | Location |
|----------|-------------|----------|
| `spec_refinement_workflow` | Gap inventory, round count, convergence metrics, reviewer disagreements | `status.md`, spec diff |
| `spec_maintenance_workflow` | Spec inventory, alignment issues, cross-ref gaps, overlap detections | Phase reports |
| `memory_audit_workflow` | Optimization candidates, validation results, category distribution | `status.md`, findings |
| `data_model_maintenance_workflow` | Type inventory, usage patterns, dead code, recommendations | Phase reports |
| `tui_testing_workflow` | Scenario results, pass/fail counts, failure descriptions | Test output |

#### 7.4.2 Review Criteria

```
REVIEW DIMENSIONS
├── EFFICIENCY
│   ├── rounds_vs_expected        # Did it take more rounds than typical?
│   ├── phase_duration_variance   # Any phase unusually slow?
│   └── backtrack_count           # How many times did it revisit earlier work?
│
├── QUALITY
│   ├── convergence_achieved      # Did gaps stabilize?
│   ├── new_gap_ratio             # New gaps found vs resolved
│   └── reviewer_agreement_rate   # (For multi-instance) How often did roles agree?
│
├── COVERAGE
│   ├── scope_completion          # Did it address everything requested?
│   ├── gap_types_addressed       # Distribution across severity levels
│   └── artifacts_produced        # Expected outputs actually created?
│
└── HANDOFF_READINESS
    ├── context_completeness      # Is output usable by next workflow?
    ├── artifact_structure        # Do outputs match expected schema?
    └── actionable_items          # Clear next steps identified?
```

#### 7.4.3 Single-Execution Review Algorithm

```
FUNCTION review_execution(workflow_id, execution_record):
    findings = []

    # Load workflow definition and execution outputs
    workflow_def = load_workflow(workflow_id)
    outputs = execution_record.outputs
    metrics = execution_record.metrics

    # EFFICIENCY REVIEW
    IF metrics.rounds > workflow_def.expected_rounds * 1.5:
        findings.append(Finding{
            dimension: EFFICIENCY,
            issue: "Exceeded expected rounds by 50%+",
            evidence: f"Expected {workflow_def.expected_rounds}, took {metrics.rounds}",
            suggested_investigation: "Review prompts for ambiguity in rounds " +
                                     identify_slow_rounds(execution_record)
        })

    IF metrics.backtrack_count > 2:
        findings.append(Finding{
            dimension: EFFICIENCY,
            issue: "Excessive backtracking",
            evidence: describe_backtracks(execution_record),
            suggested_change: "Add checkpoint or clarification step before " +
                              identify_backtrack_trigger(execution_record)
        })

    # QUALITY REVIEW
    IF NOT metrics.convergence_achieved:
        findings.append(Finding{
            dimension: QUALITY,
            issue: "Failed to converge",
            evidence: plot_convergence_trend(execution_record),
            suggested_change: "Review termination criteria or scope definition"
        })

    IF metrics.new_gap_ratio > 0.5:  # More than half of gaps were new discoveries
        findings.append(Finding{
            dimension: QUALITY,
            issue: "High new-gap rate suggests incomplete initial analysis",
            evidence: list_unexpected_gaps(execution_record),
            suggested_change: "Strengthen initial discovery phase prompts"
        })

    # For multi-instance workflows
    IF workflow_def.instance_count > 1:
        agreement_rate = calculate_agreement_rate(execution_record)
        IF agreement_rate < 0.6:
            findings.append(Finding{
                dimension: QUALITY,
                issue: "Low inter-role agreement",
                evidence: list_disagreements(execution_record),
                suggested_change: "Clarify criteria or add Mediator guidance for: " +
                                  identify_disagreement_patterns(execution_record)
            })

    # COVERAGE REVIEW
    missing_artifacts = expected_artifacts(workflow_def) - produced_artifacts(outputs)
    IF missing_artifacts:
        findings.append(Finding{
            dimension: COVERAGE,
            issue: "Missing expected artifacts",
            evidence: missing_artifacts,
            suggested_change: "Add explicit artifact checklist to completion criteria"
        })

    # HANDOFF READINESS REVIEW
    IF execution_record.has_successor_workflow:
        successor = execution_record.successor_workflow
        context_gaps = analyze_context_gaps(outputs, successor.required_inputs)
        IF context_gaps:
            findings.append(Finding{
                dimension: HANDOFF_READINESS,
                issue: "Incomplete context for successor workflow",
                evidence: context_gaps,
                suggested_change: "Add output requirements: " + context_gaps
            })

    # GENERATE PROPOSALS
    proposals = []
    FOR finding IN findings:
        IF finding.severity >= MEDIUM OR finding.is_recurring():
            proposals.append(generate_proposal(finding, execution_record))

    RETURN ReviewResult{
        findings: findings,
        immediate_proposals: [p FOR p IN proposals IF p.confidence >= HIGH],
        deferred_observations: [p FOR p IN proposals IF p.confidence < HIGH]
    }
```

#### 7.4.4 Immediate vs Deferred Proposals

| Confidence | Criteria | Action |
|------------|----------|--------|
| **HIGH** | Clear causal evidence from single execution | Generate proposal immediately |
| **MEDIUM** | Suggestive but needs more data | Log observation, propose after 3 occurrences |
| **LOW** | Anomaly that might be one-off | Log only, no proposal |

**Immediate proposal triggers:**
- Workflow failed to converge (QUALITY)
- Missing required artifacts (COVERAGE)
- Context gap blocking successor workflow (HANDOFF_READINESS)
- >2x expected duration (EFFICIENCY)

**Deferred observation triggers:**
- Slightly elevated round count (1.2-1.5x expected)
- Single backtrack event
- Minor reviewer disagreements
- Non-blocking context gaps

#### 7.4.5 Retrospective Prompt Template

After each workflow execution, the Manager can generate a structured retrospective:

```markdown
## Workflow Retrospective: {workflow_name}

**Execution ID:** {execution_id}
**Date:** {timestamp}
**Duration:** {duration} ({rounds} rounds)

---

### Execution Summary

| Metric | Value | Benchmark | Status |
|--------|-------|-----------|--------|
| Rounds | {actual_rounds} | {expected_rounds} | {OK/WARN/CRITICAL} |
| Convergence | {achieved/not_achieved} | Required | {OK/FAIL} |
| New Gap Ratio | {ratio}% | <50% | {OK/WARN} |
| Artifacts Produced | {count}/{expected} | 100% | {OK/WARN} |

---

### Findings

{FOR finding IN findings:}
#### {finding.dimension}: {finding.issue}

**Evidence:** {finding.evidence}

**Suggested Action:** {finding.suggested_change}

**Confidence:** {finding.confidence}

{END FOR}

---

### Proposals Generated

**Immediate (HIGH confidence):**
{list immediate_proposals OR "None"}

**Deferred (needs more data):**
{list deferred_observations OR "None"}

---

### Manager Notes

{Free-form observations about this execution that don't fit categories above}
```

#### 7.4.6 Review Trigger Points

The Manager performs single-execution review at these points:

```
WORKFLOW LIFECYCLE
    │
    ├─ START
    │   └─ Record: initial_scope, expected_rounds, predecessor_context
    │
    ├─ ROUND/PHASE COMPLETE
    │   └─ Record: duration, outputs, state_changes
    │   └─ Check: health_signals (Section 6.1)
    │
    ├─ COMPLETION ◄─── TRIGGER SINGLE-EXECUTION REVIEW
    │   └─ Execute: review_execution()
    │   └─ Generate: retrospective
    │   └─ Create: immediate_proposals
    │   └─ Log: deferred_observations
    │
    └─ HANDOFF (if successor exists)
        └─ Validate: context_completeness
        └─ Transform: context for successor
```

#### 7.4.7 Feedback Loop to Workflow Definitions

When proposals are approved, the Manager updates the workflow definition:

```
PROPOSAL APPROVED
       │
       ▼
┌──────────────────────────────────────────┐
│         UPDATE WORKFLOW SPEC             │
│                                          │
│  1. Read current workflow .md file       │
│  2. Apply approved change                │
│  3. Increment workflow version           │
│  4. Add changelog entry                  │
│  5. Update expected_rounds/metrics       │
│     if efficiency change                 │
└──────────────────────────────────────────┘
       │
       ▼
┌──────────────────────────────────────────┐
│         UPDATE MANAGER STATE             │
│                                          │
│  1. Move proposal: pending → approved    │
│  2. Link to workflow version             │
│  3. Reset related deferred observations  │
│  4. Adjust benchmarks based on change    │
└──────────────────────────────────────────┘
```

---

## 8. Amendment Proposals

### 8.1 Amendment Types

| Type | Description | Auto-Apply? |
|------|-------------|-------------|
| `PROMPT_REFINEMENT` | Improve workflow prompts for clarity | No |
| `PHASE_REORDER` | Change execution order | No |
| `PARALLELIZATION` | Add parallel execution | No |
| `NEW_CHECKPOINT` | Add status tracking point | Yes (with approval) |
| `CONTEXT_SCHEMA` | Change handoff data structure | No |
| `NEW_WORKFLOW` | Propose entirely new workflow | No |
| `DEPRECATION` | Mark workflow as obsolete | No |

### 8.2 Proposal Format

```markdown
## Workflow Amendment Proposal

**ID:** WA-2026-001
**Target Workflow:** spec_refinement_workflow
**Amendment Type:** PARALLELIZATION
**Confidence:** HIGH (based on 12 observations)

### Observation
In rounds 2-5 of spec refinement, the Engineer and Reviewer phases
are currently sequential. Analysis of 12 executions shows they
share no dependencies until the Mediator phase.

### Evidence
- Execution #7: Engineer took 4 mins, Reviewer took 3 mins, no blocking
- Execution #9: Engineer took 6 mins, Reviewer took 4 mins, no blocking
- [10 more instances...]

### Proposed Change
```diff
- # Round N
- 1. Engineer analyzes gaps
- 2. Reviewer validates findings
- 3. Mediator reconciles
+ # Round N
+ 1. Engineer analyzes gaps    ─┬─ PARALLEL
+ 1. Reviewer validates findings ─┘
+ 2. Mediator reconciles (after both complete)
```

### Impact Assessment
- **Time Savings:** ~40% reduction in round duration
- **Risk:** LOW - no data dependencies identified
- **Rollback:** Easy - revert to sequential if issues

### Decision Required
[ ] APPROVE - Apply this change
[ ] DEFER - Consider later
[ ] REJECT - Do not apply (please explain)
```

---

## 9. Persistent State

### 9.1 State File Structure

```
specs/meta/workflow_manager/
├── state.json                 # Current manager state
├── execution_history/         # Per-session logs
│   ├── session_001.json
│   ├── session_002.json
│   └── ...
├── patterns/
│   ├── observed.json          # Raw observations
│   ├── confirmed.json         # Validated patterns
│   └── dismissed.json         # False positives
├── proposals/
│   ├── pending/               # Awaiting decision
│   ├── approved/              # Accepted amendments
│   └── rejected/              # Declined proposals
└── metrics/
    ├── workflow_stats.json    # Per-workflow metrics
    └── handoff_stats.json     # Inter-workflow metrics
```

### 9.2 State Schema

```json
{
  "manager_version": "1.0.0",
  "last_active": "2026-01-14T10:30:00Z",
  "total_sessions": 47,
  "workflows_managed": [
    "spec_refinement_workflow",
    "spec_maintenance_workflow",
    "memory_audit_workflow",
    "data_model_maintenance_workflow",
    "tui_testing_workflow"
  ],
  "active_session": null,
  "pending_proposals": 3,
  "patterns_confirmed": 12,
  "cumulative_metrics": {
    "total_rounds_executed": 234,
    "avg_rounds_per_session": 4.98,
    "workflow_success_rate": 0.87,
    "most_common_sequence": ["spec_refinement", "tui_testing"]
  }
}
```

---

## 10. User Interaction Protocol

### 10.1 Session Start

```
USER: "I need to refine the discover spec and then test the TUI changes"

MANAGER RESPONSE:
I've analyzed your request and identified a 2-workflow sequence:

**Proposed Execution Plan:**
1. `spec_refinement_workflow` on `specs/views/discover.md`
   - Estimated: 3-5 rounds
   - Will produce: Gap inventory, spec updates, status.md

2. `tui_testing_workflow`
   - Triggered after spec refinement completes
   - Context passed: Key behaviors from refined spec
   - Will produce: Test scenarios, validation results

**Options:**
1. [Proceed with plan] (Recommended)
2. [Start with spec refinement only]
3. [Start with TUI testing only]
4. [Modify sequence]
```

### 10.2 Mid-Execution Check-ins

At natural breakpoints (end of phases/rounds):

```
MANAGER STATUS UPDATE:

**Workflow:** spec_refinement_workflow
**Phase:** Round 2 Complete

**Progress:**
- Gaps resolved: 4/7
- New gaps found: 1
- Convergence: IMPROVING

**Health:** ✓ Normal

**Next:** Round 3 - Resolving remaining 4 gaps

[Continue] [Pause] [Adjust Scope] [View Details]
```

### 10.3 Completion Summary

```
MANAGER SESSION SUMMARY:

**Workflows Executed:** 2
**Total Duration:** 45 minutes
**Outcomes:**
  - spec_refinement: SUCCESS (7 gaps resolved, spec v1.4)
  - tui_testing: SUCCESS (12 scenarios passed)

**Files Modified:**
  - specs/views/discover.md (+47 lines)
  - specs/meta/sessions/discover_refine/status.md (new)

**Patterns Observed:**
  - 1 new efficiency opportunity logged
  - Similar gap type to previous session (HANDOFF_FAILURES)

**Pending Proposals:** 1 awaiting your review
  → View with: "show workflow proposals"
```

---

## 11. Manager Invocation

### 11.1 Direct Invocation

```
# User explicitly requests manager
"workflow manager: audit all specs"
"@workflow-manager run memory audit"
"/workflow audit specs"
```

### 11.2 Implicit Activation

The Manager activates when it detects multi-workflow intent:

```
# Implicit triggers
"refine the spec and then test"     → sequence detected
"do a full code quality pass"       → compound workflow
"audit specs and fix any issues"    → workflow + follow-up
```

### 11.3 Manager Commands

| Command | Action |
|---------|--------|
| `workflow status` | Show current execution state |
| `workflow history` | List recent sessions |
| `workflow proposals` | Show pending amendments |
| `workflow patterns` | Show confirmed patterns |
| `workflow explain <id>` | Detail specific workflow |
| `workflow abort` | Halt current execution |
| `workflow checkpoint` | Save state and pause |

---

## 12. Implementation Phases

### Phase 1: Foundation
- [ ] Create `specs/meta/workflow_manager/` directory structure
- [ ] Implement state persistence (state.json)
- [ ] Build routing engine with keyword matching
- [ ] Create execution logging

### Phase 2: Orchestration
- [ ] Implement context bridging for all 5 workflows
- [ ] Add health monitoring signals
- [ ] Build intervention protocol
- [ ] Add checkpoint/resume capability

### Phase 3: Learning
- [ ] Implement pattern observation collection
- [ ] Build pattern detection algorithms
- [ ] Create proposal generation
- [ ] Add proposal review interface

### Phase 4: Implementation Protocol
- [ ] Define ActionableFinding schema
- [ ] Implement triage and prioritization logic
- [ ] Build implementation loop with rollback
- [ ] Add verification gate (check, test, clippy)
- [ ] Implement commit grouping strategies
- [ ] Add conflict detection for stale findings

### Phase 5: Optimization
- [ ] Add parallelization support for sequences
- [ ] Add parallel implementation for independent findings
- [ ] Implement smart routing (ML-based)
- [ ] Build metrics dashboard
- [ ] Add workflow recommendation engine

---

## 13. Implementation Protocol

When **Analysis workflows** (see Section 3.3.1) produce actionable findings, the Manager executes a lightweight implementation phase rather than spawning a full workflow.

> **Note:** This protocol applies ONLY to Analysis workflows that emit `actionable_findings.json`. Implementation workflows like `feature_workflow` handle their own fixes inline and don't produce findings for this protocol.

### 13.1 Actionable Finding Schema

Analysis workflows that produce code-change recommendations must emit findings in this standard format:

```rust
struct ActionableFinding {
    // Identity
    id: String,                      // e.g., "MEM-2026-001"
    source_workflow: String,         // e.g., "memory_audit_workflow"
    source_round: String,            // e.g., "round_3"

    // Location
    file_path: String,               // e.g., "crates/casparian/src/scanner.rs"
    line_start: u32,                 // e.g., 145
    line_end: Option<u32>,           // e.g., Some(152) for multi-line

    // Classification
    category: FindingCategory,       // See 13.2
    severity: Severity,              // CRITICAL, HIGH, MEDIUM, LOW
    confidence: Confidence,          // HIGH, MEDIUM, LOW

    // Description
    title: String,                   // e.g., "Unnecessary clone of Vec<FileInfo>"
    description: String,             // Detailed explanation
    current_code: Option<String>,    // Snippet of existing code
    suggested_fix: Option<String>,   // Proposed replacement

    // Dependencies
    blocks: Vec<String>,             // Finding IDs this blocks
    blocked_by: Vec<String>,         // Finding IDs blocking this
    related_files: Vec<String>,      // Other files that may need changes

    // Verification
    verify_command: Option<String>,  // e.g., "cargo test -p casparian scanner"
    expected_outcome: String,        // e.g., "Tests pass, no new warnings"
}

enum FindingCategory {
    // From memory_audit_workflow
    UnnecessaryClone,
    HeapAvoidable,
    AllocationInLoop,
    CacheHostile,
    LifetimeExtension,
    ArenaCandidate,

    // From data_model_maintenance_workflow
    DeadType,
    DeadField,
    MissingDerive,
    TypeDuplication,
    InconsistentNaming,
    MissingValidation,

    // From abstraction_audit_workflow
    DatabaseCoupling,
    LlmCoupling,
    StorageCoupling,
    ConfigCoupling,
    QueueCoupling,
    SerializationCoupling,

    // From spec_refinement_workflow (implementation gaps)
    MissingFunction,
    MissingErrorHandler,
    IncompleteMatch,
    MissingTest,

    // General
    Refactor,
    Documentation,
    Other(String),
}
```

### 13.2 Workflow Output Requirements

**Analysis workflows** (Section 3.3.1) must emit `actionable_findings.json` alongside their status.md:

| Workflow | Finding Categories | Typical Count |
|----------|-------------------|---------------|
| `memory_audit_workflow` | UnnecessaryClone, HeapAvoidable, AllocationInLoop, CacheHostile, LifetimeExtension, ArenaCandidate | 5-20 |
| `data_model_maintenance_workflow` | DeadType, DeadField, MissingDerive, TypeDuplication, InconsistentNaming | 3-15 |
| `abstraction_audit_workflow` | DatabaseCoupling, LlmCoupling, StorageCoupling, ConfigCoupling, QueueCoupling | 5-25 |
| `spec_refinement_workflow` | MissingFunction, MissingErrorHandler, IncompleteMatch, MissingTest | 0-10 |
| `spec_maintenance_workflow` | Documentation, Refactor | 0-5 |
| `tui_testing_workflow` | SpecDivergence, StateMismatch | 0-10 |

**Implementation workflows** (Section 3.3.2) emit `execution_metrics.json` instead:

| Workflow | Metrics |
|----------|---------|
| `feature_workflow` | prompts_used, outcome, validation_findings (counts only) |

### 13.3 Implementation Execution Flow

```
ACTIONABLE FINDINGS RECEIVED
            │
            ▼
┌───────────────────────────────────────────┐
│         1. TRIAGE & PRIORITIZE            │
│                                           │
│  Sort by:                                 │
│    1. Severity (CRITICAL first)           │
│    2. Dependency order (blocked_by)       │
│    3. File grouping (batch same-file)     │
│    4. Confidence (HIGH first)             │
│                                           │
│  Filter out:                              │
│    - LOW confidence without user approval │
│    - Findings with unresolved blockers    │
└───────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────┐
│         2. USER CONFIRMATION              │
│                                           │
│  Present implementation plan:             │
│    "Found 12 actionable items:            │
│     - 3 CRITICAL (memory leaks)           │
│     - 5 HIGH (unnecessary clones)         │
│     - 4 MEDIUM (dead code)                │
│                                           │
│    Estimated: 12 file edits, 4 files      │
│                                           │
│    [Proceed All] [Review Each] [Skip]"    │
└───────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────┐
│         3. IMPLEMENTATION LOOP            │
│                                           │
│  FOR each finding IN priority_order:      │
│    a. Read current file state             │
│    b. Apply suggested_fix (or generate)   │
│    c. Run verify_command                  │
│    d. IF verification fails:              │
│       - Rollback change                   │
│       - Log failure reason                │
│       - Continue to next (don't block)    │
│    e. IF verification passes:             │
│       - Mark finding RESOLVED             │
│       - Stage for commit                  │
└───────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────┐
│         4. VERIFICATION GATE              │
│                                           │
│  After all changes:                       │
│    a. cargo check (full project)          │
│    b. cargo test (full suite)             │
│    c. cargo clippy (no new warnings)      │
│                                           │
│  IF any fail:                             │
│    - Identify which change caused it      │
│    - Rollback that specific change        │
│    - Re-run verification                  │
│    - Repeat until green                   │
└───────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────┐
│         5. COMMIT GROUPING                │
│                                           │
│  Group changes into logical commits:      │
│                                           │
│  Strategy A: By category                  │
│    "fix(memory): remove unnecessary       │
│     clones in scanner module"             │
│                                           │
│  Strategy B: By file                      │
│    "refactor(scanner): memory and         │
│     dead code cleanup"                    │
│                                           │
│  Strategy C: Single commit                │
│    "fix: address memory audit findings"   │
│                                           │
│  (User preference or Manager decides)     │
└───────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────┐
│         6. COMPLETION REPORT              │
│                                           │
│  Implementation Summary:                  │
│    - Attempted: 12                        │
│    - Succeeded: 10                        │
│    - Failed: 2 (logged for manual review) │
│    - Commits created: 3                   │
│                                           │
│  Failed findings saved to:                │
│    implementation_failures.json           │
└───────────────────────────────────────────┘
```

### 13.4 Rollback Protocol

```
WHEN implementation_fails(finding):
    1. CAPTURE error output and context
    2. REVERT file to pre-change state (git checkout or cached copy)
    3. RECORD failure:
       {
         finding_id: finding.id,
         attempted_change: diff,
         error_type: "verification_failed" | "edit_failed" | "conflict",
         error_detail: stderr or exception,
         file_state_hash: hash_of_reverted_file
       }
    4. IF finding.severity == CRITICAL:
       - PAUSE implementation
       - ASK user: [Skip] [Retry with different approach] [Manual fix]
    5. ELSE:
       - LOG and continue to next finding
       - Include in completion report
```

### 13.5 Conflict Resolution

When a finding's target code has changed since the workflow ran:

```
CONFLICT DETECTION:
    stored_hash = finding.file_hash_at_discovery
    current_hash = hash(read_file(finding.file_path))

    IF stored_hash != current_hash:
        # File changed since finding was created

        IF finding.current_code IN read_file(finding.file_path):
            # Target code still exists, just moved
            UPDATE finding.line_start, finding.line_end
            PROCEED with implementation
        ELSE:
            # Target code no longer exists or changed significantly
            MARK finding as STALE
            LOG: "Finding {id} target code no longer present"
            SKIP implementation
```

### 13.6 Parallel Implementation

For independent findings (no shared files, no dependencies):

```
PARALLEL IMPLEMENTATION:
    independent_groups = group_by_file(findings)

    # Files with single finding can run in parallel
    parallel_batch = [g FOR g IN independent_groups IF len(g) == 1]

    # Files with multiple findings run sequentially within file
    sequential_batches = [g FOR g IN independent_groups IF len(g) > 1]

    # Execute
    PARALLEL FOR group IN parallel_batch:
        implement_finding(group[0])

    FOR group IN sequential_batches:
        FOR finding IN group:
            implement_finding(finding)

    # Single verification gate at end
    run_full_verification()
```

### 13.7 Learning from Implementation

The Manager tracks implementation outcomes for workflow improvement:

```rust
struct ImplementationOutcome {
    finding_id: String,
    source_workflow: String,
    category: FindingCategory,

    // Outcome
    succeeded: bool,
    attempts: u32,
    failure_reason: Option<String>,

    // Metrics
    time_to_implement_ms: u64,
    verification_time_ms: u64,
    lines_changed: u32,

    // Quality signals
    suggested_fix_used: bool,      // Did we use the workflow's suggestion?
    fix_modified: bool,            // Did we have to adjust it?
    caused_regression: bool,       // Did verification catch a problem?
}
```

Patterns the Manager detects:

| Pattern | Trigger | Workflow Improvement |
|---------|---------|---------------------|
| Low success rate for category | <70% success for FindingCategory | Improve detection criteria in source workflow |
| Suggested fixes rarely used | <50% suggested_fix_used | Improve fix generation in source workflow |
| High modification rate | >60% fix_modified | Suggested fixes too generic |
| Frequent conflicts | >30% STALE findings | Workflow running on outdated code |
| Slow verification | verify_command takes >30s | Suggest more targeted test command |

---

## 14. Contract Enforcement via Spec Maintenance

Rather than manually ensuring each workflow conforms to the Manager's contract, **leverage the existing `spec_maintenance_workflow`** to enforce compliance automatically.

### 14.1 The Meta-Pattern

```
┌─────────────────────────────────────────────────────────────────┐
│                    workflow_manager.md                          │
│                                                                 │
│  Section 13: ActionableFinding schema (the CONTRACT)            │
│  Section 13.2: Per-workflow output requirements                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ "Workflows must conform to this"
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                  spec_maintenance_workflow                      │
│                                                                 │
│  Corpus: specs/meta/*_workflow.md                               │
│  Contract: workflow_manager.md Section 13                       │
│  Check: Each workflow defines conformant output                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Gap Report
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  "memory_audit_workflow.md missing actionable_findings.json     │
│   output section per workflow_manager.md Section 13.2"          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Manager routes to spec_refinement
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                 spec_refinement_workflow                     │
│                                                                 │
│  Target: memory_audit_workflow.md                               │
│  Gap: Add output section conforming to contract                 │
└─────────────────────────────────────────────────────────────────┘
```

### 14.2 Contract Definition

The Manager's contract for workflow outputs is defined in Section 13.1-13.2. For spec_maintenance to enforce it, we express the contract as checkable requirements:

```yaml
# workflow_contract.yaml (conceptual - embedded in workflow_manager.md)

required_sections:
  - name: "Output Artifacts"
    must_contain:
      - "actionable_findings.json"  # Unless exempt

  - name: "Finding Categories"
    must_define:
      - category_enum: "Must list valid FindingCategory values"
      - severity_mapping: "Which categories map to which severity"

  - name: "Verification"
    must_define:
      - verify_command_pattern: "How to construct verify_command"

exempt_workflows:
  - tui_testing_workflow  # Reports failures, doesn't prescribe fixes
```

### 14.3 Spec Maintenance Configuration

When the Manager invokes spec_maintenance for contract enforcement:

```
spec_maintenance_workflow
  --corpus "specs/meta/*_workflow.md"
  --contract "workflow_manager.md#section-13"
  --mode "contract_compliance"
```

**Phase mapping:**

| Spec Maintenance Phase | Contract Enforcement Action |
|------------------------|----------------------------|
| Phase 1: Inventory | List all `*_workflow.md` files |
| Phase 2: Alignment | Check each workflow against contract |
| Phase 3: Cross-Spec | Check category enums match across workflows |
| Phase 4: Recommendations | Generate gaps for non-conformant workflows |
| Phase 5: Execution | Route to spec_refinement for fixes |

### 14.4 Compliance Checklist (Auto-Generated)

For each workflow, spec_maintenance checks:

```
□ OUTPUT_SECTION
  └─ Has "Output Artifacts" or "Output Files" section?
  └─ Lists actionable_findings.json (unless exempt)?
  └─ Specifies output location?

□ CATEGORY_ENUM
  └─ Defines finding categories?
  └─ Categories are valid FindingCategory variants?
  └─ No undefined categories?

□ SCHEMA_COMPLIANCE
  └─ Output matches ActionableFinding struct?
  └─ Required fields documented (id, category, severity, file_path)?
  └─ Optional fields noted (suggested_fix, verify_command)?

□ VERIFICATION_PATTERN
  └─ Documents how to construct verify_command?
  └─ Provides fallback command?

□ HANDOFF_READINESS
  └─ Output location is predictable (session directory)?
  └─ Manager can find output without parsing workflow internals?
```

### 14.5 Self-Healing Loop

The Manager can automatically fix non-conformant workflows:

```
1. Manager detects need to invoke memory_audit_workflow
2. Manager runs spec_maintenance on workflow corpus (periodic or on-demand)
3. spec_maintenance finds: "memory_audit_workflow missing output section"
4. Manager routes to spec_refinement_workflow:
   - Target: memory_audit_workflow.md
   - Gap: CONTRACT_COMPLIANCE - missing actionable_findings.json output
5. spec_refinement adds the required section
6. Manager re-validates (spec_maintenance confirms fix)
7. Manager proceeds with memory_audit_workflow (now conformant)
```

### 14.6 Contract Versioning

When the Manager's contract evolves:

```
workflow_manager.md Section 13 changes
              │
              ▼
spec_maintenance detects contract drift
              │
              │ "5 workflows reference contract v1.0,
              │  but workflow_manager.md is v1.1"
              ▼
Manager options:
  [1] Update all workflows to v1.1 (batch spec_refinement)
  [2] Mark contract change as non-breaking (backwards compatible)
  [3] Deprecate v1.0, require migration
```

### 14.7 Benefits

| Benefit | How |
|---------|-----|
| **Single source of truth** | Contract lives in workflow_manager.md only |
| **Automated enforcement** | spec_maintenance runs periodically |
| **Self-documenting gaps** | Non-compliance becomes a trackable gap |
| **Composable fix** | spec_refinement fixes workflow specs |
| **Future-proof** | New workflows auto-checked on creation |
| **Meta-consistency** | Same tools that maintain code specs maintain workflow specs |

---

## 15. Appendix: Workflow Quick Reference

### A. Spec Refinement Workflow v2
- **Instances:** 3 (Engineer, Reviewer, Mediator)
- **Trigger:** New/incomplete spec
- **Output:** Refined spec, status.md with gaps
- **Handoff data:** Gap inventory, spec version

### B. Spec Maintenance Workflow
- **Instances:** 1
- **Trigger:** Periodic audit, post-change
- **Output:** Cross-reference report, recommendations
- **Handoff data:** Spec inventory, alignment issues

### C. Memory Audit Workflow
- **Instances:** 3 (Analyst, Validator, Coordinator)
- **Trigger:** Performance concerns
- **Output:** Optimization recommendations
- **Handoff data:** Hot types, allocation sites

### D. Data Model Maintenance Workflow
- **Instances:** 1
- **Trigger:** Schema evolution, tech debt
- **Output:** Model health report, refactor plan
- **Handoff data:** Type inventory, usage patterns

### E. TUI Testing Workflow
- **Instances:** 1
- **Trigger:** TUI changes
- **Output:** Test results, scenario coverage
- **Handoff data:** Key behaviors, test scenarios

### F. Spec-Driven Feature Workflow
- **Instances:** 1 orchestrator (spawns sub-workflows)
- **Trigger:** Feature requiring spec changes first
- **Output:** Updated specs + implementation + validation results
- **Phases:** Proposal → Spec Update → Refinement → Alignment → Implementation → Validation → Commit
- **Sub-workflows:** spec_refinement, spec_maintenance (conditional), tui_testing (conditional), memory_audit (conditional)
- **Handoff data:** Session artifacts (proposal.md, diffs, logs, SUMMARY.md)

### G. Feature Workflow
- **Instances:** 1 with escalation
- **Trigger:** New feature, bug fix, refactor
- **Output:** Code changes, execution_metrics.json
- **Handoff data:** Files modified, validation results

---

## 16. Concrete Usage Examples

This section provides practical examples of invoking and using the Workflow Manager.

### Example 1: Simple Single-Workflow Request

**User Request:**
```
"Run a memory audit on the casparian crate"
```

**Manager Analysis:**
```
Signals detected:
  - "memory audit" → CODE_QUALITY.MEMORY_CONCERNS
  - "casparian crate" → scope parameter

Routing decision:
  - Primary workflow: memory_audit_workflow
  - No prerequisites needed
  - No successor workflows detected
```

**Manager Response:**
```
I'll run a memory audit on the casparian crate.

**Execution Plan:**
1. `memory_audit_workflow`
   - Scope: crates/casparian/
   - Estimated: 3-5 rounds
   - Output: Optimization recommendations + actionable_findings.json

[Proceed] [Adjust Scope] [Cancel]
```

**User Confirms → Execution:**
```
Starting memory_audit_workflow...

Round 1: Analyst scanning crates/casparian/src/
  - Found 12 files, 4,200 LOC
  - Identified 3 hot paths (scanner, tui, cli)

Round 2: Analyst findings
  - 8 potential optimizations identified
  - Categories: UnnecessaryClone(4), AllocationInLoop(2), HeapAvoidable(2)

Round 3: Validator review
  - 6/8 findings validated
  - 2 rejected (false positives)

Round 4: Coordinator synthesis
  - Final report generated
  - actionable_findings.json written

**Workflow Complete**

Results:
  - 6 actionable findings (2 HIGH, 3 MEDIUM, 1 LOW)
  - Files: specs/meta/sessions/memory_audit/2026-01-14_abc123/
    - report.md
    - actionable_findings.json

[View Report] [Implement Findings] [Done]
```

---

### Example 2: Multi-Workflow Sequence

**User Request:**
```
"Refine the discover spec and then test the TUI implementation"
```

**Manager Analysis:**
```
Signals detected:
  - "refine" + "spec" → SPEC_WORK.REFINE_SPEC
  - "discover" → target spec
  - "then" → sequence indicator
  - "test" + "TUI" → TUI_WORK.TUI_VALIDATION

Routing decision:
  - Workflow 1: spec_refinement_workflow (target: specs/views/discover.md)
  - Workflow 2: tui_testing_workflow (context from workflow 1)
  - Sequence: spec_refinement → tui_testing
```

**Manager Response:**
```
I've identified a 2-workflow sequence:

**Execution Plan:**

1. `spec_refinement_workflow`
   - Target: specs/views/discover.md
   - Estimated: 3-5 rounds
   - Output: Refined spec, gap inventory

        ↓ (context handoff: key behaviors, state transitions)

2. `tui_testing_workflow`
   - Scope: Discover mode TUI
   - Input: Behaviors from refined spec
   - Output: Test scenarios, pass/fail results

[Proceed with Sequence] [Run Spec Refinement Only] [Run TUI Testing Only] [Cancel]
```

**Execution with Context Handoff:**
```
=== Workflow 1: spec_refinement_workflow ===

Round 1-3: [Engineer/Reviewer/Mediator cycles]
...

Workflow 1 Complete.
  - 7 gaps resolved
  - Spec updated to v1.4
  - Key behaviors identified:
    * State: Browse → Filtering → EditRule → Testing → Publishing
    * Key bindings: /, e, t, p, Enter, Escape
    * Panels: Sources, Tags, Files, Preview

Preparing context for tui_testing_workflow...

=== Workflow 2: tui_testing_workflow ===

Context received from spec_refinement:
  - State machine: 6 states
  - Key behaviors: 12 transitions
  - Expected outputs: Panel content patterns

Running test scenarios...

Scenario 1: Browse → Filtering (press /)
  ✓ Filter input appears
  ✓ Cursor visible

Scenario 2: Filtering → EditRule (press e)
  ✓ Rule editor opens
  ✓ 4 sections visible

[12 scenarios total, 11 passed, 1 failed]

**Sequence Complete**

Failed scenario:
  - Scenario 8: Publishing → Published (press Enter)
  - Expected: Success screen with job_id
  - Actual: Error dialog "Database write failed"

[View Full Results] [Re-run Failed] [Done]
```

---

### Example 3: Implementation Protocol in Action

**Continuing from Example 1:**
```
User: "Implement the findings from the memory audit"
```

**Manager Response:**
```
I found 6 actionable findings from the memory audit:

**Implementation Plan:**

| # | Finding | Severity | Confidence | File |
|---|---------|----------|------------|------|
| 1 | Remove unnecessary clone | HIGH | HIGH | scanner.rs:145 |
| 2 | Remove unnecessary clone | HIGH | HIGH | scanner.rs:203 |
| 3 | Move allocation out of loop | MEDIUM | HIGH | tui/app.rs:892 |
| 4 | Use ArrayVec instead of Vec | MEDIUM | MEDIUM | cli/scan.rs:67 |
| 5 | Remove unnecessary clone | MEDIUM | HIGH | tui/ui.rs:1205 |
| 6 | Consider arena allocation | LOW | LOW | scout/mod.rs:340 |

**Proposed execution:**
- AUTO-IMPLEMENT: #1, #2, #3, #5 (HIGH confidence)
- REVIEW FIRST: #4 (MEDIUM confidence, architectural)
- SKIP: #6 (LOW confidence, needs design decision)

Verification: `cargo check && cargo test -p casparian`

[Proceed] [Review Each] [Skip All]
```

**User: "Proceed"**
```
Implementing findings...

[1/4] scanner.rs:145 - Remove clone
  ✓ Applied: `let files_copy = files.clone();` → `// removed, using files directly`
  ✓ Verified: cargo check passed

[2/4] scanner.rs:203 - Remove clone
  ✓ Applied: `.clone()` removed from iterator
  ✓ Verified: cargo check passed

[3/4] tui/app.rs:892 - Move allocation out of loop
  ✓ Applied: `let mut buffer = Vec::new();` moved before loop
  ✓ Verified: cargo check passed

[4/4] tui/ui.rs:1205 - Remove clone
  ✓ Applied: Changed to borrow instead of clone
  ✓ Verified: cargo check passed

Running full verification...
  ✓ cargo check: passed
  ✓ cargo test -p casparian: 47 tests passed
  ✓ cargo clippy: no new warnings

**Implementation Complete**

Commit grouping options:
1. [Single commit] "fix(memory): remove unnecessary clones and optimize allocations"
2. [By file] 3 commits (scanner.rs, tui/app.rs, tui/ui.rs)
3. [By category] 2 commits (clones, allocations)

[Option 1] [Option 2] [Option 3] [Don't Commit]
```

---

### Example 4: Contract Compliance Check

**User Request:**
```
"Check if all workflows conform to the manager contract"
```

**Manager Analysis:**
```
Signals detected:
  - "workflows" + "conform" + "contract" → META.WORKFLOW_REVIEW

Routing decision:
  - Use spec_maintenance_workflow in contract_compliance mode
  - Corpus: specs/meta/*_workflow.md
  - Contract: workflow_manager.md Section 13
```

**Manager Response:**
```
Running contract compliance check...

spec_maintenance_workflow --mode contract_compliance
  --corpus "specs/meta/*_workflow.md"
  --contract "workflow_manager.md#section-13"

Scanning corpus...
  - Found 5 workflow specs

Checking compliance...

**Compliance Report:**

| Workflow | Status | Gaps |
|----------|--------|------|
| spec_refinement_workflow | PARTIAL | 1 |
| spec_maintenance_workflow | COMPLIANT | 0 |
| memory_audit_workflow | PARTIAL | 2 |
| data_model_maintenance_workflow | PARTIAL | 1 |
| tui_testing_workflow | EXEMPT | 0 |

**Gaps Found:**

1. `memory_audit_workflow.md`
   - GAP-CONTRACT-001: Missing "Output Artifacts" section
   - GAP-CONTRACT-002: No verify_command pattern documented

2. `data_model_maintenance_workflow.md`
   - GAP-CONTRACT-003: Output section missing actionable_findings.json

3. `spec_refinement_workflow.md`
   - GAP-CONTRACT-004: No IMPL_* gap filtering for actionable findings

[Fix All Gaps] [Review Each] [Ignore]
```

**User: "Fix All Gaps"**
```
Routing gaps to spec_refinement_workflow...

=== Fixing GAP-CONTRACT-001, GAP-CONTRACT-002 ===
Target: memory_audit_workflow.md

Round 1: Engineer adds Output Artifacts section
Round 2: Reviewer validates format matches contract
Round 3: Complete

=== Fixing GAP-CONTRACT-003 ===
Target: data_model_maintenance_workflow.md

Round 1: Engineer adds actionable_findings.json to output
Round 2: Complete

=== Fixing GAP-CONTRACT-004 ===
Target: spec_refinement_workflow.md

Round 1: Engineer adds IMPL_* gap filtering logic
Round 2: Complete

**All gaps resolved.**

Re-running compliance check...
  ✓ All workflows now COMPLIANT (except EXEMPT)

Files modified:
  - specs/meta/memory_audit_workflow.md (+45 lines)
  - specs/meta/data_model_maintenance_workflow.md (+12 lines)
  - specs/meta/spec_refinement_workflow.md (+18 lines)

[View Diffs] [Commit Changes] [Done]
```

---

### Example 5: Learning Engine Pattern Detection

**After Several Sessions:**
```
Manager: Pattern Report Available

I've detected patterns across recent workflow executions:

**Efficiency Patterns:**

1. PARALLELIZABLE_PHASES (HIGH confidence)
   - Observed: 8 times in spec_refinement_workflow
   - Finding: Engineer and Reviewer phases have no dependencies until Mediator
   - Potential: 40% time reduction per round
   - [View Proposal]

**Friction Patterns:**

2. FREQUENT_USER_QUESTIONS (MEDIUM confidence)
   - Observed: 5 times across workflows
   - Location: Phase 2 of memory_audit_workflow
   - Question: "Should I include test files in analysis?"
   - Suggestion: Add default scope exclusion for test files
   - [View Proposal]

**Gap Patterns:**

3. RECURRING_GAP_TYPES (HIGH confidence)
   - Observed: 12 times in spec_refinement sessions
   - Gap type: MISSING_ERROR_HANDLER
   - Suggestion: Add error handling checklist to Engineer prompt
   - [View Proposal]

[View All Proposals] [Dismiss] [Configure Alerts]
```

**User: "View Proposal" for #1**
```
## Workflow Amendment Proposal

**ID:** WA-2026-003
**Target:** spec_refinement_workflow.md
**Type:** PARALLELIZATION
**Confidence:** HIGH (8 observations)

### Current Behavior
Round N:
  1. Engineer analyzes gaps (4 min avg)
  2. Reviewer validates findings (3 min avg)
  3. Mediator reconciles (2 min avg)
Total: 9 min per round

### Proposed Change
Round N:
  1a. Engineer analyzes gaps ──┬── PARALLEL (4 min)
  1b. Reviewer validates      ──┘
  2. Mediator reconciles (2 min)
Total: 6 min per round (-33%)

### Evidence
- Session 2026-01-10: Engineer 4.2min, Reviewer 3.1min, no blocking
- Session 2026-01-11: Engineer 3.8min, Reviewer 2.9min, no blocking
- Session 2026-01-12: Engineer 5.1min, Reviewer 3.5min, no blocking
- [5 more sessions...]

### Risk Assessment
- **Risk:** LOW - no data dependencies identified
- **Rollback:** Easy - revert to sequential if issues

[APPROVE] [DEFER] [REJECT with reason]
```

---

### Example 6: Error Recovery

**During Workflow Execution:**
```
Running spec_refinement_workflow...

Round 2: Engineer phase

ERROR DETECTED:
  - Signal: ROUND_STALL
  - Phase: Engineer analysis
  - Issue: Grep tool returned 0 results for expected pattern
  - Likely cause: File structure changed since session start

Manager intervention:

The Engineer phase stalled because the expected file
`crates/casparian/src/parser.rs` no longer exists.

Git log shows: File renamed to `crates/casparian/src/parser/mod.rs`
(commit abc123, 2 hours ago)

Options:
1. [Update references] - Re-scan codebase, update file paths
2. [Restart session] - Fresh start with current file structure
3. [Pause] - Save checkpoint, investigate manually
4. [Abort] - Cancel workflow

[1] [2] [3] [4]
```

**User selects "Update references"**
```
Re-scanning codebase...
  - Found renamed file: parser.rs → parser/mod.rs
  - Updating 3 references in session context

Resuming Engineer phase with updated context...

Round 2 (resumed): Engineer analysis
  ✓ Successfully found patterns in parser/mod.rs
  ✓ Phase complete

Continuing to Round 2: Reviewer phase...
```

---

### Example 7: Full Audit Request

**User Request:**
```
"Do a full code quality audit - memory, data models, and specs"
```

**Manager Analysis:**
```
Signals detected:
  - "full audit" → compound workflow
  - "memory" → memory_audit_workflow
  - "data models" → data_model_maintenance_workflow
  - "specs" → spec_maintenance_workflow

Routing decision:
  - 3 workflows identified
  - Dependency analysis:
    * memory_audit and data_model can run in PARALLEL (no dependencies)
    * spec_maintenance should run AFTER (may need findings from above)
```

**Manager Response:**
```
I'll run a comprehensive code quality audit.

**Execution Plan:**

Phase 1 (Parallel):
┌─────────────────────┐  ┌─────────────────────────┐
│ memory_audit        │  │ data_model_maintenance  │
│ (3-5 rounds)        │  │ (5 phases)              │
└─────────────────────┘  └─────────────────────────┘
           │                        │
           └──────────┬─────────────┘
                      ▼
              Context merge

Phase 2 (Sequential):
┌─────────────────────────────────────────────────┐
│ spec_maintenance_workflow                       │
│ (includes findings from Phase 1 if relevant)   │
└─────────────────────────────────────────────────┘

Estimated total: 45-60 minutes

[Proceed] [Run Sequentially Instead] [Select Individual] [Cancel]
```

**User: "Proceed"**
```
=== Phase 1: Parallel Execution ===

Starting 2 workflows in parallel...

[memory_audit]              [data_model_maintenance]
Round 1: Scanning...        Phase 1: Inventory...
Round 2: Analysis...        Phase 2: Usage...
Round 3: Validation...      Phase 3: Cross-model...
Round 4: Complete ✓         Phase 4: Recommendations...
                            Phase 5: Complete ✓

Phase 1 complete (18 minutes)
  - memory_audit: 8 findings
  - data_model: 5 findings

Merging context for Phase 2...
  - Types identified in memory audit linked to data model findings
  - 2 overlapping recommendations consolidated

=== Phase 2: spec_maintenance_workflow ===

Running with context from Phase 1...

Phase 1-5: [standard spec maintenance]

**Full Audit Complete** (52 minutes total)

Summary:
| Workflow | Findings | Actionable |
|----------|----------|------------|
| memory_audit | 8 | 6 |
| data_model_maintenance | 5 | 4 |
| spec_maintenance | 3 | 2 |
| **Total** | **16** | **12** |

[View Combined Report] [Implement All] [Implement by Workflow] [Done]
```

---

### Example 8: Feature Workflow (Simple → Complex Escalation)

**User Request (Simple Feature):**
```
"Add a keyboard shortcut to export the current view"
```

**Manager Analysis:**
```
Signals detected:
  - "add" + "keyboard shortcut" → FEATURE_WORK.NEW_FEATURE
  - "export" + "view" → TUI context

Routing decision:
  - Primary workflow: feature_workflow
  - Context load: specs/views/discover.md, specs/tui_style_guide.md
  - Complexity estimate: Simple (1-2 files)
```

**Manager Response:**
```
I'll add a keyboard shortcut for export using feature_workflow.

**Context Loaded:**
- specs/views/discover.md (keybindings)
- specs/tui_style_guide.md (key conventions)

**Design Phase (1 prompt):**

Inline Spec:
┌─────────────────────────────────────────────────┐
│ FEATURE: Export Shortcut                        │
│                                                 │
│ Key: 'x' (matches export convention in codebase)│
│ Behavior: Open export dialog with current files │
│ State: Available in Browse, Filtering states    │
│ Files: tui/app.rs (handler), tui/ui.rs (dialog) │
└─────────────────────────────────────────────────┘

Escalation check: 2 files, follows existing pattern → No escalation

[Proceed] [Refine Design] [Cancel]
```

**User: "Proceed"**
```
=== Phase 2: Implement (2 prompts) ===

[1/2] Adding key handler to tui/app.rs:2145
  ✓ 'x' key opens ExportDialog state

[2/2] Adding export dialog render to tui/ui.rs
  ✓ Dialog with file list and format options

=== Phase 3: Validate (parallel) ===

Running concurrent checks...
  ✓ Memory: No unnecessary clones
  ✓ Style: Follows keybinding conventions
  ✓ Types: No new warnings

=== Phase 4: Fix ===

No issues found. Skipping.

**Feature Complete** (3 prompts total)

Files modified:
  - crates/casparian/src/cli/tui/app.rs (+12 lines)
  - crates/casparian/src/cli/tui/ui.rs (+45 lines)

[Commit] [Test Manually] [Done]
```

**Example with Escalation:**

```
User: "Add a plugin marketplace to the TUI"
```

**Manager Analysis:**
```
Signals detected:
  - "add" + "plugin marketplace" → FEATURE_WORK.NEW_FEATURE
  - Complexity indicators: new architectural pattern, API integration

Routing decision:
  - Primary workflow: feature_workflow
  - Escalation likely: New architectural pattern detected
```

**Execution:**
```
=== Phase 1: Design ===

Inline Spec (Draft):
┌─────────────────────────────────────────────────────────────┐
│ FEATURE: Plugin Marketplace                                  │
│                                                              │
│ New state: MarketplaceBrowse                                 │
│ API: Plugin registry endpoint (TBD)                         │
│ UI: Search, categories, install button                      │
│ Files: 8+ files (new module, API client, TUI state, tests) │
└─────────────────────────────────────────────────────────────┘

**ESCALATION TRIGGERED**
Reason: >5 files, introduces new architectural pattern

Escalating to spec_refinement_workflow (3 rounds max)...

=== spec_refinement_workflow ===

Round 1: Engineer drafts marketplace.md spec
Round 2: Reviewer identifies API contract gaps
Round 3: Mediator resolves, spec complete

Returning to feature_workflow Phase 2...

=== Phase 2: Implement (3 prompts) ===
[Following approved spec...]
```

---

### Quick Reference: Manager Commands

These are **natural language commands** - just say them to Claude:

| Say This | Claude Does |
|----------|-------------|
| "Add feature X" | Runs feature_workflow |
| "Fix bug in Y" | Runs feature_workflow (simplified) |
| "Run the spec maintenance workflow" | Audits all specs |
| "Audit memory in casparian" | Runs memory_audit_workflow |
| "Check workflow compliance" | Runs contract compliance check |
| "Implement the findings from session X" | Runs implementation protocol |
| "What's the workflow status?" | Shows current execution |

---

### Invocation Patterns

**Natural Language (Recommended):**
```
"Run a memory audit on the scanner module"
"Refine the discover spec"
"Audit all specs for alignment issues"
"Check if workflows conform to the manager contract"
```

**With "workflow" Keyword (Explicit):**
```
"workflow: audit all specs"
"workflow manager: run memory audit"
"workflow: check compliance"
```

**Implicit (Claude Detects Intent):**
```
"Add a dark mode toggle"                   → feature_workflow
"Fix the crash when exporting"             → feature_workflow
"Check if the TUI matches the spec"        → tui_testing
"Clean up the data models"                 → data_model_maintenance
"The parser is using too much memory"      → memory_audit
"Refine the export spec and implement it"  → spec_refinement → implementation
```

**Note:** The `/workflow` slash command requires the skill to be registered in `.claude/skills/workflow.md`. If it's not available, use natural language instead.

---

## 17. Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-01-14 | Initial specification |
| 1.1.0 | 2026-01-14 | Added Section 7.4 Single-Execution Review |
| 1.2.0 | 2026-01-14 | Added Section 13 Implementation Protocol |
| 1.3.0 | 2026-01-14 | Added Section 14 Contract Enforcement via Spec Maintenance |
| 1.4.0 | 2026-01-14 | Added Section 3.3 Workflow Categories (Analysis vs Implementation). Clarified that `actionable_findings.json` applies only to Analysis workflows. Implementation workflows emit `execution_metrics.json` instead. |
| 1.5.0 | 2026-01-14 | Merged Section 16 "Concrete Usage Examples" from workflow_manager_examples.md. |
| 1.6.0 | 2026-01-14 | Added `spec_driven_feature_workflow` to registry (Section 3.2), categories (Section 3.3.2), routing (Section 4.1, 4.3), and appendix (Section 15.F). |
