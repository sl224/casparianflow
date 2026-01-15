# Feature Workflow

**Type:** Meta-specification (LLM Process Template)
**Version:** 1.2
**Purpose:** Optimized feature development with minimal prompts and maximum quality
**Related:** spec_refinement_workflow.md (escalation target), code_execution_workflow.md (coding standards)

---

## 1. Overview

This workflow optimizes the feature development cycle by:
1. **Front-loading design** - Catch issues before code is written
2. **Smart context loading** - Load only relevant specs, not everything
3. **Parallel validation** - Run checks concurrently after implementation
4. **Minimal round-trips** - 3-7 prompts for most features vs 10-20 in sequential approach

### 1.1 Design Principles

1. **Context Efficiency** - Load the right specs at the right time, not all specs all the time
2. **Early Validation** - Design issues caught in Phase 1, not during code review
3. **Parallelization** - Independent validations run concurrently
4. **Escalation, Not Default** - Full spec_refinement only when complexity warrants
5. **Inline Over Files** - Simple features don't need separate spec files

### 1.2 When to Use

| Scenario | Use This Workflow? |
|----------|-------------------|
| New feature (any size) | Yes |
| Bug fix | Yes (simplified) |
| Refactoring | Yes |
| New architectural pattern | Yes (will escalate to refinement) |
| Pure research/exploration | No (use Explore agent) |
| Spec-only work | No (use spec_refinement directly) |

### 1.3 Prompt Budget

| Feature Complexity | Expected Prompts | Breakdown |
|-------------------|------------------|-----------|
| Simple (1-2 files) | 2-3 | Design(1) + Implement(1-2) |
| Medium (3-5 files) | 4-5 | Design(1) + Implement(2) + Validate(1) + Fix(1) |
| Complex (5+ files) | 6-8 | Design(1) + Refinement(3) + Implement(2) + Validate(1) + Fix(1) |

---

## 2. Execution Model

### 2.1 Single-Instance with Escalation

```
┌─────────────────────────────────────────────────────────────────────┐
│                      FEATURE WORKFLOW                               │
│                    (Single Claude Instance)                         │
│                                                                     │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐      │
│  │ Phase 1  │───>│ Phase 2  │───>│ Phase 3  │───>│ Phase 4  │      │
│  │ Design   │    │Implement │    │ Validate │    │   Fix    │      │
│  └────┬─────┘    └──────────┘    └──────────┘    └──────────┘      │
│       │                                                             │
│       │ Escalate if complex                                         │
│       ▼                                                             │
│  ┌──────────────────────────────┐                                   │
│  │   spec_refinement_workflow   │                                   │
│  │   (3-instance, 3 rounds max) │                                   │
│  └──────────────────────────────┘                                   │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Phase Summary

| Phase | Purpose | Prompts | Parallel |
|-------|---------|---------|----------|
| 1. Design | Understand, plan, create inline spec | 1 | No |
| 1b. Refinement | (Optional) Resolve complex design | 3 | No |
| 2. Implement | Write code and tests | 1-3 | No |
| 3. Validate | Memory, style, type checks | 1 | Yes (internal) |
| 4. Fix | Address validation findings | 0-1 | No |

### 2.3 State Machine

Per `spec_refinement_workflow.md` Section 15, workflows with user flows must include state machine documentation.

#### 2.3.1 State Diagram

```
                              ┌─────────────────────────────────────────┐
                              │                                         │
                              │    ┌─────────┐                          │
                   ┌──────────┼───>│ABANDONED│ (user cancels at any    │
                   │          │    └─────────┘  point)                  │
                   │          │                                         │
┌─────────┐   ┌────┴────┐   ┌─┴───────────┐   ┌──────────┐   ┌────────┐
│ PENDING │──>│ DESIGN  │──>│ REFINEMENT  │──>│IMPLEMENT │──>│VALIDATE│
└─────────┘   └────┬────┘   │ (optional)  │   └────┬─────┘   └───┬────┘
                   │        └─────────────┘        │             │
                   │                               │             │
                   │   (simple: skip refinement)   │             │
                   └───────────────────────────────┘             │
                                                                 │
                              ┌──────────────────────────────────┘
                              │
                              ▼
                   ┌──────────────────┐      ┌─────────┐
                   │    FIX           │─────>│COMPLETE │
                   │ (if findings)    │      └─────────┘
                   └──────────────────┘
                              │
                              │ (no findings: skip)
                              │
                   ┌──────────┴───────────────────────┐
                   │                                   │
                   ▼                                   │
             ┌─────────────┐                          │
             │PARTIAL_DONE │ (manual review needed)   │
             └─────────────┘                          │
                                                      │
             (max attempts exhausted)                 │
                   │                                   │
                   ▼                                   │
             ┌──────────┐                             │
             │  FAILED  │<────────────────────────────┘
             └──────────┘
```

#### 2.3.2 State Definitions

| State | Entry Condition | Exit Condition | Behavior |
|-------|-----------------|----------------|----------|
| PENDING | Workflow invoked | Context loaded | Load smart context based on keywords |
| DESIGN | Context loaded | Inline spec created | Analyze request, create spec, check escalation |
| REFINEMENT | Escalation triggered | Refined spec returned | Invoke spec_refinement (3 rounds max) |
| IMPLEMENT | Spec ready | Code written + tests pass | Write code, run check/test loops |
| VALIDATE | Implementation passes | All validations complete | Run parallel checks |
| FIX | Findings exist | Findings addressed | Apply auto-fixes, flag manual items |
| COMPLETE | No findings OR all fixed | Terminal | Emit success output + metrics |
| PARTIAL_DONE | Manual review items remain | Terminal | Emit partial output + metrics |
| FAILED | Max attempts exhausted | Terminal | Emit failure output + metrics |
| ABANDONED | User cancels | Terminal | No output emitted |

#### 2.3.3 Transitions

| From | To | Trigger | Guard |
|------|----|---------|-------|
| PENDING | DESIGN | Context loaded | - |
| DESIGN | REFINEMENT | Escalation criteria met | >5 files OR new pattern OR high uncertainty |
| DESIGN | IMPLEMENT | Spec ready, no escalation | Simple/medium complexity |
| REFINEMENT | IMPLEMENT | Refined spec returned | - |
| IMPLEMENT | VALIDATE | cargo check + test pass | - |
| IMPLEMENT | FAILED | Max attempts (3) exhausted | - |
| VALIDATE | FIX | Findings exist | findings.len() > 0 |
| VALIDATE | COMPLETE | No findings | findings.len() == 0 |
| FIX | COMPLETE | All findings resolved | manual_review.len() == 0 |
| FIX | PARTIAL_DONE | Manual items remain | manual_review.len() > 0 |
| * | ABANDONED | User cancels | - |

---

## 3. Smart Context Loading

### 3.1 Always Load

These are loaded for every feature:

```
ALWAYS_LOAD = [
    "CLAUDE.md",                           # Project context
    "code_execution_workflow.md",          # Coding standards (summary)
]
```

### 3.2 Keyword-Based Loading

Load additional specs based on feature request keywords:

```
KEYWORD_MAPPING = {
    # TUI-related
    ["tui", "terminal", "ui", "view", "render", "display"]: [
        "specs/tui_style_guide.md",
        "specs/tui.md",
    ],

    # Specific views
    ["discover", "file", "scan", "tag"]: ["specs/views/discover.md"],
    ["job", "queue", "process"]: ["specs/views/jobs.md"],
    ["parser", "bench", "test"]: ["specs/views/parser_bench.md"],
    ["source", "directory"]: ["specs/views/sources.md"],
    ["extract", "rule", "pattern"]: ["specs/extraction.md"],
    ["export", "parquet", "csv", "sink"]: ["specs/export.md"],

    # Backend systems
    ["memory", "allocat", "clone", "arena"]: ["specs/meta/memory_audit_workflow.md"],
    ["type", "struct", "enum", "model"]: ["specs/meta/data_model_maintenance_workflow.md"],

    # Workflows
    ["workflow", "manager"]: ["specs/meta/workflow_manager.md"],
}
```

### 3.3 Context Loading Algorithm

```
FUNCTION load_context(feature_request):
    context = ALWAYS_LOAD.copy()

    # Keyword matching
    request_lower = feature_request.lower()
    FOR keyword_group, specs IN KEYWORD_MAPPING:
        IF any(keyword IN request_lower FOR keyword IN keyword_group):
            context.extend(specs)

    # Deduplicate
    context = unique(context)

    # Size check - if too large, summarize non-essential specs
    IF total_tokens(context) > MAX_CONTEXT_TOKENS:
        context = prioritize_and_summarize(context)

    RETURN context
```

### 3.4 Context Size Limits

| Priority | Max Tokens | Strategy |
|----------|------------|----------|
| Essential (CLAUDE.md) | 4000 | Full load |
| Primary (matched specs) | 8000 | Full load |
| Secondary (style guides) | 2000 | Summary only |
| **Total budget** | **14000** | Fits in context with room for code |

---

## 4. Phase 1: Design

### 4.1 Purpose

Understand the feature request, create an implementation plan, and decide if escalation is needed.

### 4.2 Inputs

- Feature request (from user)
- Smart-loaded context (from Section 3)

### 4.3 Outputs

1. **Inline Spec** - Feature description, behavior, edge cases
2. **Implementation Plan** - Files to modify, order of changes
3. **Test Cases** - What to test, expected outcomes
4. **Escalation Decision** - Simple, medium, or complex

### 4.4 Inline Spec Template

For simple/medium features, produce this inline (not a separate file):

```markdown
## Feature: [Title]

### Summary
[1-2 sentences describing the feature]

### Behavior
- [Bullet points of expected behavior]
- [Include edge cases]

### Files to Modify
1. `path/to/file.rs` - [what changes]
2. `path/to/other.rs` - [what changes]

### Dependencies
- [Any new crates or dependencies needed]
- [Or "None"]

### Test Cases
1. `test_[feature]_[scenario]` - [what it tests]
2. `test_[feature]_[edge_case]` - [what it tests]

### Risks
- [Potential issues or things to watch for]
- [Or "None identified"]
```

### 4.5 Escalation Criteria

| Criterion | Threshold | Action |
|-----------|-----------|--------|
| Files to modify | > 5 | Escalate |
| New architectural pattern | Any | Escalate |
| Cross-subsystem changes | > 2 subsystems | Escalate |
| State machine changes | Any | Escalate |
| Public API changes | Breaking | Escalate |
| Uncertainty | High | Escalate |

### 4.6 Escalation Protocol

When escalation is triggered:

```
1. NOTIFY user: "This feature is complex. Running spec refinement (3 rounds max)."

2. INVOKE spec_refinement_workflow:
   - Target: Create/update spec for this feature
   - Max rounds: 3 (not default 7)
   - Output: Refined spec file

3. RESUME feature_workflow at Phase 2 with refined spec
```

#### 4.6.1 Escalation Context Schema

Pass to `spec_refinement_workflow`:

```json
{
  "source": "feature_workflow",
  "session_id": "FW-2026-XXX",
  "feature_request": "[Original user request]",
  "inline_spec_draft": "[Draft from Phase 1]",
  "escalation_reasons": ["new_pattern", "file_count_exceeded"],
  "affected_subsystems": ["tui", "scout"],
  "suggested_spec_path": "specs/views/new_feature.md",
  "max_rounds": 3,
  "context_files": [
    "specs/tui.md",
    "crates/casparian/src/cli/tui/app.rs"
  ]
}
```

#### 4.6.2 Expected Output Contract

Receive from `spec_refinement_workflow`:

```json
{
  "status": "COMPLETE | GOOD_ENOUGH | ABANDONED",
  "spec_path": "specs/views/new_feature.md",
  "key_sections": [
    {"section": "State Machine", "line": 45},
    {"section": "Keybindings", "line": 120}
  ],
  "implementation_order": [
    "Add state enum to app.rs",
    "Implement key handlers",
    "Add render function to ui.rs"
  ],
  "known_gaps": ["GAP-XXX deferred by user"],
  "rounds_used": 2
}
```

#### 4.6.3 Escalation Failure Handling

| Failure Mode | Detection | Recovery |
|--------------|-----------|----------|
| Refinement stalls | 3 rounds with no progress | Proceed with inline spec + warning |
| User abandons refinement | ABANDONED status returned | Proceed with inline spec + warning |
| Max rounds exhausted | rounds_used >= 3 | Accept GOOD_ENOUGH output |
| Spec conflicts with codebase | DIVERGENCE reported | Present divergence to user, pause |

**Recovery Process:**
```
IF spec_refinement returns ABANDONED:
    LOG "Refinement abandoned, using inline spec"
    SET spec = inline_spec_draft
    ADD warning to output: "Feature implemented from draft spec (refinement skipped)"
    CONTINUE to Phase 2

IF spec_refinement returns with known_gaps:
    LOG gaps for post-implementation review
    CONTINUE to Phase 2
```

---

## 5. Phase 2: Implement

### 5.1 Purpose

Write the code and tests according to the spec from Phase 1.

### 5.2 Inputs

- Inline spec (or refined spec file from escalation)
- Relevant source files (read before modifying)

### 5.3 Process

```
IMPLEMENTATION LOOP:

1. Read existing files mentioned in spec
2. Implement changes in order specified
3. Write tests
4. Run: cargo check
5. IF check fails:
   - Fix type errors
   - Repeat step 4 (max 3 attempts)
6. Run: cargo test (relevant tests only)
7. IF tests fail:
   - Fix failing tests
   - Repeat step 6 (max 3 attempts)
8. Output: List of changed files
```

### 5.4 Coding Standards Reference

During implementation, follow `code_execution_workflow.md`:

- No stringly types (use newtypes, enums)
- Propagate errors with `?`, not `unwrap()`
- Channels over locks for concurrency
- Test critical paths with real DBs

### 5.5 Output

```
IMPLEMENTATION COMPLETE

Files modified:
  - crates/casparian/src/cli/export.rs (+45 lines)
  - crates/casparian/src/export/mod.rs (+12 lines)

Files created:
  - crates/casparian/tests/export_dry_run.rs (new)

Verification:
  ✓ cargo check: passed
  ✓ cargo test export: 3 tests passed
```

### 5.6 Implementation Failure Handling

When implementation fails after max attempts, follow this protocol:

#### 5.6.1 Failure Types

| Failure | Trigger | Recovery |
|---------|---------|----------|
| CHECK_FAILED | `cargo check` fails 3x | Rollback changes, surface error |
| TEST_FAILED | `cargo test` fails 3x | Keep passing code, rollback failing |
| PARTIAL_IMPL | Some changes work, others don't | Commit partial, log remainder |
| BLOCKED | Missing dependency or access | Pause, request user input |

#### 5.6.2 Rollback Protocol

```
WHEN check fails 3 times:
    1. Save error messages for context
    2. Rollback ALL uncommitted changes
    3. Present to user:
       - Error summary
       - Files that would have been modified
       - Suggested investigation areas
    4. Transition to FAILED state

WHEN test fails 3 times:
    1. Identify which tests pass vs fail
    2. Rollback changes that cause failures
    3. Keep changes that don't break tests
    4. Present to user:
       - Partial success summary
       - Failing test names + errors
       - Suggested fixes
    5. Transition to PARTIAL_DONE state
```

#### 5.6.3 User Options on Failure

Present via interactive prompt:

```
## Implementation Failed

**Attempts:** 3/3
**Last Error:** [error message]

Options:
1. [Retry with different approach] - Claude tries alternative strategy
2. [Proceed with partial] - Accept what works, skip failures
3. [Provide hints] - User gives additional context
4. [Abort feature] - Cancel and rollback everything
```

**Option Behavior:**

| Option | Action |
|--------|--------|
| Retry with different approach | Reset attempt counter, try different pattern |
| Proceed with partial | Transition to PARTIAL_DONE with logged issues |
| Provide hints | User adds context, retry with new info |
| Abort feature | Transition to ABANDONED, full rollback |

---

## 6. Phase 3: Validate

### 6.1 Purpose

Run targeted validations in parallel to catch issues the implementation loop missed.

### 6.2 Validation Selection

Not all validations run for every feature. Select based on what changed:

| Changed Area | Validations to Run |
|--------------|-------------------|
| Any code | `cargo check`, `cargo clippy` |
| Hot path (loops, file I/O) | Memory pattern check |
| TUI code | Style guide compliance |
| CLI code | Help text verification |
| Public API | Breaking change check |
| New dependencies | Security audit |
| Database/storage/LLM code | Platform abstraction check |
| Any significant code (optional) | Philosophy review (see `code_philosophy_review_workflow.md`) |

### 6.3 Parallel Execution

```
VALIDATE (parallel):

    # Always run
    type_check = spawn(cargo check)
    lint_check = spawn(cargo clippy)

    # Conditional
    IF changed_files INTERSECTS hot_paths:
        memory_check = spawn(check_memory_patterns(changed_files))

    IF changed_files INTERSECTS tui_files:
        style_check = spawn(check_style_compliance(changed_files))

    IF changed_files INTERSECTS cli_files:
        help_check = spawn(verify_help_text(changed_files))

    IF changed_files INTERSECTS platform_files:
        abstraction_check = spawn(check_abstraction_patterns(changed_files))

    # Optional philosophy review (user-triggered or high complexity)
    IF user_preference("philosophy_review") OR lines_changed > 500:
        philosophy_check = spawn(code_philosophy_review(changed_files))

    # Wait for all
    results = await_all([type_check, lint_check, memory_check?, style_check?, help_check?, abstraction_check?, philosophy_check?])

    RETURN aggregate_findings(results)
```

### 6.4 Memory Pattern Check (Lightweight)

Instead of full `memory_audit_workflow`, do targeted pattern matching:

```
MEMORY PATTERNS TO CHECK:

1. CLONE_IN_LOOP:
   Pattern: `for .* in .* \{[\s\S]*?\.clone\(\)`
   Severity: HIGH

2. VEC_GROW_IN_LOOP:
   Pattern: `for .* \{[\s\S]*?\.push\(`
   Check: Is Vec pre-allocated?

3. STRING_CONCAT_IN_LOOP:
   Pattern: `for .* \{[\s\S]*?(format!|\+ .*&str)`
   Suggestion: Use String::with_capacity or join()

4. BOX_IN_HOT_PATH:
   Pattern: `Box::new` in files matching hot_path_patterns
   Severity: MEDIUM
```

### 6.5 Style Guide Compliance Check (Lightweight)

Instead of manual review, check key patterns:

```
STYLE PATTERNS TO CHECK:

1. FOCUS_COLORS:
   Focused border: Color::Cyan + BorderType::Double
   Unfocused border: Color::DarkGray + BorderType::Rounded

2. SELECTION_STYLE:
   Selected + focused: Color::White.bold().bg(Color::DarkGray)
   Selected + unfocused: Color::Cyan.bg(Color::Black)

3. SEMANTIC_COLORS:
   Success: Color::Green
   Error: Color::Red
   Warning: Color::Yellow
```

### 6.6 Platform Abstraction Check (Lightweight)

Instead of full `abstraction_audit_workflow`, check key coupling patterns on changed files:

```
PLATFORM FILES (trigger abstraction check):
  - **/storage.rs, **/db.rs, **/database/**
  - **/llm/**, **/provider/**, **/client/**
  - Files importing: sqlx, anthropic, openai, aws_sdk, azure_sdk

ABSTRACTION PATTERNS TO CHECK:

1. DB_SQLITE_SPECIFIC:
   Pattern: `sqlx::Sqlite|SqlitePool|SqliteRow`
   Severity: HIGH
   Fix: Use generic `Pool<DB>` or Database trait

2. DB_SQLITE_SYNTAX:
   Pattern: `AUTOINCREMENT|INTEGER PRIMARY KEY|pragma`
   Severity: MEDIUM
   Fix: Use database-agnostic SQL

3. LLM_VENDOR_SPECIFIC:
   Pattern: `anthropic::|ClaudeClient|openai::|OpenAI`
   Severity: HIGH
   Fix: Use LlmProvider trait abstraction

4. LLM_HARDCODED_MODEL:
   Pattern: `model\s*[=:]\s*"(claude|gpt|llama)`
   Severity: MEDIUM
   Fix: Model selection via config

5. STORAGE_HARDCODED_PATH:
   Pattern: `~/.casparian|/tmp/|/var/`
   Severity: MEDIUM
   Fix: Paths should come from config
```

**Pattern Source:** These patterns are shared with `abstraction_audit_workflow.md` Section 1.3.
The audit workflow uses them for full codebase scans; this uses them incrementally on changes.

### 6.7 Aggregated Findings Format

```markdown
## Validation Findings

### Summary
| Check | Status | Findings |
|-------|--------|----------|
| Type check | ✓ Pass | 0 |
| Lint check | ⚠ Warn | 2 |
| Memory patterns | ✓ Pass | 0 |
| Style compliance | ✗ Fail | 1 |
| Abstraction | ✓ Pass | 0 |

### Details

#### Lint Warnings (2)
1. `export.rs:45` - unused variable `config`
2. `export.rs:78` - consider using `if let` instead of `match`

#### Style Violations (1)
1. `ui.rs:234` - Unfocused border uses `Color::Gray` instead of `Color::DarkGray`
```

---

## 7. Phase 4: Fix

### 7.1 Purpose

Address findings from Phase 3 validation.

### 7.2 Condition

Phase 4 only runs if Phase 3 found issues:

```
IF findings.is_empty():
    SKIP Phase 4
    GOTO Completion
ELSE:
    RUN Phase 4
```

### 7.3 Process

```
FIX LOOP:

1. Group findings by file
2. For each file:
   a. Read current state
   b. Apply fixes in order (top to bottom line numbers)
   c. Verify fix doesn't break other code
3. Run: cargo check
4. Run: cargo test (affected tests only)
5. IF new failures:
   - Rollback problematic fix
   - Log for manual review
6. Output: Fixed files + any remaining issues
```

### 7.4 Fix Prioritization

| Severity | Action | Auto-fix? |
|----------|--------|-----------|
| Error (blocks compile) | Fix immediately | Yes |
| Warning (clippy) | Fix if simple | Yes |
| Style violation | Fix | Yes |
| Memory pattern | Fix if clear solution | Maybe |
| Suggestion | Log for later | No |

### 7.5 Unfixable Issues

Some findings can't be auto-fixed:

```
UNFIXABLE CONDITIONS:
- Architectural change required
- Multiple valid approaches
- Breaking change to public API
- Requires human decision

ACTION:
1. Log issue clearly
2. Present options to user
3. Wait for decision (or proceed with default if timeout)
```

---

## 8. Completion

### 8.1 Success Output

```
## Feature Complete

**Feature:** [Title]
**Prompts Used:** [N]
**Time:** [Duration]

### Changes
- `path/to/file.rs` (+45, -12)
- `path/to/other.rs` (+8)
- `tests/feature_test.rs` (new)

### Verification
✓ cargo check: passed
✓ cargo test: 15 passed
✓ cargo clippy: 0 warnings
✓ Style compliance: passed

### Next Steps
- [ ] Manual testing recommended for [specific scenario]
- [ ] Consider adding integration test for [edge case]
```

### 8.2 Partial Success Output

```
## Feature Partially Complete

**Feature:** [Title]
**Status:** Implementation complete, some issues remain

### Changes
[Same as success]

### Issues for Manual Review
1. **Memory pattern in hot path** (export.rs:89)
   - Clone in loop, but removing it changes semantics
   - Options: [A] Accept clone, [B] Refactor to iterator

2. **Test intermittently failing** (test_concurrent_export)
   - Passes 4/5 runs, timing-related
   - Needs investigation

### Recommended Actions
- Review issue #1 and decide approach
- Investigate flaky test before merge
```

### 8.3 Output Model

> **Note:** Unlike analysis workflows (memory_audit, spec_maintenance), feature_workflow is an *implementation* workflow. It doesn't emit `actionable_findings.json` because:
> - Validation issues are either auto-fixed in Phase 4 or flagged for immediate human decision
> - There's no "implement the findings later" step - this workflow IS the implementation
>
> See `workflow_manager.md` Section 3.3 for workflow category definitions.

**What feature_workflow outputs:**

| Output | Purpose | Location |
|--------|---------|----------|
| Execution metrics | Manager learning | `{session}/execution_metrics.json` |
| Completion summary | User visibility | Displayed in response |
| Manual review items | Human decision needed | Included in completion summary |

**Manual Review Items** are NOT actionable findings. They explicitly require human judgment and cannot be auto-implemented:
- Architectural decisions ("clone needed for semantics, but hurts performance")
- Multiple valid approaches ("could use iterator OR pre-allocate")
- Breaking changes that need sign-off

These stay in the completion output (Section 8.2), not a separate file.

### 8.4 Execution Metrics

Per `workflow_manager.md` Section 7.4, emit metrics for Manager learning.

**Output Location:** `{session}/execution_metrics.json`

**Schema:**
```rust
struct FeatureWorkflowMetrics {
    session_id: String,
    feature_title: String,
    complexity: Complexity,          // Simple, Medium, Complex
    prompts_used: u32,
    prompt_budget: u32,              // From 1.3 table
    phases_executed: Vec<Phase>,
    phases_skipped: Vec<Phase>,
    escalation_triggered: bool,
    escalation_rounds: Option<u32>,
    validation_findings: ValidationFindings,
    outcome: Outcome,
    duration_seconds: u32,
}

struct ValidationFindings {
    total: u32,
    auto_fixed: u32,
    manual_review: u32,
    by_category: HashMap<FindingCategory, u32>,
}

enum Outcome {
    Success,           // All phases complete, no manual review needed
    PartialSuccess,    // Complete but has manual review items
    Failed,            // Implementation failed after max attempts
    Abandoned,         // User cancelled
}
```

**Example:**
```json
{
  "session_id": "FW-2026-001",
  "feature_title": "Add --dry-run flag to export",
  "complexity": "Simple",
  "prompts_used": 2,
  "prompt_budget": 3,
  "phases_executed": ["Design", "Implement", "Validate"],
  "phases_skipped": ["Fix"],
  "escalation_triggered": false,
  "validation_findings": {
    "total": 0,
    "auto_fixed": 0,
    "manual_review": 0,
    "by_category": {}
  },
  "outcome": "Success",
  "duration_seconds": 180
}
```

---

## 9. Integration with Workflow Manager

### 9.1 Registration

Add to `workflow_manager.md` Section 3.2:

```
| `feature_workflow` | 1-instance with escalation | "add feature", "implement", "build" | 3-8 prompts |
```

### 9.2 Routing Keywords

```
FEATURE_KEYWORDS = [
    "add", "implement", "build", "create", "new feature",
    "fix", "bug", "issue", "broken",
    "refactor", "improve", "optimize",
    "update", "change", "modify"
]
```

### 9.3 Handoff to Other Workflows

| Condition | Handoff Target |
|-----------|----------------|
| Design needs refinement | spec_refinement_workflow (3 rounds) |
| Memory issues found | memory_audit_workflow (targeted) |
| TUI changes need testing | tui_validation_workflow (context-driven) |
| Spec needs updating after impl | spec_maintenance_workflow |

**Note on TUI validation**: `tui_validation_workflow` receives context about what was changed and determines which tests to run. It invokes procedures from `tui_testing_workflow` for actual test execution.

---

## 10. Configuration

### 10.1 Tuneable Parameters

```yaml
feature_workflow:
  # Phase 1
  max_context_tokens: 14000
  escalation_file_threshold: 5

  # Phase 1b (Escalation)
  refinement_max_rounds: 3  # Not the default 7

  # Phase 2
  implement_max_attempts: 3
  test_timeout_seconds: 120

  # Phase 3
  parallel_validation: true
  memory_check_hot_paths_only: true

  # Phase 4
  auto_fix_warnings: true
  auto_fix_style: true
  auto_fix_memory: false  # Requires human decision

  # Output (Manager integration)
  session_dir: "specs/meta/sessions/feature_workflow/{session_id}"
  metrics_output: "{session_dir}/execution_metrics.json"
  emit_metrics: true  # For Manager learning engine
```

### 10.2 Hot Path Patterns

Files considered "hot paths" for memory checking:

```
HOT_PATH_PATTERNS = [
    "**/scanner.rs",
    "**/worker*.rs",
    "**/process*.rs",
    "**/parse*.rs",
    "**/render*.rs",
    "**/*_loop.rs",
    "**/executor*.rs",
]
```

---

## 11. Examples

### 11.1 Simple Feature (2 prompts)

```
User: "Add --dry-run flag to the export command"

Phase 1 (Design):
  Context loaded: CLAUDE.md, specs/export.md
  Inline spec created (not escalated)

Phase 2 (Implement):
  Files modified: export.rs, mod.rs
  Tests added: 2
  cargo check: pass
  cargo test: pass

Phase 3 (Validate):
  No hot paths touched → skip memory check
  No TUI touched → skip style check
  cargo clippy: pass

Phase 4 (Fix):
  Skipped (no findings)

COMPLETE: 2 prompts
```

### 11.2 Medium Feature (5 prompts)

```
User: "Add file preview panel to Discover view"

Phase 1 (Design):
  Context loaded: CLAUDE.md, tui_style_guide.md, specs/views/discover.md
  Inline spec created
  Complexity: 4 files → not escalated

Phase 2 (Implement):
  Prompt 1: Implement preview panel UI
  Prompt 2: Fix test failures
  Files modified: ui.rs, app.rs, mod.rs
  Tests added: 3

Phase 3 (Validate):
  TUI touched → style check runs
  Finding: Wrong unfocused border color (Color::Gray instead of DarkGray)

Phase 4 (Fix):
  Prompt 1: Auto-fix style violation
  Re-verify: pass

  → execution_metrics.json emitted:
    {
      "prompts_used": 5,
      "outcome": "Success",
      "validation_findings": { "total": 1, "auto_fixed": 1 }
    }

COMPLETE: 5 prompts
```

### 11.3 Complex Feature (8 prompts)

```
User: "Implement extraction rule inference engine"

Phase 1 (Design):
  Context loaded: CLAUDE.md, specs/extraction.md, specs/views/discover.md
  Complexity: New pattern, 7 files → ESCALATE

Phase 1b (Escalation):
  spec_refinement_workflow invoked
  Rounds: 3 (capped)
  Output: specs/extraction_inference.md

Phase 2 (Implement):
  Prompt 1: Core inference algorithm
  Prompt 2: Integration with TUI
  Prompt 3: Fix type errors

Phase 3 (Validate):
  Hot path touched → memory check
  TUI touched → style check
  Findings: 2 memory patterns, 1 clippy warning

Phase 4 (Fix):
  Memory patterns flagged for manual review (not auto-fixed)
  Clippy warning fixed

COMPLETE: 8 prompts + manual review needed
```

---

## 12. Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 1.0 | Initial specification |
| 2026-01-14 | 1.1 | **spec_refinement_workflow applied**: Added state machine (2.3), execution metrics (8.4), escalation protocol expansion (4.6), implementation failure handling (5.6). |
| 2026-01-14 | 1.2 | **Output model revised**: Removed actionable_findings.json - inappropriate for implementation workflows. This workflow auto-fixes issues or flags for human decision; nothing for Implementation Protocol to consume. Kept execution_metrics.json for Manager learning. See workflow_manager.md Section 3.3 for workflow category definitions. |
