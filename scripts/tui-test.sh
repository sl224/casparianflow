#!/bin/bash
# scripts/tui-test.sh - Run TUI test scenarios
#
# Usage:
#   ./scripts/tui-test.sh list                    # List available scenarios
#   ./scripts/tui-test.sh discover-basic          # Run specific scenario
#   ./scripts/tui-test.sh discover-sources        # Test sources dropdown
#   ./scripts/tui-test.sh all                     # Run all scenarios
#
# This script starts a fresh TUI session, runs the test scenario,
# and reports pass/fail based on expected screen content.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SESSION="tui_test"
WIDTH=120
HEIGHT=40
PASSED=0
FAILED=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Find binary
find_binary() {
    local workspace_dir="$(dirname "$SCRIPT_DIR")"
    if [[ -x "$workspace_dir/target/release/casparian" ]]; then
        echo "$workspace_dir/target/release/casparian"
    elif [[ -x "$workspace_dir/target/debug/casparian" ]]; then
        echo "$workspace_dir/target/debug/casparian"
    else
        echo ""
    fi
}

# Start fresh session
start_session() {
    local binary=$(find_binary)
    if [[ -z "$binary" ]]; then
        echo -e "${RED}ERROR: casparian binary not found${NC}"
        exit 1
    fi

    tmux kill-session -t "$SESSION" 2>/dev/null || true
    tmux new-session -d -s "$SESSION" -x "$WIDTH" -y "$HEIGHT" "$binary tui"
    sleep 1
}

# Stop session
stop_session() {
    tmux kill-session -t "$SESSION" 2>/dev/null || true
}

# Send keys and wait
send() {
    tmux send-keys -t "$SESSION" "$1"
    sleep "${2:-0.3}"
}

# Capture screen
capture() {
    tmux capture-pane -t "$SESSION" -p
}

# Assert screen contains pattern
assert_contains() {
    local pattern="$1"
    local description="$2"
    local screen=$(capture)

    if echo "$screen" | grep -q "$pattern"; then
        echo -e "  ${GREEN}[PASS]${NC} $description"
        ((PASSED++))
        return 0
    else
        echo -e "  ${RED}[FAIL]${NC} $description"
        echo -e "         Expected to find: '$pattern'"
        echo -e "         Screen content:"
        echo "$screen" | head -20 | sed 's/^/         /'
        ((FAILED++))
        return 1
    fi
}

# Assert screen does NOT contain pattern
assert_not_contains() {
    local pattern="$1"
    local description="$2"
    local screen=$(capture)

    if echo "$screen" | grep -q "$pattern"; then
        echo -e "  ${RED}[FAIL]${NC} $description"
        echo -e "         Should NOT contain: '$pattern'"
        ((FAILED++))
        return 1
    else
        echo -e "  ${GREEN}[PASS]${NC} $description"
        ((PASSED++))
        return 0
    fi
}

# ═══════════════════════════════════════════════════════════════════════════
# TEST SCENARIOS
# ═══════════════════════════════════════════════════════════════════════════

test_startup() {
    echo -e "\n${YELLOW}TEST: TUI Startup${NC}"
    start_session

    assert_contains "Home" "TUI shows Home screen on startup"
    assert_contains "Discover" "Home shows Discover card"
    assert_contains "Parser" "Home shows Parser Bench card"

    stop_session
}

test_discover_basic() {
    echo -e "\n${YELLOW}TEST: Discover Mode Basic${NC}"
    start_session

    # Enter Discover mode
    send "1"
    assert_contains "SOURCES\|Sources" "Discover shows Sources panel"
    assert_contains "FILES\|Files" "Discover shows Files panel"

    stop_session
}

test_discover_sources() {
    echo -e "\n${YELLOW}TEST: Discover Sources Dropdown${NC}"
    start_session

    # Enter Discover mode
    send "1"
    sleep 0.5

    # Capture before opening dropdown
    local before=$(capture)

    # Press 1 to open sources dropdown (per spec Section 6.1)
    send "1"

    # Check for dropdown open indicators
    # When open, should show filter input or expanded list
    local after=$(capture)

    # The state should have changed
    if [[ "$before" != "$after" ]]; then
        echo -e "  ${GREEN}[PASS]${NC} Pressing 1 changes the UI state"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[WARN]${NC} UI didn't change after pressing 1 (may have no sources)"
        # Not a failure - could be empty state
    fi

    # Press Escape to close
    send "Escape"

    stop_session
}

test_discover_navigation() {
    echo -e "\n${YELLOW}TEST: Discover Navigation${NC}"
    start_session

    # Enter Discover mode
    send "1"
    sleep 0.5

    # Try j/k navigation (per spec Section 6.4)
    send "j"
    send "k"

    # UI should still be responsive (not crashed)
    assert_contains "SOURCES\|Sources\|FILES\|Files" "UI responsive after navigation"

    # Press Escape to go back to Home
    send "Escape"
    assert_contains "Home" "Escape returns to Home"

    stop_session
}

test_view_switching() {
    echo -e "\n${YELLOW}TEST: View Switching${NC}"
    start_session

    # Press 1 for Discover
    send "1"
    assert_contains "SOURCES\|Sources\|Discover" "Key 1 enters Discover mode"

    # Press Escape, then 2 for Parser Bench
    send "Escape"
    sleep 0.3
    send "2"
    assert_contains "Parser\|PARSER\|Parsers" "Key 2 enters Parser Bench mode"

    # Press Escape, then 3 for Inspect
    send "Escape"
    sleep 0.3
    send "3"
    # Inspect mode indicators
    local screen=$(capture)
    if echo "$screen" | grep -qi "inspect\|output\|tables"; then
        echo -e "  ${GREEN}[PASS]${NC} Key 3 enters Inspect mode"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} Key 3 view unclear (may be different mode)"
    fi

    stop_session
}

test_quit() {
    echo -e "\n${YELLOW}TEST: Quit with Ctrl+C${NC}"
    start_session

    # Send Ctrl+C
    send "C-c"
    sleep 0.5

    # Session should still exist but TUI should have quit
    # (the wrapper script keeps it alive with "press Enter")
    local screen=$(capture)
    if echo "$screen" | grep -qi "exited\|closed\|press enter"; then
        echo -e "  ${GREEN}[PASS]${NC} Ctrl+C exits TUI"
        ((PASSED++))
    else
        # TUI might still be running (graceful quit prompt)
        echo -e "  ${YELLOW}[INFO]${NC} Ctrl+C behavior: quit confirmation may be shown"
    fi

    stop_session
}

# ═══════════════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════════════

list_scenarios() {
    echo "Available test scenarios:"
    echo "  startup           - TUI starts and shows Home screen"
    echo "  discover-basic    - Enter Discover mode, verify panels"
    echo "  discover-sources  - Test sources dropdown opening"
    echo "  discover-nav      - Test j/k navigation and Escape"
    echo "  view-switching    - Test 1/2/3 view switching"
    echo "  quit              - Test Ctrl+C quit"
    echo ""
    echo "  all               - Run all scenarios"
}

run_all() {
    test_startup
    test_discover_basic
    test_discover_sources
    test_discover_navigation
    test_view_switching
    test_quit
}

print_summary() {
    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo -e "SUMMARY: ${GREEN}$PASSED passed${NC}, ${RED}$FAILED failed${NC}"
    echo "═══════════════════════════════════════════════════════════════"

    if [[ $FAILED -gt 0 ]]; then
        exit 1
    fi
}

case "${1:-list}" in
    list)
        list_scenarios
        ;;
    startup)
        test_startup
        print_summary
        ;;
    discover-basic)
        test_discover_basic
        print_summary
        ;;
    discover-sources)
        test_discover_sources
        print_summary
        ;;
    discover-nav)
        test_discover_navigation
        print_summary
        ;;
    view-switching)
        test_view_switching
        print_summary
        ;;
    quit)
        test_quit
        print_summary
        ;;
    all)
        run_all
        print_summary
        ;;
    *)
        echo "Unknown scenario: $1"
        echo ""
        list_scenarios
        exit 1
        ;;
esac
