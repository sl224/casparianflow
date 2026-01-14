#!/bin/bash
# scripts/tui-send.sh - Send keystrokes to TUI debug session
#
# Usage:
#   ./scripts/tui-send.sh "hello"        # Send characters
#   ./scripts/tui-send.sh Enter          # Send Enter key
#   ./scripts/tui-send.sh Escape         # Send Escape key
#   ./scripts/tui-send.sh Down           # Send Down arrow
#   ./scripts/tui-send.sh C-c            # Send Ctrl+C
#   ./scripts/tui-send.sh -l "1"         # Send literal "1" (not F1)
#   ./scripts/tui-send.sh -c "1"         # Send "1" and capture screen
#
# Special keys (tmux names):
#   Enter, Escape, Tab, BSpace (backspace)
#   Up, Down, Left, Right
#   F1, F2, F3, ... F12
#   C-a, C-b, ... (Ctrl+key)
#   M-a, M-b, ... (Alt+key)
#   Space, Home, End, PgUp, PgDn

set -e

SESSION="tui_debug"
DELAY="0.2"  # Default delay after sending
CAPTURE=false
LITERAL=false

# Parse flags
while getopts "lcd:h" opt; do
    case $opt in
        l)
            LITERAL=true
            ;;
        c)
            CAPTURE=true
            ;;
        d)
            DELAY="$OPTARG"
            ;;
        h)
            echo "Usage: $0 [-l] [-c] [-d delay] <keys>"
            echo ""
            echo "Flags:"
            echo "  -l        Send keys literally (don't interpret as special)"
            echo "  -c        Capture screen after sending"
            echo "  -d <sec>  Delay after sending (default: 0.2)"
            echo ""
            echo "Special keys: Enter, Escape, Tab, BSpace, Up, Down, Left, Right,"
            echo "              F1-F12, C-c (Ctrl+C), M-x (Alt+X), Space, Home, End"
            exit 0
            ;;
        \?)
            exit 1
            ;;
    esac
done
shift $((OPTIND-1))

if [[ $# -eq 0 ]]; then
    echo "Usage: $0 [-l] [-c] [-d delay] <keys>"
    echo "Example: $0 Enter"
    echo "Example: $0 -c '1'  # Send '1' and capture"
    exit 1
fi

# Check session exists
if ! tmux has-session -t "$SESSION" 2>/dev/null; then
    echo "ERROR: Session '$SESSION' not running."
    echo "Start with: ./scripts/tui-debug.sh start"
    exit 1
fi

KEYS="$1"

# Send keys
if [[ "$LITERAL" == true ]]; then
    tmux send-keys -t "$SESSION" -l "$KEYS"
    echo "Sent (literal): '$KEYS'"
else
    tmux send-keys -t "$SESSION" "$KEYS"
    echo "Sent: $KEYS"
fi

# Wait for TUI to process
sleep "$DELAY"

# Optionally capture
if [[ "$CAPTURE" == true ]]; then
    echo ""
    echo "=== SCREEN AFTER '$KEYS' ==="
    tmux capture-pane -t "$SESSION" -p
    echo "=== END ==="
fi
