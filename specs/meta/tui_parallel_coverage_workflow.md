# TUI Parallel Coverage Workflow

> **Status**: Active
> **Version**: 1.3
> **Category**: Analysis workflow (per workflow_manager.md Section 3.3.1)
> **Last Updated**: 2026-01-17
> **Parent**: `workflow_manager.md`
> **Related**: `tui_testing_workflow.md` (procedures), `tui_validation_workflow.md` (orchestration)

---

## Overview

This workflow maximizes TUI test coverage over time by running parallel tests and **aggregating issues with rich context for another LLM to fix**.

### Core Principles

1. **Analysis Only**: This workflow NEVER modifies code. It only detects, documents, and contextualizes issues.
2. **Rich Context Output**: Findings include enough information (screen captures, key sequences, file locations) for an LLM to understand and fix the issue without re-running tests.
3. **Parallel Execution**: Multiple tmux sessions run simultaneously, each testing different state paths.
4. **Intelligent Prioritization**: Test paths selected based on staleness, risk, and coverage gaps.
5. **Session Isolation**: Each test uses isolated state to avoid cross-contamination.

### What This Workflow Does

| Action | Yes/No |
|--------|--------|
| Run TUI tests in parallel | ✓ Yes |
| Capture screen output on failures | ✓ Yes |
| Record keystroke sequences | ✓ Yes |
| Identify spec-code divergences | ✓ Yes |
| Aggregate findings with fix context | ✓ Yes |
| Modify any code or specs | ✗ **NO** |
| Auto-fix detected issues | ✗ **NO** |

**Key Insight**: TUI tests are I/O bound (waiting for renders), not CPU bound. Multiple tmux sessions can run concurrently with minimal interference, dramatically reducing total test time.

---

## State Machine

```
                                    ┌──────────────────────┐
                                    │   INITIALIZING       │
                                    │ (load coverage data) │
                                    └──────────┬───────────┘
                                               │ data loaded
                                               ▼
                                    ┌──────────────────────┐
                                    │   PRIORITIZING       │
                                    │ (rank test paths)    │
                                    └──────────┬───────────┘
                                               │ paths ranked
                                               ▼
                                    ┌──────────────────────┐
                                    │   SPAWNING           │
                                    │ (launch sessions)    │
                                    └──────────┬───────────┘
                                               │ all spawned
                                               ▼
                                    ┌──────────────────────┐
                                    │   MONITORING         │◄─────┐
                                    │ (track progress)     │      │
                                    └──────────┬───────────┘      │
                                               │                  │
                              ┌────────────────┴────────────────┐ │
                              │ session complete                │ │
                              ▼                                 │ │
                   ┌──────────────────────┐                     │ │
                   │  COLLECTING          │─────────────────────┘ │
                   │  (gather results)    │  more sessions        │
                   └──────────┬───────────┘                       │
                              │ all complete                      │
                              ▼                                   │
                   ┌──────────────────────┐                       │
                   │  AGGREGATING         │                       │
                   │  (merge findings)    │                       │
                   └──────────┬───────────┘                       │
                              │                                   │
              ┌───────────────┼───────────────┐                   │
              │ coverage goal │ gaps remain   │                   │
              │ met           │               │                   │
              ▼               ▼               │                   │
        ┌───────────┐   ┌───────────┐        │                   │
        │  COMPLETE │   │  SCHEDULE │────────┴───────────────────┘
        └───────────┘   │  _MORE    │  (loop with remaining paths)
                        └───────────┘
```

### State Definitions

| State | Description | Next States |
|-------|-------------|-------------|
| INITIALIZING | Load coverage history, parse spec state machines | PRIORITIZING |
| PRIORITIZING | Rank test paths by staleness, risk, coverage gaps | SPAWNING |
| SPAWNING | Launch parallel tmux sessions (up to max_parallel) | MONITORING |
| MONITORING | Watch session progress, handle timeouts | COLLECTING |
| COLLECTING | Gather results from completed sessions | MONITORING, AGGREGATING |
| AGGREGATING | Merge findings, update coverage database | COMPLETE, SCHEDULE_MORE |
| SCHEDULE_MORE | More paths to test, spawn next batch | SPAWNING |
| COMPLETE | Coverage goal met or all paths tested | (terminal) |

---

## Parallelism Model

### Session Naming Convention

```
tui-parallel-{worker_id}

Examples:
  tui-parallel-0
  tui-parallel-1
  tui-parallel-2
  tui-parallel-3
```

### Resource Calculation

```bash
# Default: min(CPU cores / 2, 4)
# Each session needs ~50MB RAM + terminal resources
max_parallel=$(( $(nproc 2>/dev/null || sysctl -n hw.ncpu) / 2 ))
max_parallel=$(( max_parallel > 4 ? 4 : max_parallel ))
max_parallel=$(( max_parallel < 1 ? 1 : max_parallel ))
```

### Session Isolation

Each parallel session:
- Has its own tmux session name
- Uses an isolated `CASPARIAN_HOME` (per test path) to prevent cross-test data pollution
- Writes results to separate output files
- Operates on independent state paths

**Isolation Rationale**: Many TUI paths create or mutate state (rules, sources, filters). Running those in parallel against a shared database creates false positives, UI flakes, and non-reproducible results. Isolated homes allow safe, parallel mutation while preserving true user flows.

---

## Test Path Prioritization

### Priority Score Calculation

```
priority_score = (staleness_weight * staleness_score)
               + (risk_weight * risk_score)
               + (coverage_weight * coverage_score)
               + (dependency_weight * dependency_score)
```

### Scoring Factors

| Factor | Weight | Score Calculation |
|--------|--------|-------------------|
| **Staleness** | 0.4 | `days_since_last_test / 30` (capped at 1.0) |
| **Risk** | 0.3 | `1.0` if code changed since last test, `0.0` otherwise |
| **Coverage Gap** | 0.2 | `1.0 - (times_tested / max_times_tested)` |
| **Dependency** | 0.1 | `1.0` if blocking other tests, `0.0` otherwise |

### Test Path Definition

A test path is a sequence of keystrokes with expected outcomes:

```yaml
test_paths:
  - id: "discover-sources-dropdown"
    view: "Discover"
    entry: ["1"]           # Keys to reach Discover from Home
    actions:
      - { keys: "1", expect: "Sources" }   # Open dropdown
      - { keys: "Escape", expect: "Files" } # Close dropdown
    exit: ["Escape"]

  - id: "discover-rule-creation"
    view: "Discover"
    entry: ["1"]
    actions:
      - { keys: "n", expect: "Rule" }      # Open dialog
      - { keys: "Escape", expect: "Files" } # Cancel
    exit: ["Escape"]

  - id: "discover-glob-explorer"
    view: "Discover"
    entry: ["1"]
    actions:
      - { keys: "g", expect: "Glob" }       # Open explorer
      - { keys: "Escape", expect: "Files" } # Close explorer
    exit: ["Escape"]
```

Test paths focus on **user-visible behavior** (what text appears), not internal state names. The `expect` field uses simple substring matching against the captured screen.

---

## Coverage Database Schema

### Table: `tui_test_coverage`

```sql
CREATE TABLE IF NOT EXISTS tui_test_coverage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_path_id TEXT NOT NULL,
    view TEXT NOT NULL,
    last_tested_at TEXT,  -- ISO8601
    times_tested INTEGER DEFAULT 0,
    last_result TEXT,     -- 'pass', 'fail', 'skip', 'timeout'
    last_duration_ms INTEGER,
    findings_count INTEGER DEFAULT 0,
    UNIQUE(test_path_id)
);

CREATE INDEX IF NOT EXISTS idx_coverage_staleness ON tui_test_coverage(last_tested_at);
CREATE INDEX IF NOT EXISTS idx_coverage_view ON tui_test_coverage(view);
```

### Table: `tui_test_runs`

```sql
CREATE TABLE IF NOT EXISTS tui_test_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,        -- UUID for this parallel run
    started_at TEXT NOT NULL,
    completed_at TEXT,
    parallel_count INTEGER,
    paths_tested INTEGER,
    paths_passed INTEGER,
    paths_failed INTEGER,
    total_duration_ms INTEGER,
    coverage_percent REAL,
    findings_json TEXT           -- Full actionable_findings.json
);
```

---

## Execution Phases

### Phase 1: Initialize

```bash
# Root for per-session homes
run_root="/tmp/casparian-tui-$RUN_ID"

# Load coverage data
coverage_data=$(sqlite3 ~/.casparian_flow/casparian_flow.sqlite3 \
  "SELECT test_path_id, last_tested_at, times_tested, last_result
   FROM tui_test_coverage")

# Get code hashes for risk detection
current_code_hash=$(find crates/casparian/src -name "*.rs" -exec md5sum {} \; | md5sum | cut -d' ' -f1)
```

### Phase 2: Prioritize

```bash
# Generate prioritized test list
# Output: test_paths_prioritized.json
./scripts/tui-parallel-prioritize.sh \
  --coverage-db ~/.casparian_flow/casparian_flow.sqlite3 \
  --spec-dir specs/views/ \
  --code-hash "$current_code_hash" \
  --output test_paths_prioritized.json
```

### Phase 3: Spawn Parallel Sessions

```bash
#!/bin/bash
# scripts/tui-parallel-spawn.sh

MAX_PARALLEL=${1:-4}
PATHS_FILE=${2:-test_paths_prioritized.json}
RUN_ID=$(uuidgen)

# Read prioritized paths
paths=($(jq -r '.[].id' "$PATHS_FILE"))
total_paths=${#paths[@]}

# Track active sessions
declare -A active_sessions
active_count=0
path_index=0
results_dir="/tmp/tui-parallel-$RUN_ID"
mkdir -p "$results_dir"

# Spawn initial batch
while [[ $active_count -lt $MAX_PARALLEL && $path_index -lt $total_paths ]]; do
    path_id="${paths[$path_index]}"
    session_name="tui-parallel-$active_count"
    session_home="$run_root/$path_id"
    mkdir -p "$session_home"

    # Start session with specific test path
    tmux new-session -d -s "$session_name" -x 120 -y 40 \
        "CASPARIAN_HOME='$session_home' ./scripts/tui-test-path.sh '$path_id' '$results_dir/${path_id}.json'"

    active_sessions[$session_name]="$path_id"
    ((active_count++))
    ((path_index++))
done

echo "Spawned $active_count parallel sessions"
```

### Phase 4: Monitor and Collect

```bash
#!/bin/bash
# Monitor loop - check for completed sessions, spawn new ones

while [[ $active_count -gt 0 || $path_index -lt $total_paths ]]; do
    for session_name in "${!active_sessions[@]}"; do
        if ! tmux has-session -t "$session_name" 2>/dev/null; then
            # Session completed
            path_id="${active_sessions[$session_name]}"
            unset active_sessions[$session_name]
            ((active_count--))

            echo "Completed: $path_id"

            # Spawn next if available
        if [[ $path_index -lt $total_paths && $active_count -lt $MAX_PARALLEL ]]; then
            next_path="${paths[$path_index]}"
            session_home="$run_root/$next_path"
            mkdir -p "$session_home"
            tmux new-session -d -s "$session_name" -x 120 -y 40 \
                    "CASPARIAN_HOME='$session_home' ./scripts/tui-test-path.sh '$next_path' '$results_dir/${next_path}.json'"
            active_sessions[$session_name]="$next_path"
            ((active_count++))
            ((path_index++))
        fi
        fi
    done

    sleep 0.5
done
```

### Phase 5: Aggregate Results

```bash
#!/bin/bash
# scripts/tui-parallel-aggregate.sh

RESULTS_DIR=$1
RUN_ID=$2
OUTPUT_FILE=${3:-actionable_findings.json}

# Merge all result files
findings=()
total_passed=0
total_failed=0
total_duration=0

for result_file in "$RESULTS_DIR"/*.json; do
    if [[ -f "$result_file" ]]; then
        status=$(jq -r '.status' "$result_file")
        duration=$(jq -r '.duration_ms' "$result_file")

        if [[ "$status" == "pass" ]]; then
            ((total_passed++))
        else
            ((total_failed++))
            # Extract findings
            jq -r '.findings[]' "$result_file" >> /tmp/all_findings.json
        fi

        total_duration=$((total_duration + duration))
    fi
done

# Generate aggregated output
cat > "$OUTPUT_FILE" <<EOF
{
  "workflow": "tui_parallel_coverage",
  "run_id": "$RUN_ID",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "status": "$([ $total_failed -eq 0 ] && echo 'passed' || echo 'issues')",
  "summary": {
    "total_paths": $((total_passed + total_failed)),
    "passed": $total_passed,
    "failed": $total_failed,
    "duration_ms": $total_duration,
    "parallel_sessions": $MAX_PARALLEL
  },
  "findings": $(cat /tmp/all_findings.json | jq -s '.'),
  "coverage_delta": {
    "paths_newly_covered": $(jq -s 'map(select(.first_test == true)) | length' "$RESULTS_DIR"/*.json),
    "paths_retested": $(jq -s 'map(select(.first_test != true)) | length' "$RESULTS_DIR"/*.json)
  }
}
EOF

# Update coverage database
./scripts/tui-parallel-update-coverage.sh "$RESULTS_DIR" "$RUN_ID"
```

---

## Workflow Manager Integration

### Trigger Keywords

| Keyword | Action |
|---------|--------|
| "parallel TUI test" | Run parallel coverage workflow |
| "maximize TUI coverage" | Run with coverage optimization |
| "TUI coverage report" | Generate coverage report without running tests |
| "test all TUI paths" | Run exhaustive parallel testing |

### Input Context

```yaml
parallel_coverage_context:
  max_parallel: 4                    # Override default parallelism
  coverage_target: 0.90              # Stop when 90% coverage reached
  priority_mode: "staleness"         # staleness | risk | gaps | balanced
  time_budget_minutes: 30            # Max total time
  views: ["Discover", "Jobs"]        # Limit to specific views (optional)
  exclude_paths: ["glob-explorer-*"] # Skip specific paths (optional)
```

### Output Format

Per workflow_manager.md Section 13. **This output is designed to give a downstream LLM all context needed to fix issues WITHOUT re-running tests.**

#### actionable_findings.json

The findings format includes rich context so another LLM can understand and fix issues:

```json
{
  "workflow": "tui_parallel_coverage",
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2026-01-17T10:30:00Z",
  "status": "issues",
  "summary": {
    "total_paths": 45,
    "passed": 42,
    "failed": 3,
    "duration_ms": 180000,
    "parallel_sessions": 4,
    "coverage_percent": 92.5
  },
  "findings": [
    {
      "test_path_id": "discover-glob-explorer",
      "findings": [
        {
          "type": "expectation_failed",
          "expected": "GlobExplorer",
          "actual_screen": "┌─ Discover ────────────────────────────┐\n│ Source: [All Sources ▼]              │\n│ Tag:    [All Tags ▼]                 │\n├───────────────────────────────────────┤\n│ > file1.csv                           │\n│   file2.json                          │\n└───────────────────────────────────────┘",
          "keys_sent": "g",
          "full_key_sequence": ["1", "Enter", "3", "j", "g"],
          "step_index": 4
        }
      ],
      "context": {
        "spec_reference": "specs/rule_builder.md Section 9 - Keybindings",
        "relevant_files": [
          "crates/casparian/src/cli/tui/app.rs",
          "crates/casparian/src/cli/tui/discover.rs",
          "crates/casparian/src/cli/tui/glob_explorer.rs"
        ],
        "key_handler_location": "app.rs:handle_key_event()",
        "expected_behavior": "Pressing 'g' in Files panel should open GlobExplorer overlay",
        "actual_behavior": "Screen still shows Discover view without GlobExplorer overlay"
      },
      "fix_hints": {
        "likely_cause": "Key 'g' not bound in Files focus state",
        "check_locations": [
          "Match arm for 'g' key in handle_key_event",
          "Focus state when in Files panel",
          "GlobExplorer component initialization"
        ]
      }
    }
  ],
  "coverage_report": {
    "by_view": {
      "Home": { "total": 5, "covered": 5, "percent": 100 },
      "Discover": { "total": 25, "covered": 23, "percent": 92 },
      "Jobs": { "total": 10, "covered": 9, "percent": 90 },
      "Inspect": { "total": 5, "covered": 5, "percent": 100 }
    },
    "stale_paths": [
      { "id": "jobs-cancel-running", "days_since_test": 14 }
    ],
    "never_tested": [
      { "id": "discover-wizard-menu" }
    ]
  }
}
```

#### Finding Fields Reference

| Field | Purpose | Used By Fixing LLM |
|-------|---------|-------------------|
| `type` | Category of failure | Understand failure type |
| `expected` | What text should appear | Know the goal |
| `actual_screen` | Captured TUI output | See exact state |
| `keys_sent` | Key that triggered failure | Reproduce mentally |
| `full_key_sequence` | All keys from start | Understand navigation path |
| `step_index` | Which step failed | Locate in test path |
| `context.spec_reference` | Link to spec | Check requirements |
| `context.relevant_files` | Files to examine | Narrow search |
| `context.key_handler_location` | Where key is handled | Start point for fix |
| `context.expected_behavior` | Human description | Understand intent |
| `context.actual_behavior` | What happened | Gap analysis |
| `fix_hints.likely_cause` | Hypothesis | Quick triage |
| `fix_hints.check_locations` | Where to look | Actionable steps |

#### Using Output for Downstream Fixing

When invoking another LLM to fix issues, provide:

1. **The findings file**: `actionable_findings.json`
2. **Prompt template**:

```
You are fixing TUI behavior issues in the Casparian Flow project.

For each finding in the attached actionable_findings.json:

1. Read the `actual_screen` to see what the user actually saw
2. Check `context.relevant_files` to find the code to modify
3. Consult `context.spec_reference` for expected behavior
4. Use `fix_hints.check_locations` as starting points

Key constraint: Do NOT re-run TUI tests. The findings contain all
information needed to understand and fix the issues.

Finding to fix:
{paste finding here}
```

**Important**: The fixing LLM should NOT re-run tests - the captured screen output and context are sufficient to understand and fix each issue.

#### execution_metrics.json

```json
{
  "workflow": "tui_parallel_coverage",
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "duration_ms": 180000,
  "phases": {
    "initialize": { "duration_ms": 500 },
    "prioritize": { "duration_ms": 200 },
    "spawn": { "duration_ms": 1000 },
    "monitor": { "duration_ms": 175000 },
    "aggregate": { "duration_ms": 3300 }
  },
  "parallelism": {
    "max_sessions": 4,
    "avg_concurrent": 3.2,
    "session_utilization": 0.80
  },
  "speedup": {
    "sequential_estimate_ms": 540000,
    "actual_ms": 180000,
    "speedup_factor": 3.0
  }
}
```

---

## Test Path Script

```bash
#!/bin/bash
# scripts/tui-test-path.sh
# Executes a single test path and outputs results with RICH CONTEXT for fixing LLM
#
# ANALYSIS ONLY: This script captures issues but does NOT fix them.
# Output includes screen captures and context for a downstream LLM to fix.

PATH_ID=$1
OUTPUT_FILE=$2
BINARY="./target/release/casparian"

# Load test path definition and metadata
path_def=$(jq ".[] | select(.id == \"$PATH_ID\")" test_paths.json)
entry_keys=$(echo "$path_def" | jq -r '.entry[]')
actions=$(echo "$path_def" | jq -c '.actions[]')
exit_keys=$(echo "$path_def" | jq -r '.exit[]')
spec_ref=$(echo "$path_def" | jq -r '.spec_reference // "specs/rule_builder.md"')
relevant_files=$(echo "$path_def" | jq -c '.relevant_files // []')

start_time=$(date +%s)
findings_json="[]"
status="pass"
key_sequence=()
step_index=0

# Start TUI
CASPARIAN_HOME=${CASPARIAN_HOME:-"$HOME/.casparian_flow"} $BINARY tui &
tui_pid=$!
sleep 1

# Navigate to entry state (track all keys)
for key in $entry_keys; do
    ./scripts/tui-send.sh "$key"
    key_sequence+=("$key")
    sleep 0.3
done

# Execute actions with rich context capture
while IFS= read -r action; do
    keys=$(echo "$action" | jq -r '.keys')
    expect=$(echo "$action" | jq -r '.expect // empty')

    ./scripts/tui-send.sh "$keys"
    key_sequence+=("$keys")
    sleep 0.3

    if [[ -n "$expect" ]]; then
        # Capture screen output
        output=$(./scripts/tui-capture-stable.sh)
        if ! echo "$output" | grep -q "$expect"; then
            status="fail"
            # Capture RICH CONTEXT for fixing LLM
            escaped_output=$(echo "$output" | jq -Rs .)
            key_seq_json=$(printf '%s\n' "${key_sequence[@]}" | jq -R . | jq -s .)
            finding=$(cat <<FINDING
{
  "type": "expectation_failed",
  "expected": "$expect",
  "actual_screen": $escaped_output,
  "keys_sent": "$keys",
  "full_key_sequence": $key_seq_json,
  "step_index": $step_index
}
FINDING
)
            findings_json=$(echo "$findings_json" | jq ". + [$finding]")
        fi
    fi
    step_index=$((step_index + 1))
done <<< "$actions"

# Exit
for key in $exit_keys; do
    ./scripts/tui-send.sh "$key"
    sleep 0.2
done

# Cleanup
kill $tui_pid 2>/dev/null

end_time=$(date +%s)
duration=$(( (end_time - start_time) * 1000 ))

# Output result with rich context
cat > "$OUTPUT_FILE" <<EOF
{
  "test_path_id": "$PATH_ID",
  "status": "$status",
  "duration_ms": $duration,
  "findings": $findings_json,
  "context": {
    "spec_reference": "$spec_ref",
    "relevant_files": $relevant_files
  },
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF
```

---

## CLI Integration

### New Commands

```bash
# Run parallel coverage workflow
casparian tui-test parallel [--max-parallel 4] [--coverage-target 0.9] [--view Discover]

# Generate coverage report
casparian tui-test coverage-report [--format json|table]

# List stale test paths
casparian tui-test stale [--days 7]

# Reset coverage database
casparian tui-test reset-coverage
```

### Example Usage

```bash
# Quick parallel run (4 sessions, balanced priority)
./scripts/tui-parallel-run.sh

# Target specific view with high parallelism
./scripts/tui-parallel-run.sh --max-parallel 8 --view Discover

# Focus on stale paths only
./scripts/tui-parallel-run.sh --priority-mode staleness --staleness-days 7

# Run until 95% coverage
./scripts/tui-parallel-run.sh --coverage-target 0.95

# Time-boxed run
./scripts/tui-parallel-run.sh --time-budget 15  # 15 minutes max
```

---

## Coverage Optimization Strategies

### Strategy 1: Staleness-First

Prioritize paths not tested recently:
- Good for: Regular maintenance runs
- Trade-off: May miss high-risk changes

```bash
./scripts/tui-parallel-run.sh --priority-mode staleness
```

### Strategy 2: Risk-First

Prioritize paths with recent code changes:
- Good for: Post-commit validation
- Trade-off: May leave old paths stale

```bash
./scripts/tui-parallel-run.sh --priority-mode risk --since-commit HEAD~5
```

### Strategy 3: Gap-First

Prioritize paths with lowest test count:
- Good for: Initial coverage building
- Trade-off: May test low-value paths

```bash
./scripts/tui-parallel-run.sh --priority-mode gaps
```

### Strategy 4: Balanced (Default)

Weighted combination of all factors:
- Good for: General purpose
- Trade-off: No single focus

```bash
./scripts/tui-parallel-run.sh --priority-mode balanced
```

---

## Failure Handling

### Session Timeout

```bash
# If session runs longer than 60s, kill and mark as timeout
TIMEOUT=60

for session_name in "${!active_sessions[@]}"; do
    start_time="${session_start_times[$session_name]}"
    elapsed=$(($(date +%s) - start_time))

    if [[ $elapsed -gt $TIMEOUT ]]; then
        tmux kill-session -t "$session_name" 2>/dev/null
        path_id="${active_sessions[$session_name]}"

        # Record timeout finding
        echo "{\"test_path_id\": \"$path_id\", \"status\": \"timeout\", \"duration_ms\": ${TIMEOUT}000}" \
            > "$results_dir/${path_id}.json"
    fi
done
```

### Session Crash

If tmux session exits unexpectedly:
1. Check for core dump / error log
2. Record as "crash" status
3. Continue with remaining paths
4. Include crash details in findings

### Database Lock Contention

Since all sessions read from the same database:
- Use `PRAGMA journal_mode=WAL` for concurrent reads
- Coverage updates happen only in aggregate phase (serial)
- Test data creation should use unique identifiers

---

## Metrics and Reporting

### Coverage Over Time

```sql
-- Coverage trend query
SELECT
    date(last_tested_at) as test_date,
    COUNT(*) as paths_tested,
    SUM(CASE WHEN last_result = 'pass' THEN 1 ELSE 0 END) as passed,
    AVG(last_duration_ms) as avg_duration
FROM tui_test_coverage
WHERE last_tested_at > date('now', '-30 days')
GROUP BY date(last_tested_at)
ORDER BY test_date;
```

### Speedup Analysis

```
Sequential time estimate: paths × avg_duration = 45 × 12s = 540s (9 min)
Parallel time (4 sessions): 540s / 4 = 135s (2.25 min)
Actual time (with overhead): 180s (3 min)
Effective speedup: 3.0x
```

---

## Revision History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-17 | 1.1 | Spec refinement: Simplified test paths to user-visible behavior, removed internal state tracking, accepted monolithic design |
| 2026-01-17 | 1.0 | Initial workflow specification |
