#!/bin/bash
# scripts/tui-debug.sh - TMux TUI debugging session manager
#
# Usage:
#   ./scripts/tui-debug.sh              # Start with defaults
#   ./scripts/tui-debug.sh start        # Start fresh session
#   ./scripts/tui-debug.sh stop         # Kill session
#   ./scripts/tui-debug.sh restart      # Kill and restart
#   ./scripts/tui-debug.sh status       # Check if running
#   ./scripts/tui-debug.sh attach       # Attach interactively
#   ./scripts/tui-debug.sh capture      # Capture current screen
#   ./scripts/tui-debug.sh log          # Show TUI stderr log

set -e

SESSION="tui_debug"
WIDTH=120
HEIGHT=40
LOG_FILE="/tmp/tui_debug.log"

# Find binary (prefer release)
find_binary() {
    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local workspace_dir="$(dirname "$script_dir")"

    if [[ -x "$workspace_dir/target/release/casparian" ]]; then
        echo "$workspace_dir/target/release/casparian"
    elif [[ -x "$workspace_dir/target/debug/casparian" ]]; then
        echo "$workspace_dir/target/debug/casparian"
    else
        echo ""
    fi
}

start_session() {
    local binary=$(find_binary)

    if [[ -z "$binary" ]]; then
        echo "ERROR: casparian binary not found. Run 'cargo build --release' first."
        exit 1
    fi

    # Kill existing session if any
    tmux kill-session -t "$SESSION" 2>/dev/null || true

    # Clear old log
    > "$LOG_FILE"

    # Start new session with logging
    # TUI runs in the session, stderr goes to log file
    tmux new-session -d -s "$SESSION" -x "$WIDTH" -y "$HEIGHT" \
        "RUST_BACKTRACE=1 $binary tui 2>$LOG_FILE; echo 'TUI exited. Press Enter to close.'; read"

    echo "Session '$SESSION' started (${WIDTH}x${HEIGHT})"
    echo "Binary: $binary"
    echo "Log: $LOG_FILE"
    echo ""
    echo "Waiting for TUI startup..."
    sleep 1

    # Capture and show initial state
    echo ""
    echo "=== INITIAL SCREEN ==="
    tmux capture-pane -t "$SESSION" -p
    echo "=== END INITIAL SCREEN ==="
    echo ""
    echo "Commands:"
    echo "  ./scripts/tui-send.sh <keys>     # Send keystrokes"
    echo "  ./scripts/tui-capture.sh         # Capture screen"
    echo "  ./scripts/tui-debug.sh attach    # Interactive view"
    echo "  ./scripts/tui-debug.sh stop      # Kill session"
}

stop_session() {
    if tmux has-session -t "$SESSION" 2>/dev/null; then
        tmux kill-session -t "$SESSION"
        echo "Session '$SESSION' stopped"
    else
        echo "Session '$SESSION' not running"
    fi
}

check_status() {
    if tmux has-session -t "$SESSION" 2>/dev/null; then
        echo "Session '$SESSION' is RUNNING"
        echo ""
        echo "Pane info:"
        tmux list-panes -t "$SESSION" -F "  Size: #{pane_width}x#{pane_height}, PID: #{pane_pid}"
    else
        echo "Session '$SESSION' is NOT running"
    fi
}

capture_screen() {
    if ! tmux has-session -t "$SESSION" 2>/dev/null; then
        echo "ERROR: Session '$SESSION' not running. Start with: ./scripts/tui-debug.sh start"
        exit 1
    fi

    echo "=== SCREEN CAPTURE ($(date +%H:%M:%S)) ==="
    tmux capture-pane -t "$SESSION" -p
    echo "=== END CAPTURE ==="
}

show_log() {
    if [[ -f "$LOG_FILE" ]]; then
        echo "=== TUI LOG ($LOG_FILE) ==="
        cat "$LOG_FILE"
        echo "=== END LOG ==="
    else
        echo "No log file found at $LOG_FILE"
    fi
}

attach_session() {
    if ! tmux has-session -t "$SESSION" 2>/dev/null; then
        echo "ERROR: Session '$SESSION' not running. Start with: ./scripts/tui-debug.sh start"
        exit 1
    fi

    echo "Attaching to session '$SESSION'..."
    echo "(Detach with Ctrl+B, then D)"
    tmux attach -t "$SESSION"
}

# Main
case "${1:-start}" in
    start)
        start_session
        ;;
    stop)
        stop_session
        ;;
    restart)
        stop_session
        sleep 0.5
        start_session
        ;;
    status)
        check_status
        ;;
    capture)
        capture_screen
        ;;
    log)
        show_log
        ;;
    attach)
        attach_session
        ;;
    *)
        echo "Usage: $0 {start|stop|restart|status|capture|log|attach}"
        exit 1
        ;;
esac
