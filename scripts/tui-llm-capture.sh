#!/bin/bash
# scripts/tui-llm-capture.sh - Capture a labeled tmux scenario for LLM review

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/tui-env.sh"

OUT_DIR=".test_output/tui_tmux_captures"
SCENARIO="critical_path"
SESSION="tui"
DELAY="0.4"

usage() {
    echo "Usage: $0 [--out DIR] [--scenario NAME] [--delay SECONDS]"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --out)
            OUT_DIR="$2"
            shift 2
            ;;
        --scenario)
            SCENARIO="$2"
            shift 2
            ;;
        --delay)
            DELAY="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown arg: $1"
            usage
            exit 1
            ;;
    esac
done

CAPTURE_DIR="$OUT_DIR/$SCENARIO"
mkdir -p "$CAPTURE_DIR"

MD_FILE="$CAPTURE_DIR/${SCENARIO}.md"

# Start a fresh session
./scripts/tui-test-workflow.sh restart >/dev/null

count=0

capture_step() {
    local label="$1"
    local keys="${2:-}"
    local file
    file=$(printf "%s/%02d_%s.txt" "$CAPTURE_DIR" "$count" "$label")

    if [[ -n "$keys" ]]; then
        ./scripts/tui-test-workflow.sh send "$keys" >/dev/null
        sleep "$DELAY"
    fi

    ./scripts/tui-capture-stable.sh -t "$SESSION" > "$file"

    if [[ $count -eq 0 ]]; then
        echo "# Tmux Captures: $SCENARIO" > "$MD_FILE"
        echo "" >> "$MD_FILE"
    fi

    echo "## $(printf '%02d' "$count") $label" >> "$MD_FILE"
    if [[ -n "$keys" ]]; then
        echo "Keys: $keys" >> "$MD_FILE"
    else
        echo "Keys: (none)" >> "$MD_FILE"
    fi
    echo "" >> "$MD_FILE"
    echo "\`\`\`text" >> "$MD_FILE"
    cat "$file" >> "$MD_FILE"
    echo "\`\`\`" >> "$MD_FILE"
    echo "" >> "$MD_FILE"

    count=$((count + 1))
}

capture_step "home" ""

capture_step "discover" "1"

capture_step "sources_dropdown" "1"

capture_step "discover_back" "Escape"

capture_step "tags_dropdown" "2"

capture_step "discover_back" "Escape"

capture_step "rule_creation" "n"

capture_step "discover_back" "Escape"

capture_step "jobs" "3"

capture_step "jobs_drawer" "J"

capture_step "jobs" "J"

capture_step "query" "6"
