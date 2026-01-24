#!/bin/bash
# TUI Parallel Coverage Runner
# Per specs/meta/tui_parallel_coverage_workflow.md
#
# ANALYSIS ONLY: This script detects and documents TUI issues but does NOT fix them.
# Output includes rich context (screen captures, key sequences) for a downstream LLM to fix.
#
# Usage:
#   ./scripts/tui-parallel-run.sh [OPTIONS]
#
# Options:
#   --max-parallel N       Max concurrent sessions (default: auto)
#   --priority-mode MODE   staleness|risk|gaps|balanced (default: balanced)
#   --coverage-target N    Stop at N% coverage (default: 100)
#   --time-budget N        Max minutes to run (default: unlimited)
#   --view VIEW            Limit to specific view (e.g., Discover)
#   --dry-run              Show what would be tested without running
#   --context-file PATH    Inject run context into results (text file)
#   --checkpoint NAME      Use built-in context template (scan_tag_rule|parser_author|parser_run|output_view)
#   --explore              Run exploratory parallel sessions (no fixed paths)
#   --explore-actions N    Max actions per worker (default: 150)
#   --explore-time N       Max seconds per worker (default: 90)
#   --explore-limit N      Max repeats per screen signature (default: 3)

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DB_PATH="${HOME}/.casparian_flow/casparian_flow.duckdb"
BINARY="$PROJECT_ROOT/target/release/casparian"
SESSION_PREFIX="tui-parallel"
WIDTH=120
HEIGHT=40

# Per-run session homes (isolates DB/state across parallel sessions)
RUN_ID=""
RUN_ROOT=""

# Defaults
MAX_PARALLEL=""
PRIORITY_MODE="balanced"
COVERAGE_TARGET=100
TIME_BUDGET=""
VIEW_FILTER=""
DRY_RUN=false
MODE="paths"
EXPLORE_ACTIONS=150
EXPLORE_TIME=90
EXPLORE_COVERAGE_LIMIT=3
CONTEXT_FILE=""
CONTEXT_TEXT=""
CONTEXT_JSON=""
CHECKPOINT=""
CONTEXT_ENTRY_KEYS=""
CONTEXT_MOCK_TREE=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

log_info()    { echo -e "${CYAN}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[PASS]${NC} $1"; }
log_fail()    { echo -e "${RED}[FAIL]${NC} $1"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_header()  { echo -e "\n${BOLD}=== $1 ===${NC}\n"; }

prepare_mock_tree() {
    local target="$1"
    if [[ -z "$target" ]]; then
        return 0
    fi
    python3 - "$target" <<'PY'
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
if root.exists():
    for dirpath, dirnames, filenames in os.walk(root, topdown=False):
        for f in filenames:
            os.remove(Path(dirpath) / f)
        for d in dirnames:
            os.rmdir(Path(dirpath) / d)
    os.rmdir(root)

paths = []
for i in range(24):
    paths.append(f"health/hl7/admission_in/ADT_202401{(i % 28) + 1:02d}_{i}.hl7")
for i in range(22):
    paths.append(f"health/hl7/adm_in/ADT_202401{(i % 28) + 1:02d}_{i}.hl7")
for i in range(20):
    paths.append(f"health/hl7/lab_in/ORU_202401{(i % 28) + 1:02d}_{i}.hl7")
for i in range(18):
    paths.append(f"health/hl7/lab_out/ORU_2024-01-{(i % 28) + 1:02d}_{i}.hl7")
for i in range(16):
    paths.append(f"health/hl7/facility_a/inbound/ADT_202401{(i % 28) + 1:02d}_{i}.hl7")
for i in range(14):
    paths.append(f"health/hl7/facility_a/in/ADT_202401{(i % 28) + 1:02d}_{i}.hl7")

for i in range(28):
    paths.append(f"defense/mission_{i:03d}/satA/2024/01/15/telemetry_{i:04d}.csv")
for i in range(26):
    paths.append(f"defense/msn_{i:03d}/satA/2024/01/15/telemetry_{i:04d}.csv")
for i in range(18):
    paths.append(f"defense/mission_{i:03d}/satA/2024/01/15/telemetry_{i:04d}.json")
for i in range(16):
    paths.append(f"defense/patrol_{i:03d}/uav_01/2024/01/15/telemetry_{i:04d}.csv")
for i in range(16):
    paths.append(f"defense/ptl_{i:03d}/uav_01/2024/01/15/telemetry_{i:04d}.csv")
for i in range(12):
    paths.append(f"defense/mission_{i:03d}/satA/2024/01/15/imagery_{i:04d}.tif")

for i in range(1, 21):
    paths.append(f"finance/netsuite/exports/2024/01/fin_export_202401{i:02d}.csv")
for i in range(1, 19):
    paths.append(f"finance/ns/exports/2024/01/fin_export_202401{i:02d}.xlsx")
for i in range(1, 17):
    paths.append(f"finance/saved_search/ap_aging/2024/01/transactions_202401{i:02d}.csv")
for i in range(1, 17):
    paths.append(f"finance/saved_search/ap_age/2024/01/transactions_202401{i:02d}.xlsx")
for i in range(1, 13):
    paths.append(f"finance/payroll/exports/2024/01/payroll_202401{i:02d}.csv")
for i in range(1, 11):
    paths.append(f"finance/gl/exports/2024/01/general_ledger_202401{i:02d}.csv")

root.mkdir(parents=True, exist_ok=True)
for rel in paths:
    p = root / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text("fixture")
PY
}

capture_stable() {
    local session_name="$1"
    local attempts=8
    local delay=0.2
    local last=""
    local current=""

    for _ in $(seq 1 "$attempts"); do
        current=$(tmux capture-pane -t "$session_name" -p 2>/dev/null || echo "")
        if [[ -n "$last" && "$current" == "$last" ]]; then
            echo "$current"
            return 0
        fi
        last="$current"
        sleep "$delay"
    done

    echo "$current"
    return 0
}

screen_signature() {
    local output="$1"
    local first_line
    first_line=$(echo "$output" | awk 'NF {print; exit}')
    if [[ -z "$first_line" ]]; then
        echo "empty"
        return 0
    fi
    echo "$first_line" | tr -s ' ' | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
}

pick_key() {
    local seed="$1"
    local step="$2"
    local keys="$3"
    python3 - "$seed" "$step" "$keys" <<'PY'
import random
import sys
seed = int(sys.argv[1])
step = int(sys.argv[2])
keys = sys.argv[3].split()
if not keys:
    print("")
    raise SystemExit(0)
rng = random.Random(seed + step)
print(rng.choice(keys))
PY
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --max-parallel)
            MAX_PARALLEL="$2"
            shift 2
            ;;
        --priority-mode)
            PRIORITY_MODE="$2"
            shift 2
            ;;
        --coverage-target)
            COVERAGE_TARGET="$2"
            shift 2
            ;;
        --time-budget)
            TIME_BUDGET="$2"
            shift 2
            ;;
        --view)
            VIEW_FILTER="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --context-file)
            CONTEXT_FILE="$2"
            shift 2
            ;;
        --checkpoint)
            CHECKPOINT="$2"
            shift 2
            ;;
        --explore)
            MODE="explore"
            shift
            ;;
        --explore-actions)
            EXPLORE_ACTIONS="$2"
            shift 2
            ;;
        --explore-time)
            EXPLORE_TIME="$2"
            shift 2
            ;;
        --explore-limit)
            EXPLORE_COVERAGE_LIMIT="$2"
            shift 2
            ;;
        --help|-h)
            echo "TUI Parallel Coverage Runner"
            echo ""
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --max-parallel N       Max concurrent sessions (default: auto-detect)"
            echo "  --priority-mode MODE   staleness|risk|gaps|balanced (default: balanced)"
            echo "  --coverage-target N    Stop at N% coverage (default: 100)"
            echo "  --time-budget N        Max minutes to run (default: unlimited)"
            echo "  --view VIEW            Limit to specific view (e.g., Discover)"
            echo "  --dry-run              Show what would be tested without running"
            echo "  --context-file PATH    Inject run context into results (text file)"
            echo "  --checkpoint NAME      Use built-in context template"
            echo "  --explore              Run exploratory parallel sessions (no fixed paths)"
            echo "  --explore-actions N    Max actions per worker (default: 150)"
            echo "  --explore-time N       Max seconds per worker (default: 90)"
            echo "  --explore-limit N      Max repeats per screen signature (default: 3)"
            echo "  --help                 Show this help"
            exit 0
            ;;
        *)
            log_fail "Unknown option: $1"
            exit 1
            ;;
    esac
done

if [[ -n "$CHECKPOINT" && -z "$CONTEXT_FILE" ]]; then
    case "$CHECKPOINT" in
        scan_tag_rule)
            CONTEXT_FILE="$PROJECT_ROOT/scripts/contexts/tui_checkpoint_scan_tag_rule.md"
            ;;
        parser_author)
            CONTEXT_FILE="$PROJECT_ROOT/scripts/contexts/tui_checkpoint_parser_author.md"
            ;;
        parser_run)
            CONTEXT_FILE="$PROJECT_ROOT/scripts/contexts/tui_checkpoint_parser_run.md"
            ;;
        output_view)
            CONTEXT_FILE="$PROJECT_ROOT/scripts/contexts/tui_checkpoint_output_view.md"
            ;;
        *)
            log_fail "Unknown checkpoint: $CHECKPOINT"
            exit 1
            ;;
    esac
fi

# Load context file if provided
if [[ -n "$CONTEXT_FILE" ]]; then
    if [[ ! -f "$CONTEXT_FILE" ]]; then
        log_fail "Context file not found: $CONTEXT_FILE"
        exit 1
    fi
    CONTEXT_TEXT=$(cat "$CONTEXT_FILE")
    CONTEXT_JSON=$(printf '%s' "$CONTEXT_TEXT" | jq -Rs .)
    CONTEXT_ENTRY_KEYS=$(rg -m1 '^<!-- ENTRY_KEYS:' "$CONTEXT_FILE" | sed -E 's/^<!-- ENTRY_KEYS:[[:space:]]*//;s/[[:space:]]*-->$//')
    CONTEXT_MOCK_TREE=$(rg -m1 '^<!-- REQUIRE_MOCK_TREE:' "$CONTEXT_FILE" | sed -E 's/^<!-- REQUIRE_MOCK_TREE:[[:space:]]*//;s/[[:space:]]*-->$//')
    if [[ -n "$CONTEXT_MOCK_TREE" ]]; then
        prepare_mock_tree "$CONTEXT_MOCK_TREE"
    fi
fi

# Calculate max parallelism if not specified
if [[ -z "$MAX_PARALLEL" ]]; then
    if command -v nproc &> /dev/null; then
        cpu_count=$(nproc)
    elif command -v sysctl &> /dev/null; then
        cpu_count=$(sysctl -n hw.ncpu)
    else
        cpu_count=4
    fi
    MAX_PARALLEL=$(( cpu_count / 2 ))
    MAX_PARALLEL=$(( MAX_PARALLEL > 4 ? 4 : MAX_PARALLEL ))
    MAX_PARALLEL=$(( MAX_PARALLEL < 1 ? 1 : MAX_PARALLEL ))
fi

# Initialize coverage database tables if needed
init_coverage_db() {
    log_info "Initializing coverage database..."
    duckdb "$DB_PATH" <<'SQL'
CREATE TABLE IF NOT EXISTS tui_test_coverage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_path_id TEXT NOT NULL UNIQUE,
    view TEXT NOT NULL,
    last_tested_at TEXT,
    times_tested INTEGER DEFAULT 0,
    last_result TEXT,
    last_duration_ms INTEGER,
    findings_count INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tui_test_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    parallel_count INTEGER,
    paths_tested INTEGER,
    paths_passed INTEGER,
    paths_failed INTEGER,
    total_duration_ms INTEGER,
    coverage_percent REAL,
    findings_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_coverage_staleness ON tui_test_coverage(last_tested_at);
CREATE INDEX IF NOT EXISTS idx_coverage_view ON tui_test_coverage(view);
SQL
}

# Define test paths (load from JSON file or use generator)
define_test_paths() {
    local json_file="$PROJECT_ROOT/test_paths.json"
    if [[ -f "$json_file" ]]; then
        cat "$json_file"
        return 0
    fi

    local generator="$SCRIPT_DIR/tui-generate-test-paths.py"
    if [[ -x "$generator" ]]; then
        if command -v python3 &> /dev/null; then
            python3 "$generator" "$PROJECT_ROOT/specs/views" "$PROJECT_ROOT/specs/tui.md"
            return 0
        elif command -v python &> /dev/null; then
            python "$generator" "$PROJECT_ROOT/specs/views" "$PROJECT_ROOT/specs/tui.md"
            return 0
        fi
    fi

    cat <<'EOF'
[]
EOF
}

# Get prioritized test paths
get_prioritized_paths() {
    local paths_json
    paths_json=$(define_test_paths)

    # Filter by view if specified
    if [[ -n "$VIEW_FILTER" ]]; then
        paths_json=$(echo "$paths_json" | jq "[.[] | select(.view == \"$VIEW_FILTER\")]")
    fi

    # Get coverage data from DB (handle empty result)
    local coverage_data
    coverage_data=$(duckdb -json "$DB_PATH" \
        "SELECT test_path_id, last_tested_at, times_tested, last_result FROM tui_test_coverage" 2>/dev/null)
    # duckdb -json returns empty string (not []) when no rows
    if [[ -z "$coverage_data" ]]; then
        coverage_data="[]"
    fi

    # Calculate priority scores and sort
    echo "$paths_json" | jq --argjson coverage "$coverage_data" --arg mode "$PRIORITY_MODE" '
        def days_since(date_str):
            if date_str == null then 30
            else
                # SQLite uses "YYYY-MM-DD HH:MM:SS", convert to ISO format
                (date_str | gsub(" "; "T") | . + "Z" | fromdateiso8601) as $ts |
                ((now - $ts) / 86400) | floor
            end;

        def coverage_for(id):
            ($coverage | map(select(.test_path_id == id)) | first) // null;

        map(. as $path |
            coverage_for($path.id) as $cov |
            {
                path: $path,
                staleness: (if $cov then days_since($cov.last_tested_at) else 30 end),
                times_tested: (if $cov then $cov.times_tested else 0 end),
                last_result: (if $cov then $cov.last_result else "never" end)
            } |
            .priority = (
                if $mode == "staleness" then .staleness
                elif $mode == "gaps" then (30 - .times_tested)
                elif $mode == "risk" then (if .last_result == "fail" then 100 else .staleness end)
                else (.staleness * 0.4 + (30 - .times_tested) * 0.3 + (if .last_result == "fail" then 30 else 0 end) * 0.3)
                end
            )
        ) | sort_by(-.priority) | map(.path)
    '
}

# Run a single test path
run_test_path() {
    local path_json="$1"
    local session_name="$2"
    local result_file="$3"

    local path_id
    path_id=$(echo "$path_json" | jq -r '.id')
    local session_home="$RUN_ROOT/$path_id"
    mkdir -p "$session_home"

    local start_time
    start_time=$(date +%s)

    local status="pass"
    local findings="[]"
    local key_sequence="[]"  # Track all keys for context
    local step_index=0

    # Kill any existing session with same name, then start fresh
    tmux kill-session -t "$session_name" 2>/dev/null || true
    tmux new-session -d -s "$session_name" -x "$WIDTH" -y "$HEIGHT" \
        "CASPARIAN_HOME=\"$session_home\" \"$BINARY\" tui"
    sleep 2

    if [[ -n "$CONTEXT_ENTRY_KEYS" ]]; then
        IFS=',' read -r -a entry_keys <<< "$CONTEXT_ENTRY_KEYS"
        for key in "${entry_keys[@]}"; do
            key=$(echo "$key" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')
            case "$key" in
                Enter|Escape|Tab|Up|Down|Left|Right)
                    tmux send-keys -t "$session_name" "$key"
                    ;;
                *)
                    tmux send-keys -t "$session_name" -l "$key"
                    ;;
            esac
            key_sequence=$(echo "$key_sequence" | jq ". + [\"$key\"]")
            sleep 0.8
        done
    fi

    # Execute entry keys (track for context)
    local entry_keys
    entry_keys=$(echo "$path_json" | jq -r '.entry[]')
    for key in $entry_keys; do
        tmux send-keys -t "$session_name" -l "$key"
        key_sequence=$(echo "$key_sequence" | jq ". + [\"$key\"]")
        sleep 0.8
    done

    # Execute actions with RICH CONTEXT capture
    local actions
    actions=$(echo "$path_json" | jq -c '.actions[]')
    while IFS= read -r action; do
        local keys expect expect_rules
        keys=$(echo "$action" | jq -r '.keys // empty')
        expect=$(echo "$action" | jq -r '.expect // empty')
        expect_rules=$(echo "$action" | jq -c '.expect_rules // []')

        if [[ -n "$keys" ]]; then
            # Handle special keys
            case "$keys" in
                Enter|Escape|Tab|Up|Down|Left|Right)
                    tmux send-keys -t "$session_name" "$keys"
                    ;;
                *)
                    tmux send-keys -t "$session_name" -l "$keys"
                    ;;
            esac
            key_sequence=$(echo "$key_sequence" | jq ". + [\"$keys\"]")
            sleep 0.8
        fi

        if [[ -n "$expect" || "$expect_rules" != "[]" ]]; then
            local output
            output=$(capture_stable "$session_name")
            if [[ -n "$expect" ]] && ! echo "$output" | grep -qi "$expect"; then
                status="fail"
                # Capture RICH CONTEXT for downstream LLM fixing
                local escaped_output
                escaped_output=$(echo "$output" | jq -Rs .)
                findings=$(echo "$findings" | jq ". + [{
                    \"type\": \"expectation_failed\",
                    \"expected\": \"$expect\",
                    \"actual_screen\": $escaped_output,
                    \"keys_sent\": \"$keys\",
                    \"full_key_sequence\": $key_sequence,
                    \"step_index\": $step_index,
                    \"visibility_assertions\": [],
                    \"quality_signals\": [],
                    \"example_items\": []
                }]")
            fi

            if [[ "$expect_rules" != "[]" ]]; then
                local visibility_result passed quality_signals example_items
                visibility_result=$(echo "$output" | "$SCRIPT_DIR/tui-visibility-check.sh" "$expect_rules")
                passed=$(echo "$visibility_result" | jq -r '.passed')
                if [[ "$passed" != "true" ]]; then
                    status="fail"
                    escaped_output=$(echo "$output" | jq -Rs .)
                    quality_signals=$(echo "$visibility_result" | jq -c '.quality_signals')
                    example_items=$(echo "$visibility_result" | jq -c '.example_items')
                    findings=$(echo "$findings" | jq ". + [{
                        \"type\": \"low_information_value\",
                        \"expected\": \"$expect\",
                        \"actual_screen\": $escaped_output,
                        \"keys_sent\": \"$keys\",
                        \"full_key_sequence\": $key_sequence,
                        \"step_index\": $step_index,
                        \"visibility_assertions\": $expect_rules,
                        \"quality_signals\": $quality_signals,
                        \"example_items\": $example_items
                    }]")
                fi
            fi
        fi
        step_index=$((step_index + 1))
    done <<< "$actions"

    # Execute exit keys
    local exit_keys
    exit_keys=$(echo "$path_json" | jq -r '.exit[]')
    for key in $exit_keys; do
        case "$key" in
            Enter|Escape|Tab|Up|Down|Left|Right)
                tmux send-keys -t "$session_name" "$key"
                ;;
            *)
                tmux send-keys -t "$session_name" -l "$key"
                ;;
        esac
        sleep 0.6
    done

    # Kill session
    tmux kill-session -t "$session_name" 2>/dev/null || true

    local end_time
    end_time=$(date +%s)
    local duration_sec=$((end_time - start_time))
    local duration=$((duration_sec * 1000))

    # Write result
    local context_field=""
    if [[ -n "$CONTEXT_JSON" ]]; then
        context_field=$',\n  "context": '"$CONTEXT_JSON"
    fi

    cat > "$result_file" <<EOF
{
  "test_path_id": "$path_id",
  "status": "$status",
  "duration_ms": $duration,
  "findings": $findings,
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"$context_field
}
EOF
}

run_explore_worker() {
    local path_json="$1"
    local session_name="$2"
    local result_file="$3"
    local coverage_map="$4"

    local path_id focus key_bias seed max_actions time_budget coverage_limit
    path_id=$(echo "$path_json" | jq -r '.id')
    focus=$(echo "$path_json" | jq -r '.focus')
    key_bias=$(echo "$path_json" | jq -r '.key_bias')
    seed=$(echo "$path_json" | jq -r '.seed')
    max_actions=$(echo "$path_json" | jq -r '.max_actions')
    time_budget=$(echo "$path_json" | jq -r '.time_budget')
    coverage_limit=$(echo "$path_json" | jq -r '.coverage_limit')

    local session_home="$RUN_ROOT/$path_id"
    mkdir -p "$session_home"

    local nav_keys="1 2 3 4 0 Escape Tab ?"
    local default_keys="Enter Escape Tab Up Down Left Right j k / ? s n R M g 0 1 2 3 4 BSpace"
    local start_time
    start_time=$(date +%s)

    local status="pass"
    local findings="[]"
    local key_sequence="[]"
    local step_index=0

    tmux kill-session -t "$session_name" 2>/dev/null || true
    tmux new-session -d -s "$session_name" -x "$WIDTH" -y "$HEIGHT" \
        "CASPARIAN_HOME=\"$session_home\" \"$BINARY\" tui"
    sleep 1

    if [[ -n "$CONTEXT_ENTRY_KEYS" ]]; then
        IFS=',' read -r -a entry_keys <<< "$CONTEXT_ENTRY_KEYS"
        for key in "${entry_keys[@]}"; do
            key=$(echo "$key" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')
            case "$key" in
                Enter|Escape|Tab|Up|Down|Left|Right)
                    tmux send-keys -t "$session_name" "$key"
                    ;;
                *)
                    tmux send-keys -t "$session_name" -l "$key"
                    ;;
            esac
            key_sequence=$(echo "$key_sequence" | jq ". + [\"$key\"]")
            sleep 0.3
        done
    fi

    while [[ $step_index -lt $max_actions ]]; do
        local now elapsed
        now=$(date +%s)
        elapsed=$((now - start_time))
        if [[ $elapsed -ge $time_budget ]]; then
            break
        fi

        local output signature count key_pool key
        output=$(capture_stable "$session_name")
        signature=$(screen_signature "$output")
        count=$(python3 "$SCRIPT_DIR/tui-coverage-map.py" --map "$coverage_map" --signature "$signature" --op update)

        if [[ $count -ge $coverage_limit ]]; then
            key_pool="$nav_keys"
        else
            key_pool="$key_bias $default_keys"
        fi

        local visibility_result passed quality_signals example_items
        visibility_result=$(echo "$output" | "$SCRIPT_DIR/tui-visibility-check.sh" '["must_show_suffix","must_differentiate"]' 2>/dev/null || echo '{"passed": true, "quality_signals": [], "example_items": []}')
        passed=$(echo "$visibility_result" | jq -r '.passed')
        if [[ "$passed" != "true" ]]; then
            status="issues"
            local escaped_output
            escaped_output=$(echo "$output" | jq -Rs .)
            quality_signals=$(echo "$visibility_result" | jq -c '.quality_signals')
            example_items=$(echo "$visibility_result" | jq -c '.example_items')
            findings=$(echo "$findings" | jq ". + [{
                \"type\": \"low_information_value\",
                \"actual_screen\": $escaped_output,
                \"keys_sent\": \"\",
                \"full_key_sequence\": $key_sequence,
                \"step_index\": $step_index,
                \"visibility_assertions\": [\"must_show_suffix\", \"must_differentiate\"],
                \"quality_signals\": $quality_signals,
                \"example_items\": $example_items,
                \"user_value_note\": \"List output is ambiguous or hides important suffix details\"
            }]")
        fi

        if echo "$output" | grep -Eiq "(panic|traceback|error:)"; then
            status="issues"
            local escaped_output error_lines
            escaped_output=$(echo "$output" | jq -Rs .)
            error_lines=$(echo "$output" | grep -Ei "(panic|traceback|error:)" | head -n 3 | tr '\n' ' ' | jq -Rs .)
            findings=$(echo "$findings" | jq ". + [{
                \"type\": \"error_text_detected\",
                \"actual_screen\": $escaped_output,
                \"keys_sent\": \"\",
                \"full_key_sequence\": $key_sequence,
                \"step_index\": $step_index,
                \"quality_signals\": [\"error_text\"],
                \"example_items\": [],
                \"user_value_note\": \"User-visible error text detected during exploration\",
                \"error_excerpt\": $error_lines
            }]")
        fi

        key=$(pick_key "$seed" "$step_index" "$key_pool")
        if [[ -z "$key" ]]; then
            break
        fi

        case "$key" in
            Backspace)
                tmux send-keys -t "$session_name" "BSpace"
                ;;
            Enter|Escape|Tab|Up|Down|Left|Right|BSpace)
                tmux send-keys -t "$session_name" "$key"
                ;;
            *)
                tmux send-keys -t "$session_name" -l "$key"
                ;;
        esac
        key_sequence=$(echo "$key_sequence" | jq ". + [\"$key\"]")
        step_index=$((step_index + 1))
        sleep 0.3
    done

    tmux kill-session -t "$session_name" 2>/dev/null || true

    local end_time
    end_time=$(date +%s)
    local duration_sec=$((end_time - start_time))
    local duration=$((duration_sec * 1000))

    local context_field=""
    if [[ -n "$CONTEXT_JSON" ]]; then
        context_field=$',\n  "context": '"$CONTEXT_JSON"
    fi

    cat > "$result_file" <<EOF
{
  "test_path_id": "$path_id",
  "status": "$status",
  "duration_ms": $duration,
  "findings": $findings,
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "explore_context": {
    "focus": "$focus",
    "key_bias": "$key_bias",
    "seed": $seed,
    "max_actions": $max_actions,
    "time_budget": $time_budget
  }$context_field
}
EOF
}

# Update coverage database
update_coverage() {
    local result_file="$1"

    if [[ "$MODE" == "explore" ]]; then
        return 0
    fi

    local path_id status duration
    path_id=$(jq -r '.test_path_id' "$result_file")
    status=$(jq -r '.status' "$result_file")
    duration=$(jq -r '.duration_ms' "$result_file")
    findings_count=$(jq '.findings | length' "$result_file")

    # Get view from test paths
    local view
    view=$(define_test_paths | jq -r ".[] | select(.id == \"$path_id\") | .view")

    duckdb "$DB_PATH" <<SQL
INSERT INTO tui_test_coverage (test_path_id, view, last_tested_at, times_tested, last_result, last_duration_ms, findings_count)
VALUES ('$path_id', '$view', datetime('now'), 1, '$status', $duration, $findings_count)
ON CONFLICT(test_path_id) DO UPDATE SET
    last_tested_at = datetime('now'),
    times_tested = times_tested + 1,
    last_result = '$status',
    last_duration_ms = $duration,
    findings_count = $findings_count;
SQL
}

# Main execution
main() {
    log_header "TUI Parallel Coverage Workflow"

    log_info "Configuration:"
    echo "  Mode: $MODE"
    echo "  Max parallel sessions: $MAX_PARALLEL"
    echo "  Priority mode: $PRIORITY_MODE"
    echo "  Coverage target: $COVERAGE_TARGET%"
    [[ -n "$VIEW_FILTER" ]] && echo "  View filter: $VIEW_FILTER"
    [[ -n "$TIME_BUDGET" ]] && echo "  Time budget: ${TIME_BUDGET}m"
    [[ -n "$CONTEXT_FILE" ]] && echo "  Context file: $CONTEXT_FILE"
    if [[ "$MODE" == "explore" ]]; then
        echo "  Explore actions: $EXPLORE_ACTIONS"
        echo "  Explore time: ${EXPLORE_TIME}s"
        echo "  Explore repeat limit: $EXPLORE_COVERAGE_LIMIT"
    fi

    # Check binary exists
    if [[ ! -x "$BINARY" ]]; then
        log_info "Building release binary..."
        cargo build -p casparian --release
    fi

    # Initialize DB
    init_coverage_db

    # Get prioritized paths or generate explore workers
    local paths_json
    local total_paths
    local run_seed
    run_seed=$(date +%s)

    if [[ "$MODE" == "explore" ]]; then
        log_header "Phase 1: Preparing Explore Workers"
        paths_json="[]"
        local focus_list=("navigation" "input" "dialogs" "lists")
        local key_bias_list=("1 2 3 4 0 Escape Tab ?" "a b c d e f g h i j k l m n o p q r s t u v w x y z 0 1 2 3 4 5 6 7 8 9 / Enter BSpace" "n R M s g" "j k Up Down / f")
        for slot in $(seq 0 $((MAX_PARALLEL - 1))); do
            local idx=$((slot % ${#focus_list[@]}))
            local focus="${focus_list[$idx]}"
            local key_bias="${key_bias_list[$idx]}"
            local seed=$((run_seed + slot * 1000))
            paths_json=$(echo "$paths_json" | jq \
                --arg id "explore-worker-$slot" \
                --arg view "Explore" \
                --arg focus "$focus" \
                --arg key_bias "$key_bias" \
                --argjson seed "$seed" \
                --argjson max_actions "$EXPLORE_ACTIONS" \
                --argjson time_budget "$EXPLORE_TIME" \
                --argjson coverage_limit "$EXPLORE_COVERAGE_LIMIT" \
                '. + [{id: $id, view: $view, focus: $focus, key_bias: $key_bias, seed: $seed, max_actions: $max_actions, time_budget: $time_budget, coverage_limit: $coverage_limit}]')
        done
        total_paths=$(echo "$paths_json" | jq 'length')
        log_info "Prepared $total_paths explore workers"
    else
        log_header "Phase 1: Prioritizing Test Paths"
        paths_json=$(get_prioritized_paths)
        total_paths=$(echo "$paths_json" | jq 'length')
        log_info "Found $total_paths test paths to run"
    fi

    if [[ "$DRY_RUN" == "true" ]]; then
        log_warn "DRY RUN - would test these paths:"
        echo "$paths_json" | jq -r '.[].id'
        exit 0
    fi

    # Create run directory
    local run_id
    run_id=$(date +%Y%m%d_%H%M%S)_$$
    RUN_ID="$run_id"
    RUN_ROOT="/tmp/casparian-tui-$run_id"
    mkdir -p "$RUN_ROOT"
    local results_dir="/tmp/tui-parallel-$run_id"
    mkdir -p "$results_dir"
    local coverage_map="$results_dir/coverage_map.tsv"
    : > "$coverage_map"
    if [[ -n "$CONTEXT_TEXT" ]]; then
        printf '%s\n' "$CONTEXT_TEXT" > "$results_dir/context.txt"
    fi

    # Track state using files (bash 3.x compatible - no associative arrays)
    local state_dir="$results_dir/state"
    mkdir -p "$state_dir"
    local timeout_limit=60
    if [[ "$MODE" == "explore" ]]; then
        timeout_limit=$((EXPLORE_TIME + 10))
    fi
    local active_count=0
    local path_index=0
    local completed=0
    local passed=0
    local failed=0
    local start_time
    start_time=$(date +%s)

    log_header "Phase 2: Spawning Parallel Sessions"

    # Main loop
    while [[ $active_count -gt 0 || $path_index -lt $total_paths ]]; do
        # Check time budget
        if [[ -n "$TIME_BUDGET" ]]; then
            local elapsed=$(( ($(date +%s) - start_time) / 60 ))
            if [[ $elapsed -ge $TIME_BUDGET ]]; then
                log_warn "Time budget exceeded ($elapsed minutes)"
                break
            fi
        fi

        # Spawn new sessions if capacity available
        while [[ $active_count -lt $MAX_PARALLEL && $path_index -lt $total_paths ]]; do
            local path_json
            path_json=$(echo "$paths_json" | jq ".[$path_index]")
            local path_id
            path_id=$(echo "$path_json" | jq -r '.id')

            # Find first FREE slot (don't use active_count - it's not the slot number!)
            local slot=-1
            for s in $(seq 0 $((MAX_PARALLEL - 1))); do
                if [[ ! -f "$state_dir/slot_${s}_path" ]]; then
                    slot=$s
                    break
                fi
            done
            if [[ $slot -eq -1 ]]; then
                break  # No free slots (shouldn't happen if active_count is correct)
            fi

            local session_name="${SESSION_PREFIX}-${slot}"

            log_info "[$((path_index + 1))/$total_paths] Starting: $path_id"

            # Track session state in files
            echo "$path_id" > "$state_dir/slot_${slot}_path"
            date +%s > "$state_dir/slot_${slot}_start"

            # Run in background
            if [[ "$MODE" == "explore" ]]; then
                run_explore_worker "$path_json" "$session_name" "$results_dir/${path_id}.json" "$coverage_map" &
            else
                run_test_path "$path_json" "$session_name" "$results_dir/${path_id}.json" &
            fi

            active_count=$((active_count + 1))
            path_index=$((path_index + 1))
        done

        # Check for completed sessions (by result file existence, not tmux session)
        for slot in $(seq 0 $((MAX_PARALLEL - 1))); do
            local slot_file="$state_dir/slot_${slot}_path"
            if [[ -f "$slot_file" ]]; then
                local path_id
                path_id=$(cat "$slot_file")
                local result_file="$results_dir/${path_id}.json"

                # Check if result file exists (test completed)
                if [[ -f "$result_file" ]]; then
                    local status
                    status=$(jq -r '.status' "$result_file" 2>/dev/null || echo "unknown")
                    if [[ "$status" == "pass" ]]; then
                        log_success "Completed: $path_id"
                        passed=$((passed + 1))
                    else
                        log_fail "Failed: $path_id ($status)"
                        failed=$((failed + 1))
                    fi

                    # Update coverage DB
                    update_coverage "$result_file" 2>/dev/null || true

                    rm -f "$slot_file" "$state_dir/slot_${slot}_start"
                    active_count=$((active_count - 1))
                    completed=$((completed + 1))
                fi
            fi
        done

        # Check for stuck sessions (timeout 60s) - only if no result file yet
        for slot in $(seq 0 $((MAX_PARALLEL - 1))); do
            local slot_file="$state_dir/slot_${slot}_path"
            local start_file="$state_dir/slot_${slot}_start"
            if [[ -f "$slot_file" && -f "$start_file" ]]; then
                local path_id
                path_id=$(cat "$slot_file")
                local result_file="$results_dir/${path_id}.json"

                # Only timeout if result doesn't exist
                if [[ ! -f "$result_file" ]]; then
                    local slot_start
                    slot_start=$(cat "$start_file")
                    local elapsed=$(($(date +%s) - slot_start))
                    if [[ $elapsed -gt $timeout_limit ]]; then
                        local session_name="${SESSION_PREFIX}-${slot}"
                        log_warn "Timeout: $path_id"
                        tmux kill-session -t "$session_name" 2>/dev/null || true

                        # Record timeout
                        local context_field=""
                        if [[ -n "$CONTEXT_JSON" ]]; then
                            context_field=$',\n  "context": '"$CONTEXT_JSON"
                        fi
                        cat > "$result_file" <<EOF
{
  "test_path_id": "$path_id",
  "status": "timeout",
  "duration_ms": $((timeout_limit * 1000)),
  "findings": [{"type": "timeout"}],
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"$context_field
}
EOF
                        update_coverage "$result_file" 2>/dev/null || true

                        rm -f "$slot_file" "$start_file"
                        active_count=$((active_count - 1))
                        failed=$((failed + 1))
                        completed=$((completed + 1))
                    fi
                fi
            fi
        done

        sleep 0.5
    done

    # Wait for remaining background jobs
    wait

    log_header "Phase 3: Aggregating Results"

    local end_time
    end_time=$(date +%s)
    local total_duration_ms=$(( (end_time - start_time) * 1000 ))
    local coverage_percent
    coverage_percent=$(echo "scale=1; $passed * 100 / $completed" | bc 2>/dev/null || echo "0")

    # Generate actionable_findings.json
    local all_findings="[]"
    for result_file in "$results_dir"/*.json; do
        # Skip non-result files
        if [[ -f "$result_file" && "$(basename "$result_file")" != "actionable_findings.json" ]]; then
            # Check if file has findings field
            if jq -e '.findings' "$result_file" > /dev/null 2>&1; then
                local file_findings
                file_findings=$(jq '.findings' "$result_file" 2>/dev/null || echo "[]")
                if [[ "$file_findings" != "[]" && "$file_findings" != "null" ]]; then
                    all_findings=$(echo "$all_findings" | jq --slurpfile f "$result_file" '. + [{test_path_id: $f[0].test_path_id, findings: $f[0].findings}]')
                fi
            fi
        fi
    done

    local context_field=""
    if [[ -n "$CONTEXT_JSON" ]]; then
        context_field=$',\n  "context": '"$CONTEXT_JSON"
    fi

    cat > "$results_dir/actionable_findings.json" <<EOF
{
  "workflow": "tui_parallel_coverage",
  "run_id": "$run_id",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "status": "$([ $failed -eq 0 ] && echo 'passed' || echo 'issues')",
  "summary": {
    "total_paths": $completed,
    "passed": $passed,
    "failed": $failed,
    "duration_ms": $total_duration_ms,
    "parallel_sessions": $MAX_PARALLEL,
    "coverage_percent": $coverage_percent
  }$context_field,
  "findings": $all_findings
}
EOF

    # Record run in database (escape JSON for SQL by doubling single quotes)
    local escaped_json
    escaped_json=$(cat "$results_dir/actionable_findings.json" | jq -c . | sed "s/'/''/g")
    duckdb "$DB_PATH" <<SQL
INSERT INTO tui_test_runs (run_id, started_at, completed_at, parallel_count, paths_tested, paths_passed, paths_failed, total_duration_ms, coverage_percent, findings_json)
VALUES ('$run_id', datetime('now', '-$((end_time - start_time)) seconds'), datetime('now'), $MAX_PARALLEL, $completed, $passed, $failed, $total_duration_ms, $coverage_percent, '$escaped_json');
SQL

    log_header "Results"
    echo -e "  Total paths tested: ${BOLD}$completed${NC}"
    echo -e "  Passed: ${GREEN}$passed${NC}"
    echo -e "  Failed: ${RED}$failed${NC}"
    echo -e "  Duration: ${total_duration_ms}ms"
    echo -e "  Pass rate: ${coverage_percent}%"
    echo ""
    echo "  Results: $results_dir/actionable_findings.json"

    # Show failures if any
    if [[ $failed -gt 0 ]]; then
        log_header "Failed Paths"
        for result_file in "$results_dir"/*.json; do
            # Skip aggregated findings file
            if [[ -f "$result_file" && "$(basename "$result_file")" != "actionable_findings.json" ]]; then
                local status
                status=$(jq -r '.status' "$result_file")
                if [[ "$status" != "pass" ]]; then
                    local path_id
                    path_id=$(jq -r '.test_path_id' "$result_file")
                    echo "  - $path_id ($status)"
                fi
            fi
        done
    fi

    # Return exit code based on failures
    [[ $failed -eq 0 ]]
}

main "$@"
