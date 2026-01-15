#!/bin/bash
# TUI Testing Workflow Helper
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
    tmux new-session -d -s "$SESSION" -x "$WIDTH" -y "$HEIGHT" "$BINARY tui"
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

    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    # Check for expected elements
    local passed=0
    local failed=0

    if echo "$output" | grep -q "Discover"; then
        log_success "Home Hub: Discover panel present"
        ((passed++))
    else
        log_fail "Home Hub: Discover panel missing"
        ((failed++))
    fi

    if echo "$output" | grep -q "Parser Bench"; then
        log_success "Home Hub: Parser Bench panel present"
        ((passed++))
    else
        log_fail "Home Hub: Parser Bench panel missing"
        ((failed++))
    fi

    if echo "$output" | grep -q "Jobs"; then
        log_success "Home Hub: Jobs panel present"
        ((passed++))
    else
        log_fail "Home Hub: Jobs panel missing"
        ((failed++))
    fi

    if echo "$output" | grep -q "Sources"; then
        log_success "Home Hub: Sources panel present"
        ((passed++))
    else
        log_fail "Home Hub: Sources panel missing"
        ((failed++))
    fi

    echo ""
    log_info "Home Hub: $passed passed, $failed failed"
    return $failed
}

# Test discover mode
cmd_test_discover() {
    log_info "Testing Discover Mode..."

    cmd_restart
    sleep 0.5

    # Navigate to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.5

    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    local passed=0
    local failed=0

    # Check for Discover mode elements
    if echo "$output" | grep -q "Discover"; then
        log_success "Discover: Mode entered"
        ((passed++))
    else
        log_fail "Discover: Mode not entered"
        ((failed++))
    fi

    if echo "$output" | grep -q "GLOB PATTERN\|FOLDERS\|Files"; then
        log_success "Discover: Main panel present"
        ((passed++))
    else
        log_fail "Discover: Main panel missing"
        ((failed++))
    fi

    if echo "$output" | grep -q "Tags"; then
        log_success "Discover: Tags dropdown present"
        ((passed++))
    else
        log_fail "Discover: Tags dropdown missing"
        ((failed++))
    fi

    # Test Sources dropdown
    tmux send-keys -t "$SESSION" -l "1"
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -q "Source.*▲"; then
        log_success "Discover: Sources dropdown opens"
        ((passed++))
    else
        log_fail "Discover: Sources dropdown doesn't open"
        ((failed++))
    fi

    # Close dropdown
    tmux send-keys -t "$SESSION" Escape
    sleep 0.2

    # Test Tags dropdown
    tmux send-keys -t "$SESSION" -l "2"
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -q "Tags.*▲"; then
        log_success "Discover: Tags dropdown opens"
        ((passed++))
    else
        log_fail "Discover: Tags dropdown doesn't open"
        ((failed++))
    fi

    tmux send-keys -t "$SESSION" Escape
    sleep 0.2

    echo ""
    log_info "Discover Mode: $passed passed, $failed failed"
    return $failed
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

    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    local passed=0
    local failed=0

    if echo "$output" | grep -q "Create Rule"; then
        log_success "Rule Dialog: Opens correctly"
        ((passed++))
    else
        log_fail "Rule Dialog: Doesn't open"
        ((failed++))
    fi

    if echo "$output" | grep -q "PATTERN"; then
        log_success "Rule Dialog: Pattern field present"
        ((passed++))
    else
        log_fail "Rule Dialog: Pattern field missing"
        ((failed++))
    fi

    if echo "$output" | grep -q "TAG"; then
        log_success "Rule Dialog: Tag field present"
        ((passed++))
    else
        log_fail "Rule Dialog: Tag field missing"
        ((failed++))
    fi

    if echo "$output" | grep -q "OPTIONS"; then
        log_success "Rule Dialog: Options section present"
        ((passed++))
    else
        log_fail "Rule Dialog: Options section missing"
        ((failed++))
    fi

    if echo "$output" | grep -q "LIVE PREVIEW"; then
        log_success "Rule Dialog: Live preview present"
        ((passed++))
    else
        log_fail "Rule Dialog: Live preview missing"
        ((failed++))
    fi

    # Test typing in pattern field
    tmux send-keys -t "$SESSION" -l "*.txt"
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -q '\*.txt'; then
        log_success "Rule Dialog: Can type in pattern field"
        ((passed++))
    else
        log_fail "Rule Dialog: Can't type in pattern field"
        ((failed++))
    fi

    # Test Tab navigation
    tmux send-keys -t "$SESSION" Tab
    sleep 0.2
    tmux send-keys -t "$SESSION" -l "test_tag"
    sleep 0.2
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -q "test_tag"; then
        log_success "Rule Dialog: Tab navigation works"
        ((passed++))
    else
        log_fail "Rule Dialog: Tab navigation broken"
        ((failed++))
    fi

    # Test Escape closes dialog (goes back to Discover or Home)
    tmux send-keys -t "$SESSION" Escape
    sleep 0.5
    output=$(tmux capture-pane -t "$SESSION" -p)

    # After Escape, we should NOT see the dialog title "Create Rule" centered
    # The dialog has a specific bordered structure
    if ! echo "$output" | grep -q "┌ Create Rule"; then
        log_success "Rule Dialog: Escape closes dialog"
        ((passed++))
    else
        log_fail "Rule Dialog: Escape doesn't close dialog"
        ((failed++))
    fi

    echo ""
    log_info "Rule Creation Dialog: $passed passed, $failed failed"
    return $failed
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

    local passed=0
    local failed=0

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

    # Capture and verify all characters appeared
    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -q "abcdefghij"; then
        log_success "Latency: All 10 characters rendered (${elapsed}ms total)"
        ((passed++))
    else
        log_fail "Latency: Characters missing or delayed"
        ((failed++))
    fi

    # Test that typing doesn't freeze UI - check for pattern field
    if echo "$output" | grep -q "PATTERN"; then
        log_success "Latency: UI remained responsive during input"
        ((passed++))
    else
        log_fail "Latency: UI appears frozen"
        ((failed++))
    fi

    # Verify debounce is working - preview should update after delay
    sleep 0.3  # Wait for debounce (150ms) + render
    output=$(tmux capture-pane -t "$SESSION" -p)

    # Check that live preview section exists (even if empty)
    if echo "$output" | grep -q "LIVE PREVIEW"; then
        log_success "Latency: Live preview section present after debounce"
        ((passed++))
    else
        log_fail "Latency: Live preview section missing"
        ((failed++))
    fi

    # Test backspace latency (tmux uses BSpace for backspace)
    log_info "Testing backspace handling..."
    for i in {1..5}; do
        tmux send-keys -t "$SESSION" BSpace
    done
    sleep 0.2

    output=$(tmux capture-pane -t "$SESSION" -p)
    if echo "$output" | grep -q "abcde" && ! echo "$output" | grep -q "abcdefghij"; then
        log_success "Latency: Backspace handled correctly"
        ((passed++))
    else
        log_fail "Latency: Backspace not working as expected"
        ((failed++))
    fi

    echo ""
    log_info "Latency Tests: $passed passed, $failed failed"
    return $failed
}

# Test glob explorer to rule dialog handoff
cmd_test_glob_handoff() {
    log_info "Testing Glob Explorer → Rule Dialog Handoff..."

    cmd_restart
    sleep 0.5

    # Navigate to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.5

    local passed=0
    local failed=0

    # Enter glob pattern mode
    tmux send-keys -t "$SESSION" "/"
    sleep 0.2

    # Type a pattern
    tmux send-keys -t "$SESSION" -l "*.txt"
    sleep 0.3

    # Confirm pattern
    tmux send-keys -t "$SESSION" Enter
    sleep 0.3

    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    # Check glob pattern is set
    if echo "$output" | grep -q "\\*.txt"; then
        log_success "Handoff: Glob pattern set correctly"
        ((passed++))
    else
        log_fail "Handoff: Glob pattern not set"
        ((failed++))
    fi

    # Now open rule dialog - should inherit pattern
    tmux send-keys -t "$SESSION" "n"
    sleep 0.3

    output=$(tmux capture-pane -t "$SESSION" -p)

    # Check that dialog opened with pattern prefilled
    if echo "$output" | grep -q "Create Rule"; then
        log_success "Handoff: Rule dialog opened"
        ((passed++))
    else
        log_fail "Handoff: Rule dialog didn't open"
        ((failed++))
    fi

    # Check pattern is prefilled (should contain *.txt or the pattern)
    if echo "$output" | grep -q "\\*.txt\|txt"; then
        log_success "Handoff: Pattern prefilled from glob explorer"
        ((passed++))
    else
        log_fail "Handoff: Pattern not prefilled"
        ((failed++))
    fi

    # Check file count is shown (should be inherited)
    if echo "$output" | grep -qE "\[[0-9]+ files\]|files match"; then
        log_success "Handoff: File count inherited from glob explorer"
        ((passed++))
    else
        log_warn "Handoff: File count may not be inherited (check manually)"
    fi

    echo ""
    log_info "Glob Handoff Tests: $passed passed, $failed failed"
    return $failed
}

# Test Jobs mode
cmd_test_jobs() {
    log_info "Testing Jobs Mode..."

    cmd_restart
    sleep 0.5

    # Navigate to Jobs (press 4 from home)
    tmux send-keys -t "$SESSION" "4"
    sleep 0.5

    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    local passed=0
    local failed=0

    # Check for Jobs mode elements
    if echo "$output" | grep -q "Jobs\|JOB\|Pipeline"; then
        log_success "Jobs: Mode entered"
        ((passed++))
    else
        log_fail "Jobs: Mode not entered"
        ((failed++))
    fi

    # Check for job list or empty state
    if echo "$output" | grep -qiE "running|pending|completed|failed|no jobs"; then
        log_success "Jobs: Status indicators present"
        ((passed++))
    else
        log_fail "Jobs: Status indicators missing"
        ((failed++))
    fi

    # Test ESC returns to home
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -q "Discover"; then
        log_success "Jobs: ESC returns to Home Hub"
        ((passed++))
    else
        log_fail "Jobs: ESC doesn't return to Home Hub"
        ((failed++))
    fi

    echo ""
    log_info "Jobs Mode: $passed passed, $failed failed"
    return $failed
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

    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    local passed=0
    local failed=0

    if echo "$output" | grep -qiE "sources.*manager|manage.*sources"; then
        log_success "Sources Manager: Opens correctly"
        ((passed++))
    else
        log_fail "Sources Manager: Doesn't open"
        ((failed++))
    fi

    # Check for source list or empty state
    if echo "$output" | grep -qiE "path|name|files|no sources"; then
        log_success "Sources Manager: Content present"
        ((passed++))
    else
        log_fail "Sources Manager: Content missing"
        ((failed++))
    fi

    # Check for action keys
    if echo "$output" | grep -qiE "\[n\]|\[e\]|\[d\]|\[r\]|new|edit|delete|rescan"; then
        log_success "Sources Manager: Action keys shown"
        ((passed++))
    else
        log_fail "Sources Manager: Action keys missing"
        ((failed++))
    fi

    # Test ESC closes dialog
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if ! echo "$output" | grep -qiE "sources.*manager|manage.*sources"; then
        log_success "Sources Manager: ESC closes dialog"
        ((passed++))
    else
        log_fail "Sources Manager: ESC doesn't close dialog"
        ((failed++))
    fi

    echo ""
    log_info "Sources Manager: $passed passed, $failed failed"
    return $failed
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

    local output
    output=$(tmux capture-pane -t "$SESSION" -p)

    local passed=0
    local failed=0

    if echo "$output" | grep -qiE "rules.*manager|tagging.*rules|manage.*rules"; then
        log_success "Rules Manager: Opens correctly"
        ((passed++))
    else
        log_fail "Rules Manager: Doesn't open"
        ((failed++))
    fi

    # Check for rule list or empty state
    if echo "$output" | grep -qiE "pattern|tag|priority|no rules"; then
        log_success "Rules Manager: Content present"
        ((passed++))
    else
        log_fail "Rules Manager: Content missing"
        ((failed++))
    fi

    # Test ESC closes dialog
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if ! echo "$output" | grep -qiE "rules.*manager|tagging.*rules"; then
        log_success "Rules Manager: ESC closes dialog"
        ((passed++))
    else
        log_fail "Rules Manager: ESC doesn't close dialog"
        ((failed++))
    fi

    echo ""
    log_info "Rules Manager: $passed passed, $failed failed"
    return $failed
}

# Test view navigation with number keys
cmd_test_view_navigation() {
    log_info "Testing View Navigation..."

    cmd_restart
    sleep 0.5

    local output
    local passed=0
    local failed=0

    # Test navigation to Discover (1)
    tmux send-keys -t "$SESSION" "1"
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -qi "discover\|source\|files\|tags"; then
        log_success "Navigation: 1 → Discover"
        ((passed++))
    else
        log_fail "Navigation: 1 → Discover failed"
        ((failed++))
    fi

    # Return to Home
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3

    # Test navigation to Parser Bench (2)
    tmux send-keys -t "$SESSION" "2"
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -qi "parser\|bench"; then
        log_success "Navigation: 2 → Parser Bench"
        ((passed++))
    else
        log_fail "Navigation: 2 → Parser Bench failed"
        ((failed++))
    fi

    # Return to Home
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3

    # Test navigation to Inspect (3)
    tmux send-keys -t "$SESSION" "3"
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -qi "inspect\|output\|query"; then
        log_success "Navigation: 3 → Inspect"
        ((passed++))
    else
        log_fail "Navigation: 3 → Inspect failed"
        ((failed++))
    fi

    # Return to Home
    tmux send-keys -t "$SESSION" Escape
    sleep 0.3

    # Test navigation to Jobs (4)
    tmux send-keys -t "$SESSION" "4"
    sleep 0.3
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -qi "jobs\|queue\|pipeline"; then
        log_success "Navigation: 4 → Jobs"
        ((passed++))
    else
        log_fail "Navigation: 4 → Jobs failed"
        ((failed++))
    fi

    echo ""
    log_info "View Navigation: $passed passed, $failed failed"
    return $failed
}

# Test state transitions (ESC behavior from various states)
cmd_test_state_transitions() {
    log_info "Testing State Transitions..."

    cmd_restart
    sleep 0.5

    local output
    local passed=0
    local failed=0

    # Go to Discover
    tmux send-keys -t "$SESSION" "1"
    sleep 0.3

    # Test: Sources dropdown → ESC → Files
    tmux send-keys -t "$SESSION" -l "1"
    sleep 0.2
    tmux send-keys -t "$SESSION" Escape
    sleep 0.2
    output=$(tmux capture-pane -t "$SESSION" -p)

    if ! echo "$output" | grep -q "Source.*▲"; then
        log_success "Transition: Sources dropdown → ESC → Files"
        ((passed++))
    else
        log_fail "Transition: Sources dropdown ESC broken"
        ((failed++))
    fi

    # Test: Tags dropdown → ESC → Files
    tmux send-keys -t "$SESSION" -l "2"
    sleep 0.2
    tmux send-keys -t "$SESSION" Escape
    sleep 0.2
    output=$(tmux capture-pane -t "$SESSION" -p)

    if ! echo "$output" | grep -q "Tags.*▲"; then
        log_success "Transition: Tags dropdown → ESC → Files"
        ((passed++))
    else
        log_fail "Transition: Tags dropdown ESC broken"
        ((failed++))
    fi

    # Test: Rule dialog → ESC → Files
    tmux send-keys -t "$SESSION" "n"
    sleep 0.2
    output=$(tmux capture-pane -t "$SESSION" -p)
    if echo "$output" | grep -qi "create rule"; then
        tmux send-keys -t "$SESSION" Escape
        sleep 0.2
        output=$(tmux capture-pane -t "$SESSION" -p)

        if ! echo "$output" | grep -qi "create rule"; then
            log_success "Transition: Rule dialog → ESC → previous state"
            ((passed++))
        else
            log_fail "Transition: Rule dialog ESC broken"
            ((failed++))
        fi
    else
        log_warn "Transition: Rule dialog didn't open (skipping ESC test)"
    fi

    # Test: Nested dialogs preserve parent state
    # Rules Manager → New Rule → ESC → Rules Manager
    tmux send-keys -t "$SESSION" "R"
    sleep 0.2
    output=$(tmux capture-pane -t "$SESSION" -p)

    if echo "$output" | grep -qiE "rules.*manager|tagging.*rules"; then
        tmux send-keys -t "$SESSION" "n"
        sleep 0.2
        tmux send-keys -t "$SESSION" Escape
        sleep 0.2
        output=$(tmux capture-pane -t "$SESSION" -p)

        if echo "$output" | grep -qiE "rules.*manager|tagging.*rules"; then
            log_success "Transition: Nested dialog → ESC → parent dialog"
            ((passed++))
        else
            log_fail "Transition: Nested dialog ESC returns to wrong state"
            ((failed++))
        fi
    else
        log_warn "Transition: Rules Manager didn't open (skipping nested test)"
    fi

    echo ""
    log_info "State Transitions: $passed passed, $failed failed"
    return $failed
}

# Run all smoke tests
cmd_smoke() {
    log_info "Running TUI Smoke Tests..."
    echo ""

    local total_failed=0

    cmd_test_home || ((total_failed+=$?))
    echo ""

    cmd_test_discover || ((total_failed+=$?))
    echo ""

    cmd_test_rule_dialog || ((total_failed+=$?))
    echo ""

    cmd_test_latency || ((total_failed+=$?))
    echo ""

    cmd_test_glob_handoff || ((total_failed+=$?))
    echo ""

    cmd_stop

    if [[ $total_failed -eq 0 ]]; then
        log_success "All smoke tests passed!"
    else
        log_fail "Smoke tests: $total_failed failures"
    fi

    return $total_failed
}

# Run full test suite (smoke + additional tests)
cmd_full() {
    log_info "Running Full TUI Test Suite..."
    echo ""

    local total_failed=0

    # Smoke tests
    cmd_test_home || ((total_failed+=$?))
    echo ""
    cmd_test_discover || ((total_failed+=$?))
    echo ""
    cmd_test_rule_dialog || ((total_failed+=$?))
    echo ""

    # Additional coverage
    cmd_test_view_navigation || ((total_failed+=$?))
    echo ""
    cmd_test_state_transitions || ((total_failed+=$?))
    echo ""
    cmd_test_jobs || ((total_failed+=$?))
    echo ""
    cmd_test_sources_manager || ((total_failed+=$?))
    echo ""
    cmd_test_rules_manager || ((total_failed+=$?))
    echo ""

    # Performance
    cmd_test_latency || ((total_failed+=$?))
    echo ""
    cmd_test_glob_handoff || ((total_failed+=$?))
    echo ""

    cmd_stop

    if [[ $total_failed -eq 0 ]]; then
        log_success "Full test suite passed!"
    else
        log_fail "Full test suite: $total_failed failures"
    fi

    return $total_failed
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
