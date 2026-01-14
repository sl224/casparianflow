#!/bin/bash
# scripts/tui-capture.sh - Capture TUI screen with annotation
#
# Usage:
#   ./scripts/tui-capture.sh                    # Capture with timestamp
#   ./scripts/tui-capture.sh "after pressing 1" # Capture with label
#   ./scripts/tui-capture.sh -s                 # Include scrollback history
#   ./scripts/tui-capture.sh -f screen.txt      # Save to file
#   ./scripts/tui-capture.sh -q                 # Quiet (no header/footer)

set -e

SESSION="tui_debug"
LABEL=""
SCROLLBACK=false
OUTPUT_FILE=""
QUIET=false

# Parse flags
while getopts "sf:qh" opt; do
    case $opt in
        s)
            SCROLLBACK=true
            ;;
        f)
            OUTPUT_FILE="$OPTARG"
            ;;
        q)
            QUIET=true
            ;;
        h)
            echo "Usage: $0 [-s] [-f file] [-q] [label]"
            echo ""
            echo "Flags:"
            echo "  -s        Include scrollback history (last 200 lines)"
            echo "  -f <file> Save capture to file"
            echo "  -q        Quiet mode (no headers)"
            echo ""
            echo "Examples:"
            echo "  $0                          # Basic capture"
            echo "  $0 'after pressing Enter'   # With label"
            echo "  $0 -s                       # With scrollback"
            echo "  $0 -f /tmp/screen.txt       # Save to file"
            exit 0
            ;;
        \?)
            exit 1
            ;;
    esac
done
shift $((OPTIND-1))

LABEL="${1:-}"

# Check session exists
if ! tmux has-session -t "$SESSION" 2>/dev/null; then
    echo "ERROR: Session '$SESSION' not running."
    echo "Start with: ./scripts/tui-debug.sh start"
    exit 1
fi

# Build capture command
CAPTURE_ARGS=("-t" "$SESSION" "-p")
if [[ "$SCROLLBACK" == true ]]; then
    CAPTURE_ARGS+=("-S" "-200")
fi

# Capture
CONTENT=$(tmux capture-pane "${CAPTURE_ARGS[@]}")

# Format output
if [[ "$QUIET" == true ]]; then
    OUTPUT="$CONTENT"
else
    TIMESTAMP=$(date +%H:%M:%S)
    if [[ -n "$LABEL" ]]; then
        HEADER="=== $LABEL ($TIMESTAMP) ==="
    else
        HEADER="=== SCREEN CAPTURE ($TIMESTAMP) ==="
    fi
    OUTPUT="$HEADER"$'\n'"$CONTENT"$'\n'"=== END ==="
fi

# Output
if [[ -n "$OUTPUT_FILE" ]]; then
    echo "$OUTPUT" > "$OUTPUT_FILE"
    echo "Captured to: $OUTPUT_FILE"
else
    echo "$OUTPUT"
fi
