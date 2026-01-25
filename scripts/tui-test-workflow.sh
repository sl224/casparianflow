#!/bin/bash
# TUI Testing Workflow Helper (manual)
# NOTE: Manual-only. Prefer `casparian tui-flow` + `tui-state-graph --render --lint`
# and `tui-ux-lint` for correctness checks.
# Per specs/meta/tui_testing_workflow.md
#
# Usage:
#   ./scripts/tui-test-workflow.sh start          # Start fresh session
#   ./scripts/tui-test-workflow.sh stop           # Kill session
#   ./scripts/tui-test-workflow.sh restart        # Kill and restart
#   ./scripts/tui-test-workflow.sh send "keys"    # Send keystrokes
#   ./scripts/tui-test-workflow.sh capture        # Capture screen
#   ./scripts/tui-test-workflow.sh sc "keys"      # Send and capture
#   ./scripts/tui-test-workflow.sh test-home      # Test home hub
#   ./scripts/tui-test-workflow.sh test-discover  # Test discover mode
#   ./scripts/tui-test-workflow.sh smoke          # Run smoke tests

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/tui-env.sh"

SESSION="tui"
WIDTH=120
HEIGHT=40
BINARY="./target/release/casparian"
DELAY="0.3"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${CYAN}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

# Start a fresh tmux session
cmd_start() {
    if tmux has-session -t "$SESSION" 2>/dev/null; then
        log_warn "Session '$SESSION' already exists. Use 'restart' to replace it."
        return 1
    fi

    if [[ ! -x "$BINARY" ]]; then
        log_info "Binary not found, building..."
        cargo build -p casparian --release
    fi

    log_info "Starting tmux session '$SESSION' (${WIDTH}x${HEIGHT})"
    tmux new-session -d -s "$SESSION" -x "$WIDTH" -y "$HEIGHT" "CASPARIAN_HOME=\"$CASPARIAN_HOME\" \"$BINARY\" tui"
    sleep 1
    log_success "Session started"
    cmd_capture
}

# Stop the tmux session
cmd_stop() {
    if tmux has-session -t "$SESSION" 2>/dev/null; then
        tmux kill-session -t "$SESSION"
        log_success "Session '$SESSION' killed"
    else
        log_warn "No session '$SESSION' to kill"
    fi
}

# Restart (stop + start)
cmd_restart() {
    cmd_stop 2>/dev/null || true
    sleep 0.2
    cmd_start
}

# Send keys to the session
cmd_send() {
    local keys="$1"
    if [[ -z "$keys" ]]; then
        echo "Usage: $0 send <keys>"
        echo "Examples:"
        echo "  $0 send '1'        # Send character 1"
        echo "  $0 send Enter      # Send Enter key"
        echo "  $0 send 'hello'    # Send string"
        return 1
    fi

    if ! tmux has-session -t "$SESSION" 2>/dev/null; then
        log_fail "No session '$SESSION'. Run 'start' first."
        return 1
    fi

    # Check if it's a special key
    case "$keys" in
        Enter|Escape|Tab|BackTab|Up|Down|Left|Right|Space|Home|End|PageUp|PageDown|BSpace)
            tmux send-keys -t "$SESSION" "$keys"
            ;;
        BackSpace)
            # Alias BackSpace to BSpace (tmux's name for backspace)
            tmux send-keys -t "$SESSION" BSpace
            ;;
        C-*)
            tmux send-keys -t "$SESSION" "$keys"
            ;;
        *)
            # Use -l for literal to prevent F-key interpretation
            tmux send-keys -t "$SESSION" -l "$keys"
            ;;
    esac
    log_info "Sent: $keys"
}

# Capture current screen
cmd_capture() {
    if ! tmux has-session -t "$SESSION" 2>/dev/null; then
        log_fail "No session '$SESSION'. Run 'start' first."
        return 1
    fi

    echo "────────────────────────────────────────────────────────────────────"
    tmux capture-pane -t "$SESSION" -p
    echo "────────────────────────────────────────────────────────────────────"
}

# Send and capture (most common operation)
cmd_sc() {
    local keys="$1"
    local delay="${2:-$DELAY}"

    cmd_send "$keys"
    sleep "$delay"
    cmd_capture
}

# Attach to session for manual inspection
cmd_attach() {
    if ! tmux has-session -t "$SESSION" 2>/dev/null; then
        log_fail "No session '$SESSION'. Run 'start' first."
        return 1
    fi
    log_info "Attaching to session. Press Ctrl+B then D to detach."
    tmux attach -t "$SESSION"
}

# Test home hub
cmd_test_home() {
    log_info "Testing Home Hub..."

    cmd_restart
    sleep 0.5

    cmd_capture
    return 0
}

# Test discover mode
cmd_test_discover() {
    log_info "Testing Discover Mode..."

    cmd_restart
    sleep 0.5

    # Navigate to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.5

    cmd_capture

    # Open Sources dropdown
    tmux send-keys -t "$SESSION" -l "1"
    sleep 0.3
    cmd_capture

    # Close dropdown
    tmux send-keys -t "$SESSION" Escape
    sleep 0.2

    # Open Tags dropdown
    tmux send-keys -t "$SESSION" -l "2"
    sleep 0.3
    cmd_capture

    tmux send-keys -t "$SESSION" Escape
    sleep 0.2

    return 0
}

# Test Rule Creation dialog
cmd_test_rule_dialog() {
    log_info "Testing Rule Creation Dialog..."

    cmd_restart
    sleep 0.5

    # Navigate to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.5

    # Open Rule Creation
    tmux send-keys -t "$SESSION" "n"
    sleep 0.3

    cmd_capture

    # Type in pattern field
    tmux send-keys -t "$SESSION" -l "*.txt"
    sleep 0.3
    cmd_capture

    # Tab and type a tag
    tmux send-keys -t "$SESSION" Tab
    sleep 0.2
    tmux send-keys -t "$SESSION" -l "test_tag"
    sleep 0.2
    cmd_capture

    # Close dialog
    tmux send-keys -t "$SESSION" Escape
    sleep 0.5
    cmd_capture

    return 0
}

# Test input latency in Rule Creation dialog
cmd_test_latency() {
    log_info "Testing Input Latency..."

    cmd_restart
    sleep 0.5

    # Navigate to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.5

    # Open Rule Creation dialog
    tmux send-keys -t "$SESSION" "n"
    sleep 0.3

    # Test rapid keystroke handling - type 10 characters quickly
    log_info "Testing rapid keystroke handling (10 chars)..."
    local start_time=$(python3 -c "import time; print(time.time())")

    # Send characters rapidly (no delay between them)
    for char in a b c d e f g h i j; do
        tmux send-keys -t "$SESSION" -l "$char"
    done

    # Small delay for UI to process
    sleep 0.2

    local end_time=$(python3 -c "import time; print(time.time())")
    local elapsed=$(python3 -c "print(f'{($end_time - $start_time) * 1000:.0f}')")

    log_info "Latency sample: ${elapsed}ms total for 10 chars"
    cmd_capture

    # Verify debounce is working - preview should update after delay
    sleep 0.3  # Wait for debounce (150ms) + render
    cmd_capture

    # Test backspace latency (tmux uses BSpace for backspace)
    log_info "Testing backspace handling..."
    for i in {1..5}; do
        tmux send-keys -t "$SESSION" BSpace
    done
    sleep 0.2

    cmd_capture
    return 0
}

# Test glob explorer to rule dialog handoff
cmd_test_glob_handoff() {
    log_info "Testing Glob Explorer → Rule Dialog Handoff..."

    cmd_restart
    sleep 0.5

    # Navigate to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.5

    # Enter glob pattern mode
    tmux send-keys -t "$SESSION" "/"
    sleep 0.2

    # Type a pattern
    tmux send-keys -t "$SESSION" -l "*.txt"
    sleep 0.3

    # Confirm pattern
    tmux send-keys -t "$SESSION" Enter
    sleep 0.3

    cmd_capture

    # Now open rule dialog - should inherit pattern
    tmux send-keys -t "$SESSION" "n"
    sleep 0.3

    cmd_capture
    return 0
}

# Test Jobs mode
cmd_test_jobs() {
    log_info "Testing Jobs Mode..."

    cmd_restart
    sleep 0.5

    # Navigate to Jobs (press 4 from home)
    tmux send-keys -t "$SESSION" "4"
    sleep 0.5

    cmd_capture

    # Return to Home
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    cmd_capture

    return 0
}

# Test Sources Manager dialog
cmd_test_sources_manager() {
    log_info "Testing Sources Manager..."

    cmd_restart
    sleep 0.5

    # Navigate to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.5

    # Open Sources Manager
    tmux send-keys -t "$SESSION" "M"
    sleep 0.3

    cmd_capture

    # Close dialog
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    cmd_capture

    return 0
}

# Test Rules Manager dialog
cmd_test_rules_manager() {
    log_info "Testing Rules Manager..."

    cmd_restart
    sleep 0.5

    # Navigate to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.5

    # Open Rules Manager
    tmux send-keys -t "$SESSION" "R"
    sleep 0.3

    cmd_capture

    # Close dialog
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    cmd_capture

    return 0
}

# Test view navigation with number keys
cmd_test_view_navigation() {
    log_info "Testing View Navigation..."

    cmd_restart
    sleep 0.5

    # Test navigation to Discover (1)
    tmux send-keys -t "$SESSION" "1"
    sleep 0.3
    cmd_capture

    # Return to Home
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    cmd_capture

    # Test navigation to Parser Bench (2)
    tmux send-keys -t "$SESSION" "2"
    sleep 0.3
    cmd_capture

    # Return to Home
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    cmd_capture

    # Test navigation to Inspect (3)
    tmux send-keys -t "$SESSION" "3"
    sleep 0.3
    cmd_capture

    # Return to Home
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    cmd_capture

    # Test navigation to Jobs (4)
    tmux send-keys -t "$SESSION" "4"
    sleep 0.3
    cmd_capture

    return 0
}

# Test state transitions (ESC behavior from various states)
cmd_test_state_transitions() {
    log_info "Testing State Transitions..."

    cmd_restart
    sleep 0.5

    # Go to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.3

    # Test: Sources dropdown → ESC → Files
    tmux send-keys -t "$SESSION" -l "1"
    sleep 0.2
    tmux send-keys -t "$SESSION" Escape
    sleep 0.2
    cmd_capture

    # Test: Tags dropdown → ESC → Files
    tmux send-keys -t "$SESSION" -l "2"
    sleep 0.2
    tmux send-keys -t "$SESSION" Escape
    sleep 0.2
    cmd_capture

    # Test: Rule dialog → ESC → Files
    tmux send-keys -t "$SESSION" "n"
    sleep 0.2
    tmux send-keys -t "$SESSION" Escape
    sleep 0.2
    cmd_capture

    # Test: Nested dialogs preserve parent state
    # Rules Manager → New Rule → ESC → Rules Manager
    tmux send-keys -t "$SESSION" "R"
    sleep 0.2
    cmd_capture
    tmux send-keys -t "$SESSION" "n"
    sleep 0.2
    tmux send-keys -t "$SESSION" Escape
    sleep 0.2
    cmd_capture

    return 0
}

# Run all smoke tests
cmd_smoke() {
    log_info "Running TUI Smoke Tests..."
    echo ""

    cmd_test_home
    echo ""

    cmd_test_discover
    echo ""

    cmd_test_rule_dialog
    echo ""

    cmd_test_latency
    echo ""

    cmd_test_glob_handoff
    echo ""

    cmd_stop
    log_success "Smoke run completed"
    return 0
}

# Run full test suite (smoke + additional tests)
cmd_full() {
    log_info "Running Full TUI Test Suite..."
    echo ""

    # Smoke tests
    cmd_test_home
    echo ""
    cmd_test_discover
    echo ""
    cmd_test_rule_dialog
    echo ""

    # Additional coverage
    cmd_test_view_navigation
    echo ""
    cmd_test_state_transitions
    echo ""
    cmd_test_jobs
    echo ""
    cmd_test_sources_manager
    echo ""
    cmd_test_rules_manager
    echo ""

    # Performance
    cmd_test_latency
    echo ""
    cmd_test_glob_handoff
    echo ""

    cmd_stop
    log_success "Full test run completed"
    return 0
}

# Show usage
cmd_help() {
    echo "TUI Testing Workflow Helper"
    echo ""
    echo "Usage: $0 <command> [args]"
    echo ""
    echo "Session Commands:"
    echo "  start              Start fresh tmux session"
    echo "  stop               Kill tmux session"
    echo "  restart            Kill and restart session"
    echo "  attach             Attach to session for manual inspection"
    echo ""
    echo "Testing Commands:"
    echo "  send <keys>        Send keystrokes to session"
    echo "  capture            Capture and display current screen"
    echo "  sc <keys> [delay]  Send keys and capture (default delay: ${DELAY}s)"
    echo ""
    echo "Automated Tests:"
    echo "  test-home             Test home hub rendering"
    echo "  test-discover         Test discover mode entry and dropdowns"
    echo "  test-rule-dialog      Test rule creation dialog"
    echo "  test-jobs             Test jobs mode"
    echo "  test-sources-manager  Test sources manager dialog"
    echo "  test-rules-manager    Test rules manager dialog"
    echo "  test-view-navigation  Test navigation between views (1/2/3/4)"
    echo "  test-state-transitions Test ESC behavior from various states"
    echo "  test-latency          Test input latency and debouncing"
    echo "  test-glob-handoff     Test glob explorer to rule dialog handoff"
    echo ""
    echo "Test Suites:"
    echo "  smoke              Quick smoke tests (home, discover, rule dialog)"
    echo "  full               Full test suite (all tests)"
    echo ""
    echo "Examples:"
    echo "  $0 start                    # Start session"
    echo "  $0 sc '1'                   # Press 1 and capture"
    echo "  $0 sc Enter 0.5             # Press Enter, wait 0.5s, capture"
    echo "  $0 send 'hello world'       # Type text"
    echo "  $0 smoke                    # Quick smoke tests"
    echo "  $0 full                     # Full test suite"
}

# Main dispatch
case "${1:-help}" in
    start)      cmd_start ;;
    stop)       cmd_stop ;;
    restart)    cmd_restart ;;
    attach)     cmd_attach ;;
    send)       cmd_send "$2" ;;
    capture)    cmd_capture ;;
    sc)         cmd_sc "$2" "$3" ;;
    test-home)  cmd_test_home ;;
    test-discover) cmd_test_discover ;;
    test-rule-dialog) cmd_test_rule_dialog ;;
    test-jobs)  cmd_test_jobs ;;
    test-sources-manager) cmd_test_sources_manager ;;
    test-rules-manager) cmd_test_rules_manager ;;
    test-view-navigation) cmd_test_view_navigation ;;
    test-state-transitions) cmd_test_state_transitions ;;
    test-latency) cmd_test_latency ;;
    test-glob-handoff) cmd_test_glob_handoff ;;
    smoke)      cmd_smoke ;;
    full)       cmd_full ;;
    help|--help|-h) cmd_help ;;
    *)
        log_fail "Unknown command: $1"
        cmd_help
        exit 1
        ;;
esac
