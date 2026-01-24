# TUI Validation Workflow

> **Status**: Active
> **Version**: 2.1
> **Category**: Analysis workflow (per workflow_manager.md Section 3.3.1)
> **Last Updated**: 2026-01-14
> **Parent**: `workflow_manager.md`
> **Related**: `tui_testing_workflow.md` (procedures), `feature_workflow.md` (caller)

---

## Overview

This workflow defines how to use TUI tests as a post-feature validation step. Rather than analyzing git diffs (a CI approach), this workflow leverages:

1. **Context from workflow manager** - The previous workflow passes explicit context about what was implemented
2. **Spec-driven validation** - Read the spec's state machine and test all documented states/transitions
3. **Coverage tracking in specs** - Specs track validation status

**Key Insight**: Unit tests verify logic correctness. TUI tests verify the *user-visible behavior* across state transitions that unit tests miss.

**Relationship to tui_testing_workflow**: This workflow orchestrates *when* and *what* to test. The actual test procedures (tmux commands, patterns) are defined in `tui_testing_workflow.md`.

---

## State Machine

This workflow follows a defined state machine for validation:

```
                                    ┌──────────────────────┐
                                    │    AWAITING_CONTEXT  │
                                    │  (waiting for input) │
                                    └──────────┬───────────┘
                                               │ receive context
                                               ▼
                                    ┌──────────────────────┐
                                    │  ANALYZING_CONTEXT   │
                                    │  (parse spec, scope) │
                                    └──────────┬───────────┘
                                               │ scope determined
                                               ▼
                                    ┌──────────────────────┐
                                    │   RUNNING_SMOKE      │
                                    │   (quick sanity)     │
                                    └──────────┬───────────┘
                                               │
                              ┌────────────────┴────────────────┐
                              │ pass                            │ fail
                              ▼                                 ▼
                   ┌──────────────────────┐         ┌──────────────────────┐
                   │  RUNNING_TARGETED    │         │   BLOCKED            │
                   │  (context-based)     │         │   (smoke failed)     │
                   └──────────┬───────────┘         └──────────────────────┘
                              │ complete
                              ▼
                   ┌──────────────────────┐
                   │  RUNNING_COVERAGE    │
                   │  (spec-driven)       │
                   └──────────┬───────────┘
                              │ complete
                              ▼
                   ┌──────────────────────┐
                   │   REPORTING          │
                   │   (emit findings)    │
                   └──────────┬───────────┘
                              │
              ┌───────────────┼───────────────┐
              │ all pass      │ issues found  │ blocked
              ▼               ▼               ▼
        ┌───────────┐   ┌───────────┐   ┌───────────┐
        │  PASSED   │   │  ISSUES   │   │  FAILED   │
        └───────────┘   └───────────┘   └───────────┘
```

### State Definitions

| State | Description | Next States |
|-------|-------------|-------------|
| AWAITING_CONTEXT | Waiting for parent workflow to provide validation context | ANALYZING_CONTEXT |
| ANALYZING_CONTEXT | Parsing spec file, determining test scope from context | RUNNING_SMOKE |
| RUNNING_SMOKE | Executing smoke tests for quick sanity check | RUNNING_TARGETED, BLOCKED |
| BLOCKED | Smoke tests failed, cannot proceed | (terminal) |
| RUNNING_TARGETED | Executing tests specific to changed states/keys | RUNNING_COVERAGE |
| RUNNING_COVERAGE | Verifying full state coverage per spec | REPORTING |
| REPORTING | Generating actionable findings and metrics | PASSED, ISSUES, FAILED |
| PASSED | All validations successful | (terminal) |
| ISSUES | Validation found non-blocking issues | (terminal) |
| FAILED | Validation found blocking failures | (terminal) |

---

## Gap Lifecycle

When validation discovers issues, they are tracked using gap lifecycle states (per spec_refinement_workflow.md):

| State | Meaning | Who Sets |
|-------|---------|----------|
| OPEN | Issue discovered during validation | Validator |
| IN_PROGRESS | Being actively fixed | Engineer |
| PROPOSED | Fix implemented, awaiting re-validation | Engineer |
| ACCEPTED | Re-validation passed | Validator |
| RESOLVED | Merged to codebase | Auto (on merge) |

### Severity Levels

Issues are classified by severity weight:

| Severity | Weight | Criteria | Example |
|----------|--------|----------|---------|
| CRITICAL | 16 | Blocks user workflow entirely | Dialog won't open |
| HIGH | 4 | Feature partially broken | Submit button doesn't work |
| MEDIUM | 2 | Non-blocking but visible issue | Wrong focus indicator |
| LOW | 1 | Minor polish issue | Alignment off by 1 char |

**Weighted Score**: Total severity = Σ(weight × count). Used to prioritize fix order.

---

## Context Handoff from Workflow Manager

When the workflow manager triggers TUI validation, it should provide:

```
Validation Context:
  - implemented_feature: "Rule Creation dialog"
  - affected_view: "Discover"
  - affected_states: ["Files", "CreatingRule"]
  - key_handlers_changed: ["n", "Tab", "Enter", "Escape"]
  - data_changes: ["scout_tagging_rules table"]
  - spec_file: "specs/views/discover.md"
```

This context tells the validation workflow exactly what to test without parsing git diffs.

### Context Categories

| Category | What to Test |
|----------|--------------|
| `affected_view` | Run view-specific smoke test |
| `affected_states` | Test entry/exit for each state |
| `key_handlers_changed` | Test each key in context |
| `data_changes` | Test persistence + reload |
| `spec_file` | Parse spec for full state coverage |

---

## Spec-Driven Validation

Instead of maintaining separate test path mappings, the workflow reads the spec file directly.

### Step 1: Parse State Machine from Spec

Each view spec (e.g., `specs/views/discover.md`) contains a state machine definition:

```markdown
### 4.2 State Categories

**Category 1: DEFAULT**
- `Files` - Default state, navigate files

**Category 2: DROPDOWNS**
- `SourcesDropdown` - Source selection
- `TagsDropdown` - Tag selection

**Category 3: DIALOGS**
- `CreatingRule` - Rule creation dialog
- `RulesManager` - Rules CRUD
```

The validation workflow extracts these states and generates test paths.

### Step 2: Generate Test Plan from Spec

For each state in the spec:
1. **Entry test**: Can we reach this state?
2. **Interaction test**: Do documented keys work?
3. **Exit test**: Does ESC return to correct parent?
4. **Visual test**: Does UI match spec layout?

### Step 3: Compare Code to Spec

The workflow verifies code matches spec:
- State enum in code matches spec states
- Key handlers match spec keybindings
- UI elements match spec layout

---

## Validation Status Tracking

Specs can include a validation section that tracks what's been tested:

```markdown
## Validation Status

| State | Entry | Keys | Exit | Visual | Last Validated |
|-------|-------|------|------|--------|----------------|
| Files | ✓ | ✓ | ✓ | ✓ | 2026-01-14 |
| SourcesDropdown | ✓ | ✓ | ✓ | ✓ | 2026-01-14 |
| CreatingRule | ✓ | partial | ✓ | pending | 2026-01-14 |
```

After validation runs, update this section with results.

---

## When to Trigger TUI Validation

### From Workflow Manager (Primary)

The workflow manager triggers validation after any workflow that modifies TUI code:

```
Previous Workflow: feature_implementation
  - Implemented Rule Creation dialog
  - Files modified: app.rs, ui.rs

→ Trigger: tui_validation_workflow
  - Context: { affected_view: "Discover", affected_states: ["CreatingRule"] }
```

### From Spec Changes

When a spec file is modified, validate that code still matches:

```
Previous Workflow: spec_refinement
  - Updated discover.md state machine

→ Trigger: tui_validation_workflow
  - Context: { spec_file: "specs/views/discover.md", check_code_sync: true }
```

### Manual Triggers

Use explicit TUI validation when:
- Multiple components changed
- Refactoring shared infrastructure
- Debugging user-reported bugs
- Pre-release verification

---

## Validation Execution

### Phase 1: Smoke Test (Always Run First)

```bash
./scripts/tui-test-workflow.sh smoke
```

Quick validation that nothing is broken. Exit if this fails.

### Phase 2: Context-Based Validation

Based on context from workflow manager:

```
If affected_view == "Discover":
    ./scripts/tui-test-workflow.sh test-discover
    ./scripts/tui-test-workflow.sh test-rule-dialog  # if dialog states affected

If affected_view == "Home":
    ./scripts/tui-test-workflow.sh test-home
    ./scripts/tui-test-workflow.sh test-view-navigation

If affected_view == "Jobs":
    ./scripts/tui-test-workflow.sh test-jobs

If key_handlers_changed is not empty:
    ./scripts/tui-test-workflow.sh test-state-transitions

If data_changes is not empty:
    # Manual step: restart TUI, verify data persists
```

### Phase 3: Spec Coverage Validation

Parse the spec file and verify all states are reachable:

```
For each state in spec_file.state_machine:
    1. Navigate to state (use documented key sequence)
    2. Verify UI matches spec layout
    3. Verify documented keys work
    4. Verify ESC returns to documented parent state
    5. Mark state as validated
```

---

## Test Commands Reference

| Command | Purpose | When to Use |
|---------|---------|-------------|
| `smoke` | Quick sanity check | Always first |
| `test-home` | Home hub validation | After Home changes |
| `test-discover` | Discover mode validation | After Discover changes |
| `test-jobs` | Jobs mode validation | After Jobs changes |
| `test-rule-dialog` | Rule dialog deep test | After dialog changes |
| `test-sources-manager` | Sources CRUD test | After source management changes |
| `test-rules-manager` | Rules CRUD test | After rule management changes |
| `test-view-navigation` | View switching (1/2/3/4) | After navigation changes |
| `test-state-transitions` | ESC behavior validation | After state machine changes |
| `test-latency` | Input performance | After event handling changes |
| `test-glob-handoff` | Cross-component data flow | After glob explorer changes |
| `full` | Complete test suite | Pre-release, major refactors |

---

## Workflow Manager Integration

### Input Format

When the workflow manager triggers this workflow, it passes context:

```yaml
validation_context:
  trigger: "feature_implementation"  # or "spec_refinement", "bug_fix"
  implemented_feature: "Rule Creation dialog"
  affected_view: "Discover"
  affected_states:
    - Files
    - CreatingRule
  key_handlers_changed:
    - "n"      # open dialog
    - "Tab"    # field navigation
    - "Enter"  # submit
    - "Escape" # cancel
  data_changes:
    - table: "scout_tagging_rules"
      operations: ["INSERT"]
  spec_file: "specs/views/discover.md"
  spec_section: "4. State Machine"
```

### Validation Algorithm

```
1. Run smoke tests (exit on failure)

2. For each affected_state:
   - Navigate to state using documented path
   - Verify entry renders correctly
   - Test each key in key_handlers_changed
   - Verify ESC behavior

3. If data_changes present:
   - Create test data
   - Restart TUI
   - Verify data persisted

4. If spec_file provided:
   - Parse state machine section
   - Verify all states reachable
   - Update validation status in spec

5. Report results:
   - States validated: X/Y
   - Keys validated: A/B
   - Issues found: [list]
```

### Output Format

The workflow emits two outputs per workflow_manager.md Section 13:

#### 1. actionable_findings.json

```json
{
  "workflow": "tui_validation",
  "timestamp": "2026-01-14T10:30:00Z",
  "status": "issues",
  "summary": {
    "total_states": 25,
    "validated": 23,
    "passed": 21,
    "issues": 2
  },
  "findings": [
    {
      "id": "TUI-001",
      "severity": "HIGH",
      "weight": 4,
      "category": "state_transition",
      "state": "GlobExplorer",
      "title": "ESC returns to wrong parent",
      "description": "Pressing ESC in GlobExplorer returns to Files instead of previous state",
      "spec_reference": "specs/views/discover.md#4.3",
      "lifecycle": "OPEN",
      "evidence": {
        "expected": "Return to SourcesDropdown",
        "actual": "Return to Files"
      },
      "suggested_fix": "Check parent_state stack in handle_escape()"
    },
    {
      "id": "TUI-002",
      "severity": "MEDIUM",
      "weight": 2,
      "category": "key_handler",
      "state": "CreatingRule",
      "title": "Shift+Tab navigation missing",
      "description": "Cannot navigate backwards through fields with Shift+Tab",
      "spec_reference": "specs/views/discover.md#6.4",
      "lifecycle": "OPEN"
    }
  ],
  "weighted_score": 6,
  "recommendations": [
    "Fix GlobExplorer ESC handling before release",
    "Add Shift+Tab support as enhancement"
  ]
}
```

#### 2. execution_metrics.json

```json
{
  "workflow": "tui_validation",
  "duration_ms": 45000,
  "phases": {
    "smoke": { "duration_ms": 5000, "tests": 15, "passed": 15 },
    "targeted": { "duration_ms": 20000, "tests": 8, "passed": 8 },
    "coverage": { "duration_ms": 20000, "states": 25, "validated": 23 }
  },
  "coverage_percent": 92,
  "key_coverage_percent": 89
}
```

#### Legacy YAML Format (Deprecated)

For backwards compatibility:

```yaml
validation_results:
  status: "passed" | "failed" | "partial"
  smoke_tests:
    passed: 15
    failed: 0
  state_coverage:
    total: 25
    validated: 23
    issues:
      - state: "GlobExplorer"
        issue: "ESC returns to wrong parent"
  key_coverage:
    total: 12
    validated: 12
  data_persistence: "verified"
  recommendations:
    - "Fix GlobExplorer ESC handling"
    - "Add test for Shift+Tab navigation"
```

---

## Test Scenario Templates

### Template: State Machine Coverage

For any mode with a state machine, test every state:

```bash
# Example: Discover mode (25 states)
# Category 1: DEFAULT
Test: Files state (default entry)
  - Navigate to Discover (press 1)
  - Verify: Files panel focused, footer shows correct keys

# Category 2: DROPDOWNS
Test: Sources dropdown
  - From Files, press 1
  - Verify: Dropdown opens, Sources visible, filter input active

Test: Tags dropdown
  - From Files, press 2
  - Verify: Dropdown opens, Tags visible, live preview works

# Category 3: INPUTS
Test: Filter mode
  - From Files, press /
  - Type filter text
  - Verify: Files filtered, ESC clears filter

Test: Entering path
  - From Files, press s
  - Verify: Path input active

# Category 4: DIALOGS
Test: Rule Creation dialog
  - Press n
  - Verify: Dialog opens, pattern field focused

Test: Rules Manager
  - Press R
  - Verify: Rules list visible

Test: Sources Manager
  - Press M
  - Verify: Sources list visible

# Test all ESC paths
Test: ESC from each state returns to correct parent
```

### Template: Dialog Input Coverage

For any dialog with form fields:

```bash
Test: [Dialog Name]

1. Open dialog
   - Press trigger key
   - Verify: Dialog opens, correct field focused

2. Test each field
   - Type input
   - Verify: Characters appear correctly
   - Test backspace
   - Test cursor movement (if supported)

3. Test Tab navigation
   - Press Tab
   - Verify: Focus moves to next field
   - Press Shift+Tab (if supported)
   - Verify: Focus moves to previous field

4. Test submission
   - Fill all required fields
   - Press Enter
   - Verify: Dialog closes, data persisted

5. Test cancellation
   - Open dialog, type data
   - Press Escape
   - Verify: Dialog closes, data NOT persisted

6. Test validation (if applicable)
   - Submit with invalid input
   - Verify: Error shown, dialog remains open
```

### Template: Data Flow Coverage

For features that load/save data:

```bash
Test: [Data Feature]

1. Initial load
   - Start TUI fresh
   - Navigate to feature
   - Verify: Data loads from database correctly

2. Create operation
   - Create new item
   - Verify: Item appears in list
   - Verify: Database contains new item

3. Update operation
   - Modify existing item
   - Verify: Changes visible in UI
   - Verify: Database reflects changes

4. Delete operation
   - Delete item
   - Verify: Item removed from list
   - Verify: Database no longer contains item

5. Persistence across sessions
   - Create/modify data
   - Restart TUI
   - Verify: Changes persisted
```

---

## Spec-Driven State Coverage

### Reading the State Machine from Spec

The workflow parses the spec file to understand what states need validation.

**Example**: For `specs/views/discover.md` Section 4:

```
Extracted States:
  Category DEFAULT: [Files]
  Category DROPDOWNS: [SourcesDropdown, TagsDropdown]
  Category INPUTS: [Filtering, EnteringPath]
  Category DIALOGS: [CreatingRule, RulesManager, SourcesManager, WizardMenu]
  Category GLOB: [GlobExplorerActive, GlobExplorerScanning, ...]

Extracted Transitions:
  Files --[1]--> SourcesDropdown
  Files --[2]--> TagsDropdown
  Files --[n]--> CreatingRule
  Files --[R]--> RulesManager
  SourcesDropdown --[Esc]--> Files
  CreatingRule --[Enter]--> Files (with data saved)
  ...
```

### Test Generation from Spec

For each extracted state, generate tests:

```python
for state in spec_states:
    # Entry test
    test(f"Can reach {state} via documented path")

    # Key tests
    for key in state.documented_keys:
        test(f"Key '{key}' works in {state}")

    # Exit test
    test(f"ESC from {state} returns to {state.parent}")

    # Visual test (manual)
    verify(f"UI in {state} matches spec layout")
```

### Coverage Report

After validation, generate coverage report:

```
STATE COVERAGE REPORT - Discover Mode
=====================================

State               Entry   Keys    Exit    Visual
--------------------------------------------------
Files               ✓       ✓       N/A     ✓
SourcesDropdown     ✓       ✓       ✓       ✓
TagsDropdown        ✓       ✓       ✓       ✓
Filtering           ✓       partial ✓       ✓
EnteringPath        ✓       ✓       ✓       ✓
CreatingRule        ✓       ✓       ✓       ✓
RulesManager        ✓       partial pending ✓
SourcesManager      ✓       partial pending ✓
GlobExplorerActive  ✓       ✓       FAIL    ✓

Coverage: 9/9 states (100%)
Key Coverage: 34/38 keys (89%)
Issues: 1 (GlobExplorer ESC)
```

---

## Workflow Manager Integration Examples

### Example 1: After Feature Implementation

```
Workflow Manager: "Feature implemented: Rule Creation dialog"

Context passed to tui_validation_workflow:
  implemented_feature: "Rule Creation dialog"
  affected_view: "Discover"
  affected_states: ["Files", "CreatingRule"]
  key_handlers_changed: ["n", "Tab", "Enter", "Escape"]
  data_changes: [{ table: "scout_tagging_rules" }]
  spec_file: "specs/views/discover.md"

Validation workflow executes:
1. ./scripts/tui-test-workflow.sh smoke        # Sanity check
2. ./scripts/tui-test-workflow.sh test-discover # Affected view
3. ./scripts/tui-test-workflow.sh test-rule-dialog # Deep test of feature
4. Verify: Can create rule, data persists, restart shows rule

Result: { status: "passed", issues: [] }
```

### Example 2: After State Machine Refactor

```
Workflow Manager: "Refactored Discover state machine"

Context:
  trigger: "refactor"
  affected_view: "Discover"
  spec_file: "specs/views/discover.md"
  check_full_coverage: true

Validation workflow executes:
1. Parse specs/views/discover.md Section 4 (State Machine)
2. Extract all 25 states
3. For each state: test entry, keys, exit
4. Generate coverage report

Result:
  status: "partial"
  coverage: "23/25 states"
  issues:
    - state: "GlobExplorerScanning"
      issue: "State unreachable in test"
    - state: "SourceEditDialog"
      issue: "ESC returns to wrong parent"
```

### Example 3: After Spec Update

```
Workflow Manager: "Updated discover.md spec - added new state"

Context:
  trigger: "spec_refinement"
  spec_file: "specs/views/discover.md"
  changes: ["Added ExportDialog state"]
  check_code_sync: true

Validation workflow executes:
1. Parse updated spec
2. Search code for ExportDialog state
3. If missing: Report "Spec-code mismatch"
4. If present: Run targeted tests

Result:
  status: "action_required"
  issues:
    - "ExportDialog in spec but not in code"
    - "Recommend: implement or update spec"
```

---

## Adding New Test Scenarios

When implementing a new feature, follow this pattern:

### 1. Update Spec First

Add the new state/feature to `specs/views/<mode>.md`:

```markdown
### 4.2 State Categories

**Category 4: DIALOGS**
- `ExportDialog` - NEW: Export data dialog
  - Entry: `x` from Files state
  - Exit: `Escape` returns to Files
  - Actions: `Enter` confirms export
```

### 2. Implement Feature

Write the code matching the spec.

### 3. Add Test to Script

Edit `scripts/tui-test-workflow.sh`:

```bash
cmd_test_export_dialog() {
    log_info "Testing Export Dialog..."

    cmd_restart
    sleep 0.5

    # Navigate per spec
    tmux send-keys -t "$SESSION" "1"  # Go to Discover
    sleep 0.5
    tmux send-keys -t "$SESSION" "x"  # Open export dialog
    sleep 0.3

    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    # Assert per spec
    if echo "$output" | grep -qi "export"; then
        log_success "Export Dialog: Opens correctly"
    else
        log_fail "Export Dialog: Missing"
    fi
}
```

### 4. Update Validation Status

Add to spec's validation section:

```markdown
| ExportDialog | ✓ | ✓ | ✓ | ✓ | 2026-01-14 |
```

---

## Troubleshooting

### Test Flakiness

| Symptom | Cause | Fix |
|---------|-------|-----|
| Intermittent failures | Timing issues | Increase sleep duration |
| Wrong output captured | Async operation incomplete | Add wait loop for completion |
| State not as expected | Stale session | Always restart session |

### Debugging Failed Tests

```bash
# 1. Restart fresh
./scripts/tui-test-workflow.sh restart

# 2. Step through manually
./scripts/tui-test-workflow.sh sc '1' 0.5    # Each keystroke
./scripts/tui-test-workflow.sh sc 'n' 0.5
# ... until failure point

# 3. Attach for live inspection
./scripts/tui-test-workflow.sh attach
# (Press Ctrl+B then D to detach)

# 4. Compare output to spec
cat specs/views/discover.md | grep -A 20 "State Machine"
```

---

## Metrics

Track TUI validation effectiveness:

| Metric | Target | How to Measure |
|--------|--------|----------------|
| State coverage | 100% of spec states | `validated_states / spec_states` |
| Key coverage | 100% of documented keys | `validated_keys / spec_keys` |
| Spec-code sync | 100% match | States in spec == states in code |
| Validation freshness | <7 days | Last validation date in spec |
| Issue resolution time | <24h | Time from issue found to fixed |

### Coverage Dashboard

Maintain in each spec file:

```markdown
## Validation Status

Total States: 25
Validated: 23 (92%)
Last Full Run: 2026-01-14

Issues:
- GlobExplorerScanning: unreachable in automated test
- SourceEditDialog: ESC behavior incorrect

Next Actions:
- [ ] Fix SourceEditDialog ESC
- [ ] Add manual test for GlobExplorerScanning
```

---

## Quick Reference

### For Workflow Manager

Pass this context when triggering validation:

```yaml
validation_context:
  trigger: "feature|refactor|spec_change|bug_fix"
  affected_view: "Discover|Home|Jobs|Inspect"
  affected_states: [list of state names]
  key_handlers_changed: [list of keys]
  data_changes: [list of tables]
  spec_file: "path to spec"
  check_full_coverage: true|false
```

### For Validation Execution

```bash
# Quick validation (after small change)
./scripts/tui-test-workflow.sh smoke

# Targeted validation (after feature in specific view)
./scripts/tui-test-workflow.sh test-discover

# Full validation (after refactor or pre-release)
./scripts/tui-test-workflow.sh full

# Manual exploration (after spec change)
./scripts/tui-test-workflow.sh attach
```

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-14 | 2.1 | Added: State machine, gap lifecycle, severity levels, actionable_findings.json output format per spec_refinement_workflow requirements |
| 2026-01-14 | 2.0 | Redesigned for LLM context handoff, spec-driven validation |
| 2026-01-14 | 1.0 | Initial workflow specification |
