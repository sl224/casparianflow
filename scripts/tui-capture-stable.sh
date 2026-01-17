#!/bin/bash
# scripts/tui-capture-stable.sh - Capture TUI screen after render stabilizes
#
# Usage:
#   ./scripts/tui-capture-stable.sh              # Uses session "tui" by default
#   ./scripts/tui-capture-stable.sh -t session   # Specify tmux session

set -e

SESSION="tui"
ATTEMPTS=8
DELAY=0.2

while getopts "t:h" opt; do
    case $opt in
        t)
            SESSION="$OPTARG"
            ;;
        h)
            echo "Usage: $0 [-t session]"
            exit 0
            ;;
        \?)
            exit 1
            ;;
    esac
done

if ! tmux has-session -t "$SESSION" 2>/dev/null; then
    echo "ERROR: Session '$SESSION' not running."
    exit 1
fi

last=""
current=""

for _ in $(seq 1 "$ATTEMPTS"); do
    current=$(tmux capture-pane -t "$SESSION" -p 2>/dev/null || echo "")
    if [[ -n "$last" && "$current" == "$last" ]]; then
        echo "$current"
        exit 0
    fi
    last="$current"
    sleep "$DELAY"
done

echo "$current"
