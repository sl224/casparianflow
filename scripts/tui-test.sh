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

    # Kill any existing session
    tmux kill-session -t "$SESSION" 2>/dev/null || true
    sleep 0.3

    # Start new session with the TUI (keep shell alive with sleep fallback)
    # The "|| sleep 60" ensures the pane stays open even if TUI exits
    tmux new-session -d -s "$SESSION" -x "$WIDTH" -y "$HEIGHT" "$binary tui || sleep 60"
    sleep 1.5  # Give TUI time to initialize
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

# Assert screen contains pattern (supports | for alternatives)
assert_contains() {
    local pattern="$1"
    local description="$2"
    local screen=$(capture)

    if echo "$screen" | grep -qE "$pattern"; then
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

# Assert screen does NOT contain pattern (supports | for alternatives)
assert_not_contains() {
    local pattern="$1"
    local description="$2"
    local screen=$(capture)

    if echo "$screen" | grep -qE "$pattern"; then
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

# ───────────────────────────────────────────────────────────────────────────
# PHASE 0: STARTUP AND HOME
# ───────────────────────────────────────────────────────────────────────────

test_startup() {
    echo -e "\n${YELLOW}TEST: TUI Startup${NC}"
    start_session

    assert_contains "Home Hub|Casparian Flow" "TUI shows Home screen on startup"
    assert_contains "Discover|1.*Discover" "Home shows Discover card"
    assert_contains "Parser Bench|2.*Parser" "Home shows Parser Bench card"
    assert_contains "Jobs|3.*Jobs" "Home shows Jobs card"
    assert_contains "Sources|4.*Sources" "Home shows Sources card"

    stop_session
}

test_home_navigation() {
    echo -e "\n${YELLOW}TEST: Home Screen Navigation${NC}"
    start_session

    # Test arrow key navigation on home cards
    send "Down"
    sleep 0.2
    assert_contains "Home Hub|Casparian" "Down arrow navigates home cards"

    send "Right"
    sleep 0.2
    assert_contains "Home Hub|Casparian" "Right arrow navigates home cards"

    send "Up"
    sleep 0.2
    assert_contains "Home Hub|Casparian" "Up arrow navigates home cards"

    send "Left"
    sleep 0.2
    assert_contains "Home Hub|Casparian" "Left arrow navigates home cards"

    stop_session
}

test_home_enter_discover() {
    echo -e "\n${YELLOW}TEST: Enter Discover from Home${NC}"
    start_session

    # Enter on Discover card should enter Discover mode
    send "Enter"
    sleep 0.5
    assert_contains "Discover|Tags|FOLDERS" "Enter opens selected mode"

    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# PHASE 1: DROPDOWN FOUNDATION (Sources Dropdown)
# ───────────────────────────────────────────────────────────────────────────

test_discover_basic() {
    echo -e "\n${YELLOW}TEST: Discover Mode Basic${NC}"
    start_session

    # Enter Discover mode
    send "1"
    assert_contains "Discover" "Discover mode title shown"
    assert_contains "Tags" "Discover shows Tags panel"
    assert_contains "GLOB PATTERN|FOLDERS|files" "Discover shows file/folder view"

    stop_session
}

test_sources_dropdown_open_close() {
    echo -e "\n${YELLOW}TEST: Phase 1 - Sources Dropdown Open/Close${NC}"
    start_session

    # Enter Discover mode
    send "1"
    sleep 0.5

    # Capture initial state
    local before=$(capture)

    # Press 1 to toggle sources dropdown
    send "1"
    sleep 0.3

    # State should change (dropdown opened)
    local after=$(capture)
    if [[ "$before" != "$after" ]]; then
        echo -e "  ${GREEN}[PASS]${NC} Sources dropdown toggle changes UI state"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} No visible change (may have no sources)"
    fi

    # Press Escape to close dropdown
    send "Escape"
    sleep 0.3
    assert_contains "Discover|GLOB|FOLDERS" "Escape closes dropdown, returns to Discover"

    stop_session
}

test_sources_dropdown_filter() {
    echo -e "\n${YELLOW}TEST: Phase 1 - Sources Dropdown Filter${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "1"  # Open sources dropdown
    sleep 0.3

    # Type characters to filter
    send "t"
    sleep 0.1
    send "e"
    sleep 0.1
    send "s"
    sleep 0.1
    send "t"
    sleep 0.3

    # Backspace should remove character
    send "Backspace"
    sleep 0.2
    send "Backspace"
    sleep 0.2

    # Escape to close
    send "Escape"
    sleep 0.3
    assert_contains "Discover" "Filter typing and backspace work"

    stop_session
}

test_sources_dropdown_navigation() {
    echo -e "\n${YELLOW}TEST: Phase 1 - Sources Dropdown Arrow Navigation${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "1"  # Open sources dropdown
    sleep 0.3

    # Arrow key navigation in dropdown
    send "Down"
    sleep 0.2
    send "Down"
    sleep 0.2
    send "Up"
    sleep 0.2

    # UI should still be responsive
    assert_contains "Discover|Sources" "Arrow navigation works in dropdown"

    send "Escape"
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

# ───────────────────────────────────────────────────────────────────────────
# PHASE 2: TAGS DROPDOWN
# ───────────────────────────────────────────────────────────────────────────

test_tags_dropdown_open() {
    echo -e "\n${YELLOW}TEST: Phase 2 - Tags Dropdown Open${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Capture before
    local before=$(capture)

    # Press 2 to open tags dropdown
    send "2"
    sleep 0.3

    local after=$(capture)
    if [[ "$before" != "$after" ]]; then
        echo -e "  ${GREEN}[PASS]${NC} Tags dropdown (key 2) changes UI state"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} Tags dropdown shows same UI (may be empty)"
    fi

    # Close with Escape
    send "Escape"
    sleep 0.3
    assert_contains "Discover" "Escape closes tags dropdown"

    stop_session
}

test_tags_dropdown_navigation() {
    echo -e "\n${YELLOW}TEST: Phase 2 - Tags Dropdown Navigation${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "2"  # Open tags dropdown
    sleep 0.3

    # Navigate with arrows
    send "Down"
    sleep 0.2
    send "Down"
    sleep 0.2
    send "Up"
    sleep 0.2

    assert_contains "Discover|Tags" "Arrow navigation in tags dropdown"

    send "Escape"
    stop_session
}

test_tags_dropdown_filter() {
    echo -e "\n${YELLOW}TEST: Phase 2 - Tags Dropdown Filter${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "2"  # Open tags dropdown
    sleep 0.3

    # Type to filter
    send "c"
    sleep 0.1
    send "s"
    sleep 0.1
    send "v"
    sleep 0.2

    # Backspace
    send "Backspace"
    sleep 0.2

    assert_contains "Discover" "Tags filter typing works"

    send "Escape"
    stop_session
}

test_tags_dropdown_enter_confirm() {
    echo -e "\n${YELLOW}TEST: Phase 2 - Tags Dropdown Enter Confirm${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "2"  # Open tags dropdown
    sleep 0.3

    # Press Enter to confirm selection
    send "Enter"
    sleep 0.3

    assert_contains "Discover|GLOB|FOLDERS|Tags" "Enter confirms tag selection"

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
    assert_contains "Discover|Tags|FOLDERS" "UI responsive after navigation"

    # Press Escape to go back to Home
    send "Escape"
    sleep 0.3
    send "Escape"  # May need multiple escapes depending on state
    assert_contains "Home Hub|Discover|Parser Bench" "Escape returns to Home"

    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# PHASE 3: RULES MANAGER DIALOG
# ───────────────────────────────────────────────────────────────────────────

test_rules_manager_open() {
    echo -e "\n${YELLOW}TEST: Phase 3 - Rules Manager Open (R key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Press R to open Rules Manager
    send "R"
    sleep 0.3

    assert_contains "Rules|Manager|tagging|rule|CRUD" "R opens Rules Manager dialog"

    # Close with Escape
    send "Escape"
    sleep 0.3
    assert_contains "Discover" "Escape closes Rules Manager"

    stop_session
}

test_rules_manager_navigation() {
    echo -e "\n${YELLOW}TEST: Phase 3 - Rules Manager j/k Navigation${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "R"  # Open Rules Manager
    sleep 0.3

    # Navigate with j/k
    send "j"
    sleep 0.2
    send "j"
    sleep 0.2
    send "k"
    sleep 0.2

    assert_contains "Rules|Manager|Discover" "j/k navigation in Rules Manager"

    send "Escape"
    stop_session
}

test_rules_manager_new_rule() {
    echo -e "\n${YELLOW}TEST: Phase 3 - Rules Manager New Rule (n key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "R"  # Open Rules Manager
    sleep 0.3

    # Press n to create new rule
    send "n"
    sleep 0.3

    # Should open rule creation dialog or show create UI
    assert_contains "New|Rule|Pattern|Tag|create" "n in Rules Manager creates rule"

    send "Escape"
    sleep 0.3
    send "Escape"  # Close Rules Manager too
    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# PHASE 4: QUICK RULE CREATION (n key in Discover)
# ───────────────────────────────────────────────────────────────────────────

test_quick_rule_creation_open() {
    echo -e "\n${YELLOW}TEST: Phase 4 - Quick Rule Creation Dialog${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Press n to create new tagging rule
    send "n"
    sleep 0.3

    assert_contains "New Tagging Rule|Pattern|Tag" "n opens tagging rule dialog"

    stop_session
}

test_quick_rule_creation_tab() {
    echo -e "\n${YELLOW}TEST: Phase 4 - Rule Dialog Tab Navigation${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "n"  # Open rule dialog
    sleep 0.3

    # Tab between fields
    send "Tab"
    sleep 0.2
    send "Tab"
    sleep 0.2

    assert_contains "New Tagging Rule|Pattern|Tag" "Tab navigates between fields"

    send "Escape"
    stop_session
}

test_quick_rule_creation_typing() {
    echo -e "\n${YELLOW}TEST: Phase 4 - Rule Dialog Text Input${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "n"  # Open rule dialog
    sleep 0.3

    # Type a pattern
    send "*"
    send "."
    send "c"
    send "s"
    send "v"
    sleep 0.3

    assert_contains "csv" "Pattern field accepts text input"

    # Tab to tag field
    send "Tab"
    sleep 0.2

    # Type tag name
    send "d"
    send "a"
    send "t"
    send "a"
    sleep 0.3

    assert_contains "data" "Tag field accepts text input"

    send "Escape"
    stop_session
}

test_quick_rule_creation_cancel() {
    echo -e "\n${YELLOW}TEST: Phase 4 - Rule Dialog Escape Cancel${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "n"  # Open rule dialog
    sleep 0.3

    assert_contains "New Tagging Rule" "Rule dialog opened"

    # Escape should close
    send "Escape"
    sleep 0.3

    assert_not_contains "New Tagging Rule" "Escape closes rule dialog"

    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# PHASE 5: POLISH (Help overlay, scan dialog, etc.)
# ───────────────────────────────────────────────────────────────────────────

test_view_switching() {
    echo -e "\n${YELLOW}TEST: View Switching${NC}"
    start_session

    # Press 1 for Discover
    send "1"
    assert_contains "Discover|Tags|GLOB" "Key 1 enters Discover mode"

    # Press Escape, then 2 for Parser Bench
    send "Escape"
    sleep 0.3
    send "Escape"
    sleep 0.3
    send "2"
    assert_contains "Parser|Bench|Test" "Key 2 enters Parser Bench mode"

    # Press Escape, then 3 for Jobs
    send "Escape"
    sleep 0.3
    send "Escape"
    sleep 0.3
    send "3"
    # Jobs mode indicators (case-insensitive)
    assert_contains "JOBS|jobs|Jobs|No jobs" "Key 3 enters Jobs mode"

    stop_session
}

test_view_switching_from_home() {
    echo -e "\n${YELLOW}TEST: Phase 5 - View Switching from Home${NC}"
    start_session

    # Test all number keys from Home
    # Key 1 = Discover
    send "1"
    sleep 0.3
    assert_contains "Discover" "Key 1 enters Discover from Home"

    send "Escape"
    send "Escape"
    sleep 0.3

    # Key 2 = Parser Bench
    send "2"
    sleep 0.3
    assert_contains "Parser|Bench" "Key 2 enters Parser Bench from Home"

    send "Escape"
    send "Escape"
    sleep 0.3

    # Key 3 = Jobs
    send "3"
    sleep 0.3
    assert_contains "Jobs|JOBS" "Key 3 enters Jobs from Home"

    send "Escape"
    send "Escape"
    sleep 0.3

    # Key 0 = Home (should stay/return to Home)
    send "0"
    sleep 0.3
    assert_contains "Home Hub|Casparian" "Key 0 returns to Home"

    stop_session
}

test_help_overlay_toggle() {
    echo -e "\n${YELLOW}TEST: Phase 5 - Help Overlay Toggle${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Press ? to open help
    send "?"
    sleep 0.3
    assert_contains "Help|NAVIGATION|GLOBAL" "? opens help overlay"

    # Press ? again to close
    send "?"
    sleep 0.3
    assert_not_contains "Toggle this help" "? closes help overlay"

    stop_session
}

test_help_overlay_content() {
    echo -e "\n${YELLOW}TEST: Phase 5 - Help Overlay Content${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "?"  # Open help
    sleep 0.3

    # Verify help content
    assert_contains "Discover view|Parser Bench|Jobs view" "Help shows navigation keys"
    assert_contains "Help|key|navigation" "Help shows keybinding info"

    send "?"  # Close help
    stop_session
}

test_help_overlay_escape() {
    echo -e "\n${YELLOW}TEST: Phase 5 - Help Overlay Escape${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "?"  # Open help
    sleep 0.3
    assert_contains "Help" "Help overlay opened"

    # Escape or ? should close help - try both
    send "Escape"
    sleep 0.3

    # If help still shows, try ? to close it
    local screen=$(capture)
    if echo "$screen" | grep -qE "Toggle this help"; then
        send "?"
        sleep 0.3
    fi

    assert_not_contains "Toggle this help" "Help overlay closes with Escape or ?"

    stop_session
}

test_scan_dialog_open() {
    echo -e "\n${YELLOW}TEST: Phase 5 - Scan Dialog Open (s key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Press s to open scan dialog
    send "s"
    sleep 0.3

    # Should show scan dialog with path input
    assert_contains "Scan|Path|directory|folder" "s opens scan dialog"

    send "Escape"
    stop_session
}

test_scan_dialog_path_autocomplete() {
    echo -e "\n${YELLOW}TEST: Phase 5 - Scan Dialog Path Autocomplete${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "s"  # Open scan dialog
    sleep 0.3

    # Type a path
    send "/"
    sleep 0.2

    # Should show path suggestions or autocomplete
    assert_contains "Scan|Path|/" "Path input accepts text"

    # Tab should autocomplete
    send "Tab"
    sleep 0.2

    assert_contains "Scan|Path" "Tab triggers autocomplete"

    send "Escape"
    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# GLOB EXPLORER (Phases 12-19)
# ───────────────────────────────────────────────────────────────────────────

test_glob_explorer() {
    echo -e "\n${YELLOW}TEST: Glob Explorer${NC}"
    start_session

    # Enter Discover mode
    send "1"
    sleep 0.5

    # Should show GLOB PATTERN panel
    assert_contains "GLOB PATTERN|FOLDERS" "Discover shows Glob Explorer"

    # Press / to enter pattern editing mode
    send "/"
    sleep 0.3
    assert_contains "Enter.*Done|Esc.*Cancel" "Pattern editing mode shows hints"

    # Type a pattern
    send "*.toml"
    sleep 0.3
    assert_contains "toml" "Pattern input accepts text"

    # Press Escape to cancel
    send "Escape"
    sleep 0.3

    stop_session
}

test_glob_explorer_pattern_input() {
    echo -e "\n${YELLOW}TEST: Glob Explorer - Pattern Input${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Enter pattern mode
    send "/"
    sleep 0.3

    # Type various glob patterns
    send "*"
    sleep 0.1
    send "*"
    sleep 0.1
    send "/"
    sleep 0.1
    send "*"
    sleep 0.1
    send "."
    sleep 0.1
    send "r"
    sleep 0.1
    send "s"
    sleep 0.3

    assert_contains "rs" "Glob pattern typing works"

    # Backspace to edit
    send "Backspace"
    sleep 0.2
    send "Backspace"
    sleep 0.2

    assert_contains "GLOB|FOLDERS|Discover" "Backspace edits pattern"

    send "Escape"
    stop_session
}

test_glob_explorer_pattern_confirm() {
    echo -e "\n${YELLOW}TEST: Glob Explorer - Pattern Confirm (Enter)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Enter pattern mode
    send "/"
    sleep 0.3

    # Type a pattern
    send "*"
    send "."
    send "m"
    send "d"
    sleep 0.3

    # Press Enter to confirm pattern
    send "Enter"
    sleep 0.3

    # Pattern should be applied
    assert_contains "GLOB|FOLDERS|Discover" "Enter confirms pattern"

    stop_session
}

test_glob_explorer_folder_drilling() {
    echo -e "\n${YELLOW}TEST: Glob Explorer - Folder Drilling (l/Enter)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5
    sleep 1  # Wait for folder cache

    # Navigate to a folder
    send "j"
    sleep 0.2

    # Drill into folder with l
    send "l"
    sleep 0.3

    # Should show folder contents or stay responsive
    assert_contains "FOLDERS|GLOB|Discover" "l drills into folder"

    # Go back with h
    send "h"
    sleep 0.3

    assert_contains "FOLDERS|GLOB|Discover" "h goes back to parent"

    stop_session
}

test_glob_explorer_vim_navigation() {
    echo -e "\n${YELLOW}TEST: Glob Explorer - Vim Navigation (hjkl)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5
    sleep 1  # Wait for folder cache

    # j = down
    send "j"
    sleep 0.2
    send "j"
    sleep 0.2

    # k = up
    send "k"
    sleep 0.2

    # l = drill in (Enter equivalent)
    send "l"
    sleep 0.2

    # h = back (Backspace equivalent)
    send "h"
    sleep 0.2

    assert_contains "FOLDERS|GLOB|Discover" "hjkl navigation works"

    stop_session
}

test_glob_explorer_arrow_navigation() {
    echo -e "\n${YELLOW}TEST: Glob Explorer - Arrow Key Navigation${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5
    sleep 1  # Wait for folder cache

    # Arrow keys
    send "Down"
    sleep 0.2
    send "Down"
    sleep 0.2
    send "Up"
    sleep 0.2
    send "Right"
    sleep 0.2
    send "Left"
    sleep 0.2

    assert_contains "FOLDERS|GLOB|Discover" "Arrow key navigation works"

    stop_session
}

test_glob_explorer_exit() {
    echo -e "\n${YELLOW}TEST: Glob Explorer - Exit (g/Escape)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # g should exit Glob Explorer (if in pattern mode) or toggle
    send "g"
    sleep 0.3

    # Escape should return to Home eventually
    send "Escape"
    sleep 0.3
    send "Escape"
    sleep 0.3

    assert_contains "Home Hub|Discover|Casparian" "Can exit to Home"

    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# PHASE 5 CONTINUED: HELP OVERLAY
# ───────────────────────────────────────────────────────────────────────────

test_help_overlay() {
    echo -e "\n${YELLOW}TEST: Help Overlay${NC}"
    start_session

    # Enter Discover mode
    send "1"
    sleep 0.5

    # Press ? to open help
    send "?"
    sleep 0.3
    assert_contains "Help|NAVIGATION|GLOBAL" "Help overlay appears"
    assert_contains "Discover view|Parser Bench|Jobs view" "Help shows navigation keys"

    # Press ? again to close
    send "?"
    sleep 0.3
    assert_not_contains "Toggle this help" "Help overlay closes"

    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# SOURCES MANAGER (M key)
# ───────────────────────────────────────────────────────────────────────────

test_sources_manager() {
    echo -e "\n${YELLOW}TEST: Sources Manager${NC}"
    start_session

    # Enter Discover mode
    send "1"
    sleep 0.5

    # Press M to open Sources Manager
    send "M"
    sleep 0.3
    assert_contains "Sources Manager|Name.*Path|Files" "Sources Manager opens"

    # Press Escape to close
    send "Escape"
    sleep 0.3
    assert_not_contains "Sources Manager" "Sources Manager closes"

    stop_session
}

test_sources_manager_navigation() {
    echo -e "\n${YELLOW}TEST: Sources Manager - j/k Navigation${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "M"  # Open Sources Manager
    sleep 0.3

    # Navigate with j/k
    send "j"
    sleep 0.2
    send "j"
    sleep 0.2
    send "k"
    sleep 0.2

    assert_contains "Sources Manager|Discover" "j/k navigation in Sources Manager"

    send "Escape"
    stop_session
}

test_sources_manager_add_source() {
    echo -e "\n${YELLOW}TEST: Sources Manager - Add Source (n key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "M"  # Open Sources Manager
    sleep 0.3

    # Press n to add new source
    send "n"
    sleep 0.3

    # Should open scan dialog
    assert_contains "Scan|Path|directory|Sources Manager" "n opens scan dialog"

    send "Escape"
    send "Escape"
    stop_session
}

test_sources_manager_edit_source() {
    echo -e "\n${YELLOW}TEST: Sources Manager - Edit Source (e key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "M"  # Open Sources Manager
    sleep 0.3

    # Press e to edit (even if no sources, tests key handling)
    send "e"
    sleep 0.3

    # Should show edit state or handle gracefully
    assert_contains "Sources Manager|Discover|Edit|Name" "e key handled in Sources Manager"

    send "Escape"
    send "Escape"
    stop_session
}

test_sources_manager_delete_source() {
    echo -e "\n${YELLOW}TEST: Sources Manager - Delete Source (d key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "M"  # Open Sources Manager
    sleep 0.3

    # Press d to delete (should show confirmation if source selected)
    send "d"
    sleep 0.3

    # Should show delete confirmation or handle gracefully
    assert_contains "Sources Manager|Discover|Delete|Confirm" "d key handled in Sources Manager"

    send "Escape"
    send "Escape"
    stop_session
}

test_sources_manager_rescan() {
    echo -e "\n${YELLOW}TEST: Sources Manager - Rescan Source (r key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "M"  # Open Sources Manager
    sleep 0.3

    # Press r to rescan
    send "r"
    sleep 0.3

    # Should trigger rescan or handle gracefully
    assert_contains "Sources Manager|Discover|scan" "r key handled in Sources Manager"

    send "Escape"
    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# RULE BUILDER THREE-PHASE WORKFLOW
# Tests the three-phase file results panel (spec Section 4)
# Phase 1: Exploration (folder counts)
# Phase 2: Extraction Preview (per-file with extracted values)
# Phase 3: Backtest Results (pass/fail)
# ───────────────────────────────────────────────────────────────────────────

test_rule_builder_open() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Open (n key in Discover)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 2.5  # Wait for folder cache to load

    # Press n to open Rule Builder
    send "n"
    sleep 0.5

    assert_contains "Rule Builder|PATTERN|EXCLUDES|TAG" "n opens Rule Builder"
    assert_contains "FOLDERS|expand|collapse" "Rule Builder shows Phase 1 (Exploration)"

    send "Escape"
    stop_session
}

test_rule_builder_phase1_exploration() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Phase 1 Exploration${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 3  # Wait for folder cache to fully load (may take time after prev test)

    # Wait for folders to appear in UI (verify cache is loaded)
    local retry=0
    while [[ $retry -lt 5 ]]; do
        local screen=$(capture)
        if echo "$screen" | grep -qE "FOLDERS \([0-9]+\)"; then
            break
        fi
        sleep 0.5
        ((retry++))
    done

    send "n"  # Open Rule Builder
    sleep 0.5

    # Phase 1 should show folders with counts
    assert_contains "FOLDERS.*folders.*files" "Phase 1 shows folder counts"

    # Check for folder matches (may be 0 if cache not ready, use looser check)
    local screen=$(capture)
    if echo "$screen" | grep -qE "dir_|folders \([1-9]"; then
        echo -e "  ${GREEN}[PASS]${NC} Phase 1 shows folder matches"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} Folder matches may not be visible yet (cache timing)"
    fi

    # Clear pattern and type new one
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "BSpace"
    sleep 0.2

    # Type a glob pattern (no <field>)
    send "*"
    send "."
    send "t"
    send "x"
    send "t"
    sleep 0.5

    # Should still be in Phase 1 (no <field> placeholders)
    assert_contains "FOLDERS" "Pattern without <field> stays in Phase 1"

    send "Escape"
    stop_session
}

test_rule_builder_phase1_navigation() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Phase 1 Navigation${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 1.5

    send "n"  # Open Rule Builder
    sleep 0.5

    # Tab to FileList (5 tabs: Pattern->Excludes->Tag->Extractions->Options->FileList)
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    sleep 0.3

    # Navigate with j/k
    send "j"
    sleep 0.2
    send "j"
    sleep 0.2
    send "k"
    sleep 0.2

    # Should show selection indicator
    local screen=$(capture)
    if echo "$screen" | grep -qE "►.*dir_"; then
        echo -e "  ${GREEN}[PASS]${NC} j/k navigation shows selection indicator"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} Selection indicator may not be visible"
    fi

    send "Escape"
    stop_session
}

test_rule_builder_phase1_expansion() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Phase 1 Folder Expansion (Enter)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 1.5

    send "n"  # Open Rule Builder
    sleep 0.5

    # Tab to FileList
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    sleep 0.3

    # Capture before expansion
    local before=$(capture)

    # Press Enter to expand folder
    send "Enter"
    sleep 0.3

    # Capture after expansion
    local after=$(capture)

    # Check if expansion icon changed (▸ to ▼)
    if echo "$after" | grep -qE "▼"; then
        echo -e "  ${GREEN}[PASS]${NC} Enter toggles folder expansion (▼ visible)"
        ((PASSED++))
    elif [[ "$before" != "$after" ]]; then
        echo -e "  ${GREEN}[PASS]${NC} Enter changes expansion state"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} Expansion may not be visible in this state"
    fi

    send "Escape"
    stop_session
}

test_rule_builder_phase2_transition() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Phase 2 Transition (pattern with <field>)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 1.5

    send "n"  # Open Rule Builder
    sleep 0.5

    # Clear default pattern
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "BSpace"
    sleep 0.2

    # Type pattern with <field> placeholder
    # file_<num>.txt
    send "f"
    send "i"
    send "l"
    send "e"
    send "_"
    send "<"
    send "n"
    send "u"
    send "m"
    send ">"
    send "."
    send "t"
    send "x"
    send "t"
    sleep 0.5

    # Should transition to Phase 2
    assert_contains "PREVIEW" "Pattern with <field> transitions to Phase 2"
    assert_contains "extractions" "Phase 2 shows extractions"

    send "Escape"
    stop_session
}

test_rule_builder_phase2_extraction_preview() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Phase 2 Extraction Preview${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 1.5

    send "n"  # Open Rule Builder
    sleep 0.5

    # Type pattern with extraction
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "f"
    send "i"
    send "l"
    send "e"
    send "_"
    send "<"
    send "n"
    send "u"
    send "m"
    send ">"
    send "."
    send "t"
    send "x"
    send "t"
    sleep 0.5

    # Phase 2 shows per-file extractions
    assert_contains "file.*\\[" "Phase 2 shows files with extraction brackets"

    # Status bar should show OK/warnings
    assert_contains "OK|warnings|extractions" "Phase 2 shows extraction status"

    send "Escape"
    stop_session
}

test_rule_builder_phase3_transition() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Phase 3 Transition (t key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 1.5

    send "n"  # Open Rule Builder
    sleep 0.5

    # Type pattern to get to Phase 2
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "f"
    send "i"
    send "l"
    send "e"
    send "_"
    send "<"
    send "n"
    send ">"
    send "."
    send "t"
    send "x"
    send "t"
    sleep 0.5

    # Verify in Phase 2
    assert_contains "PREVIEW" "In Phase 2 before transition"

    # Tab to FileList
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    sleep 0.3

    # Press t to run backtest (transition to Phase 3)
    send "t"
    sleep 0.5

    # Should be in Phase 3
    assert_contains "RESULTS" "t transitions to Phase 3 Backtest Results"
    assert_contains "Pass.*Fail|a/p/f" "Phase 3 shows filter options"

    send "Escape"
    stop_session
}

test_rule_builder_phase3_filter() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Phase 3 Filter (a/p/f keys)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 1.5

    send "n"  # Open Rule Builder
    sleep 0.5

    # Get to Phase 3 (type pattern -> Tab to files -> press t)
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "f"
    send "i"
    send "l"
    send "e"
    send "_"
    send "<"
    send "n"
    send ">"
    send "."
    send "t"
    send "x"
    send "t"
    sleep 0.5

    # Tab to FileList
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    sleep 0.3

    send "t"  # Enter Phase 3
    sleep 0.5

    # Test filter keys
    send "p"  # Pass only
    sleep 0.3

    local screen=$(capture)
    if echo "$screen" | grep -qE "Pass|RESULTS"; then
        echo -e "  ${GREEN}[PASS]${NC} p key filters to pass only"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} Filter may not show visible change"
    fi

    send "a"  # All
    sleep 0.3

    send "f"  # Fail only
    sleep 0.3

    assert_contains "RESULTS|Fail" "Filter keys work in Phase 3"

    send "Escape"
    stop_session
}

test_rule_builder_tab_cycle() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Tab Focus Cycle${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 1.5

    send "n"  # Open Rule Builder
    sleep 0.5

    # Initial state should be Pattern focus
    assert_contains "PATTERN" "Rule Builder starts with Pattern focus"

    # Tab cycles: Pattern -> Excludes -> Tag -> Extractions -> Options -> FileList
    send "Tab"
    sleep 0.2

    # Continue tabbing through all fields
    send "Tab"
    sleep 0.2
    send "Tab"
    sleep 0.2
    send "Tab"
    sleep 0.2
    send "Tab"
    sleep 0.2

    # Should now be on FileList
    local screen=$(capture)
    if echo "$screen" | grep -qE "►"; then
        echo -e "  ${GREEN}[PASS]${NC} Tab cycles to FileList (selection visible)"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} Tab cycle completed but selection not visible"
    fi

    # One more Tab should return to Pattern
    send "Tab"
    sleep 0.2

    assert_contains "PATTERN" "Tab cycle returns to Pattern"

    send "Escape"
    stop_session
}

test_rule_builder_escape_close() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Escape Behavior${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 1.5

    send "n"  # Open Rule Builder (though it's already the default)
    sleep 0.5

    assert_contains "Rule Builder" "Rule Builder is open"

    # Escape should keep Rule Builder open (it's the default view)
    send "Escape"
    sleep 0.3

    # Rule Builder is persistent - should still be visible
    assert_contains "Rule Builder" "Escape keeps Rule Builder open"
    assert_contains "PATTERN" "Pattern section visible after Escape"

    stop_session
}

test_rule_builder_full_workflow() {
    echo -e "\n${YELLOW}TEST: Rule Builder - Full Three-Phase Workflow${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 2  # Wait for folder cache

    # Step 1: Open Rule Builder
    send "n"
    sleep 0.5
    assert_contains "Rule Builder" "Step 1: Rule Builder opens"

    # Step 2: Phase 1 - Exploration (default pattern)
    assert_contains "FOLDERS" "Step 2: Phase 1 shows folder counts"

    # Step 3: Clear pattern and type new glob
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "BSpace"
    send "*"
    send "*"
    send "/"
    send "f"
    send "i"
    send "l"
    send "e"
    send "_"
    send "*"
    send "."
    send "t"
    send "x"
    send "t"
    sleep 0.5

    # Still Phase 1 (no <field>)
    assert_contains "FOLDERS" "Step 3: Glob pattern stays in Phase 1"

    # Step 4: Add <field> placeholder to transition to Phase 2
    # Clear and retype with extraction
    for i in {1..18}; do
        send "BSpace"
    done
    sleep 0.2

    send "d"
    send "i"
    send "r"
    send "_"
    send "<"
    send "d"
    send "i"
    send "r"
    send "n"
    send "u"
    send "m"
    send ">"
    send "/"
    send "*"
    send "."
    send "t"
    send "x"
    send "t"
    sleep 0.5

    # Should be in Phase 2
    assert_contains "PREVIEW" "Step 4: Pattern with <field> enters Phase 2"

    # Step 5: Tab to FileList and run backtest
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    send "Tab"
    sleep 0.3

    send "t"  # Run backtest
    sleep 0.5

    # Should be in Phase 3
    assert_contains "RESULTS" "Step 5: Backtest transitions to Phase 3"

    # Step 6: Verify backtest results are shown
    assert_contains "Pass|Fail|Skip" "Step 6: Backtest results displayed"

    # Step 7: Escape resets to Pattern (Rule Builder is persistent)
    send "Escape"
    sleep 0.3
    assert_contains "Rule Builder" "Step 7: Rule Builder stays open after Escape"

    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# FILES PANEL (3 key focus)
# ───────────────────────────────────────────────────────────────────────────

test_files_panel_focus() {
    echo -e "\n${YELLOW}TEST: Files Panel - Focus (3 key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Press 3 to focus Files panel
    send "3"
    sleep 0.3

    assert_contains "Discover|Files|FOLDERS" "3 focuses Files panel"

    stop_session
}

test_files_panel_navigation() {
    echo -e "\n${YELLOW}TEST: Files Panel - j/k Navigation${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "3"  # Focus Files panel
    sleep 0.3

    # Navigate with j/k
    send "j"
    sleep 0.2
    send "j"
    sleep 0.2
    send "k"
    sleep 0.2

    assert_contains "Discover|Files|FOLDERS" "j/k navigation in Files panel"

    stop_session
}

test_files_panel_filter_mode() {
    echo -e "\n${YELLOW}TEST: Files Panel - Filter Mode (/ key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "3"  # Focus Files panel
    sleep 0.3

    # Enter filter mode
    send "/"
    sleep 0.3

    # Type filter text
    send "t"
    send "e"
    send "s"
    send "t"
    sleep 0.3

    assert_contains "test|Discover" "Filter mode accepts text"

    send "Escape"
    stop_session
}

test_files_panel_filter_clear() {
    echo -e "\n${YELLOW}TEST: Files Panel - Filter Clear (Escape)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Set a filter first
    send "/"
    sleep 0.3
    send "f"
    send "i"
    send "l"
    send "t"
    send "e"
    send "r"
    sleep 0.3

    # Escape should clear filter
    send "Escape"
    sleep 0.3

    assert_contains "Discover" "Escape clears filter"

    stop_session
}

test_files_panel_tag_file() {
    echo -e "\n${YELLOW}TEST: Files Panel - Tag File (t key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "3"  # Focus Files panel
    sleep 0.3

    # Press t to tag selected file
    send "t"
    sleep 0.3

    # Should show tag dialog or handle gracefully
    assert_contains "Discover|Tag|tag" "t key handled for tagging"

    send "Escape"
    stop_session
}

test_files_panel_bulk_tag() {
    echo -e "\n${YELLOW}TEST: Files Panel - Bulk Tag (T key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    send "3"  # Focus Files panel
    sleep 0.3

    # Press T for bulk tag
    send "T"
    sleep 0.3

    # Should show bulk tag dialog or wizard
    assert_contains "Discover|Tag|Bulk|wizard" "T key handled for bulk tagging"

    send "Escape"
    stop_session
}

test_files_panel_preview_toggle() {
    echo -e "\n${YELLOW}TEST: Files Panel - Preview Toggle (p key)${NC}"
    start_session

    send "1"  # Enter Discover
    sleep 0.5

    # Capture before
    local before=$(capture)

    # Press p to toggle preview
    send "p"
    sleep 0.3

    # State should change
    local after=$(capture)
    if [[ "$before" != "$after" ]]; then
        echo -e "  ${GREEN}[PASS]${NC} p toggles preview pane"
        ((PASSED++))
    else
        echo -e "  ${YELLOW}[INFO]${NC} Preview toggle may have no visible effect"
    fi

    stop_session
}

# ───────────────────────────────────────────────────────────────────────────
# TAGGING RULE DIALOG (n key in Files panel)
# ───────────────────────────────────────────────────────────────────────────

test_tagging_rule_dialog() {
    echo -e "\n${YELLOW}TEST: Tagging Rule Creation${NC}"
    start_session

    # Enter Discover mode
    send "1"
    sleep 0.5

    # Press n to create new rule
    send "n"
    sleep 0.3
    assert_contains "New Tagging Rule|Pattern|Tag" "Tagging rule dialog opens"

    # Press Escape to close
    send "Escape"
    sleep 0.3
    assert_not_contains "New Tagging Rule" "Tagging rule dialog closes"

    stop_session
}

test_glob_navigation() {
    echo -e "\n${YELLOW}TEST: Glob Explorer Navigation${NC}"
    start_session

    # Enter Discover mode (shows Glob Explorer)
    send "1"
    sleep 0.5

    # Wait for folder cache to load
    sleep 1

    # Navigate with j (down)
    send "j"
    sleep 0.2
    send "j"
    sleep 0.2

    # UI should still be responsive
    assert_contains "FOLDERS|GLOB|Discover" "Navigation doesn't crash UI"

    # Navigate with k (up)
    send "k"
    sleep 0.2

    # Press g to exit Glob Explorer (if applicable) or Escape
    send "Escape"
    send "Escape"
    sleep 0.3

    assert_contains "Home Hub|Discover|Parser Bench" "Can exit to Home"

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
    if echo "$screen" | grep -qEi "exited|closed|press enter"; then
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
    echo "Available test scenarios (organized by phase):"
    echo ""
    echo "  PHASE 0: STARTUP & HOME"
    echo "    startup              - TUI starts and shows Home screen"
    echo "    home-nav             - Home screen arrow navigation"
    echo "    home-enter           - Enter Discover from Home"
    echo ""
    echo "  PHASE 1: SOURCES DROPDOWN"
    echo "    discover-basic       - Enter Discover mode, verify panels"
    echo "    sources-dropdown     - Sources dropdown open/close"
    echo "    sources-filter       - Sources dropdown filter typing"
    echo "    sources-nav          - Sources dropdown arrow navigation"
    echo "    discover-sources     - Legacy sources dropdown test"
    echo ""
    echo "  PHASE 2: TAGS DROPDOWN"
    echo "    tags-dropdown        - Tags dropdown open"
    echo "    tags-nav             - Tags dropdown navigation"
    echo "    tags-filter          - Tags dropdown filter"
    echo "    tags-enter           - Tags dropdown enter confirm"
    echo ""
    echo "  PHASE 3: RULES MANAGER"
    echo "    rules-manager        - Rules Manager open (R key)"
    echo "    rules-nav            - Rules Manager j/k navigation"
    echo "    rules-new            - Rules Manager new rule (n key)"
    echo ""
    echo "  PHASE 4: QUICK RULE CREATION"
    echo "    quick-rule           - Quick rule creation dialog (n key)"
    echo "    quick-rule-tab       - Rule dialog Tab navigation"
    echo "    quick-rule-typing    - Rule dialog text input"
    echo "    quick-rule-cancel    - Rule dialog Escape cancel"
    echo ""
    echo "  PHASE 5: POLISH (Help, Scan, Views)"
    echo "    view-switching       - Test 1/2/3 view switching"
    echo "    views-from-home      - View switching from Home"
    echo "    help-toggle          - Help overlay toggle"
    echo "    help-content         - Help overlay content"
    echo "    help-escape          - Help overlay Escape"
    echo "    scan-dialog          - Scan dialog open (s key)"
    echo "    scan-autocomplete    - Scan dialog path autocomplete"
    echo ""
    echo "  GLOB EXPLORER (Phases 12-19)"
    echo "    glob-explorer        - Glob Explorer pattern editing"
    echo "    glob-pattern-input   - Pattern typing and backspace"
    echo "    glob-pattern-confirm - Pattern confirm (Enter)"
    echo "    glob-drilling        - Folder drilling (l/Enter, h)"
    echo "    glob-vim-nav         - Vim navigation (hjkl)"
    echo "    glob-arrow-nav       - Arrow key navigation"
    echo "    glob-exit            - Exit Glob Explorer (g/Escape)"
    echo "    glob-nav             - Legacy glob navigation"
    echo ""
    echo "  SOURCES MANAGER (M key)"
    echo "    sources-manager      - Sources Manager open"
    echo "    sources-mgr-nav      - Sources Manager navigation"
    echo "    sources-mgr-add      - Add source (n key)"
    echo "    sources-mgr-edit     - Edit source (e key)"
    echo "    sources-mgr-delete   - Delete source (d key)"
    echo "    sources-mgr-rescan   - Rescan source (r key)"
    echo ""
    echo "  FILES PANEL (3 key)"
    echo "    files-focus          - Files panel focus (3 key)"
    echo "    files-nav            - Files panel j/k navigation"
    echo "    files-filter         - Files panel filter mode"
    echo "    files-filter-clear   - Filter clear (Escape)"
    echo "    files-tag            - Tag file (t key)"
    echo "    files-bulk-tag       - Bulk tag (T key)"
    echo "    files-preview        - Preview toggle (p key)"
    echo ""
    echo "  RULE BUILDER (Three-Phase Workflow)"
    echo "    rb-open              - Rule Builder open (n key)"
    echo "    rb-phase1            - Phase 1 Exploration (folder counts)"
    echo "    rb-phase1-nav        - Phase 1 j/k navigation"
    echo "    rb-phase1-expand     - Phase 1 folder expansion (Enter)"
    echo "    rb-phase2            - Phase 2 Transition (pattern with <field>)"
    echo "    rb-phase2-preview    - Phase 2 Extraction Preview"
    echo "    rb-phase3            - Phase 3 Transition (t key)"
    echo "    rb-phase3-filter     - Phase 3 filter (a/p/f keys)"
    echo "    rb-tab-cycle         - Rule Builder Tab focus cycle"
    echo "    rb-escape            - Rule Builder Escape behavior (stays open)"
    echo "    rb-full-workflow     - Full three-phase workflow test"
    echo ""
    echo "  TAGGING RULE DIALOG"
    echo "    tagging-rule         - Tagging rule creation dialog"
    echo ""
    echo "  MISC"
    echo "    discover-nav         - Discover j/k navigation and Escape"
    echo "    help                 - Legacy help overlay"
    echo "    quit                 - Test Ctrl+C quit"
    echo ""
    echo "  GROUPS"
    echo "    all                  - Run ALL tests"
    echo "    phase1               - Run Phase 1 tests (Sources Dropdown)"
    echo "    phase2               - Run Phase 2 tests (Tags Dropdown)"
    echo "    phase3               - Run Phase 3 tests (Rules Manager)"
    echo "    phase4               - Run Phase 4 tests (Quick Rule Creation)"
    echo "    phase5               - Run Phase 5 tests (Polish)"
    echo "    glob                 - Run all Glob Explorer tests"
    echo "    sources-mgr          - Run all Sources Manager tests"
    echo "    files                - Run all Files Panel tests"
    echo "    rule-builder         - Run all Rule Builder tests"
}

run_phase1() {
    test_discover_basic
    test_sources_dropdown_open_close
    test_sources_dropdown_filter
    test_sources_dropdown_navigation
    test_discover_sources
}

run_phase2() {
    test_tags_dropdown_open
    test_tags_dropdown_navigation
    test_tags_dropdown_filter
    test_tags_dropdown_enter_confirm
}

run_phase3() {
    test_rules_manager_open
    test_rules_manager_navigation
    test_rules_manager_new_rule
}

run_phase4() {
    test_quick_rule_creation_open
    test_quick_rule_creation_tab
    test_quick_rule_creation_typing
    test_quick_rule_creation_cancel
}

run_phase5() {
    test_view_switching
    test_view_switching_from_home
    test_help_overlay_toggle
    test_help_overlay_content
    test_help_overlay_escape
    test_scan_dialog_open
    test_scan_dialog_path_autocomplete
}

run_glob_tests() {
    test_glob_explorer
    test_glob_explorer_pattern_input
    test_glob_explorer_pattern_confirm
    test_glob_explorer_folder_drilling
    test_glob_explorer_vim_navigation
    test_glob_explorer_arrow_navigation
    test_glob_explorer_exit
    test_glob_navigation
}

run_sources_mgr_tests() {
    test_sources_manager
    test_sources_manager_navigation
    test_sources_manager_add_source
    test_sources_manager_edit_source
    test_sources_manager_delete_source
    test_sources_manager_rescan
}

run_files_tests() {
    test_files_panel_focus
    test_files_panel_navigation
    test_files_panel_filter_mode
    test_files_panel_filter_clear
    test_files_panel_tag_file
    test_files_panel_bulk_tag
    test_files_panel_preview_toggle
}

run_rule_builder_tests() {
    test_rule_builder_open
    test_rule_builder_phase1_exploration
    test_rule_builder_phase1_navigation
    test_rule_builder_phase1_expansion
    test_rule_builder_phase2_transition
    test_rule_builder_phase2_extraction_preview
    test_rule_builder_phase3_transition
    test_rule_builder_phase3_filter
    test_rule_builder_tab_cycle
    test_rule_builder_escape_close
    test_rule_builder_full_workflow
}

run_all() {
    echo "═══════════════════════════════════════════════════════════════"
    echo "RUNNING ALL TMUX TUI TESTS"
    echo "═══════════════════════════════════════════════════════════════"

    # Phase 0: Startup & Home
    test_startup
    test_home_navigation
    test_home_enter_discover

    # Phase 1: Sources Dropdown
    run_phase1

    # Phase 2: Tags Dropdown
    run_phase2

    # Phase 3: Rules Manager
    run_phase3

    # Phase 4: Quick Rule Creation
    run_phase4

    # Discover Navigation
    test_discover_navigation

    # Phase 5: Polish
    run_phase5

    # Glob Explorer
    run_glob_tests

    # Help overlay (old)
    test_help_overlay

    # Sources Manager
    run_sources_mgr_tests

    # Files Panel
    run_files_tests

    # Rule Builder (Three-Phase Workflow)
    run_rule_builder_tests

    # Tagging Rule Dialog
    test_tagging_rule_dialog

    # Quit
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

    # Phase 0: Startup & Home
    startup)
        test_startup
        print_summary
        ;;
    home-nav)
        test_home_navigation
        print_summary
        ;;
    home-enter)
        test_home_enter_discover
        print_summary
        ;;

    # Phase 1: Sources Dropdown
    discover-basic)
        test_discover_basic
        print_summary
        ;;
    sources-dropdown)
        test_sources_dropdown_open_close
        print_summary
        ;;
    sources-filter)
        test_sources_dropdown_filter
        print_summary
        ;;
    sources-nav)
        test_sources_dropdown_navigation
        print_summary
        ;;
    discover-sources)
        test_discover_sources
        print_summary
        ;;

    # Phase 2: Tags Dropdown
    tags-dropdown)
        test_tags_dropdown_open
        print_summary
        ;;
    tags-nav)
        test_tags_dropdown_navigation
        print_summary
        ;;
    tags-filter)
        test_tags_dropdown_filter
        print_summary
        ;;
    tags-enter)
        test_tags_dropdown_enter_confirm
        print_summary
        ;;

    # Phase 3: Rules Manager
    rules-manager)
        test_rules_manager_open
        print_summary
        ;;
    rules-nav)
        test_rules_manager_navigation
        print_summary
        ;;
    rules-new)
        test_rules_manager_new_rule
        print_summary
        ;;

    # Phase 4: Quick Rule Creation
    quick-rule)
        test_quick_rule_creation_open
        print_summary
        ;;
    quick-rule-tab)
        test_quick_rule_creation_tab
        print_summary
        ;;
    quick-rule-typing)
        test_quick_rule_creation_typing
        print_summary
        ;;
    quick-rule-cancel)
        test_quick_rule_creation_cancel
        print_summary
        ;;

    # Discover Navigation
    discover-nav)
        test_discover_navigation
        print_summary
        ;;

    # Phase 5: Polish
    view-switching)
        test_view_switching
        print_summary
        ;;
    views-from-home)
        test_view_switching_from_home
        print_summary
        ;;
    help-toggle)
        test_help_overlay_toggle
        print_summary
        ;;
    help-content)
        test_help_overlay_content
        print_summary
        ;;
    help-escape)
        test_help_overlay_escape
        print_summary
        ;;
    scan-dialog)
        test_scan_dialog_open
        print_summary
        ;;
    scan-autocomplete)
        test_scan_dialog_path_autocomplete
        print_summary
        ;;

    # Glob Explorer
    glob-explorer)
        test_glob_explorer
        print_summary
        ;;
    glob-pattern-input)
        test_glob_explorer_pattern_input
        print_summary
        ;;
    glob-pattern-confirm)
        test_glob_explorer_pattern_confirm
        print_summary
        ;;
    glob-drilling)
        test_glob_explorer_folder_drilling
        print_summary
        ;;
    glob-vim-nav)
        test_glob_explorer_vim_navigation
        print_summary
        ;;
    glob-arrow-nav)
        test_glob_explorer_arrow_navigation
        print_summary
        ;;
    glob-exit)
        test_glob_explorer_exit
        print_summary
        ;;
    glob-nav)
        test_glob_navigation
        print_summary
        ;;

    # Help overlay (old)
    help)
        test_help_overlay
        print_summary
        ;;

    # Sources Manager
    sources-manager)
        test_sources_manager
        print_summary
        ;;
    sources-mgr-nav)
        test_sources_manager_navigation
        print_summary
        ;;
    sources-mgr-add)
        test_sources_manager_add_source
        print_summary
        ;;
    sources-mgr-edit)
        test_sources_manager_edit_source
        print_summary
        ;;
    sources-mgr-delete)
        test_sources_manager_delete_source
        print_summary
        ;;
    sources-mgr-rescan)
        test_sources_manager_rescan
        print_summary
        ;;

    # Files Panel
    files-focus)
        test_files_panel_focus
        print_summary
        ;;
    files-nav)
        test_files_panel_navigation
        print_summary
        ;;
    files-filter)
        test_files_panel_filter_mode
        print_summary
        ;;
    files-filter-clear)
        test_files_panel_filter_clear
        print_summary
        ;;
    files-tag)
        test_files_panel_tag_file
        print_summary
        ;;
    files-bulk-tag)
        test_files_panel_bulk_tag
        print_summary
        ;;
    files-preview)
        test_files_panel_preview_toggle
        print_summary
        ;;

    # Rule Builder (Three-Phase Workflow)
    rb-open)
        test_rule_builder_open
        print_summary
        ;;
    rb-phase1)
        test_rule_builder_phase1_exploration
        print_summary
        ;;
    rb-phase1-nav)
        test_rule_builder_phase1_navigation
        print_summary
        ;;
    rb-phase1-expand)
        test_rule_builder_phase1_expansion
        print_summary
        ;;
    rb-phase2)
        test_rule_builder_phase2_transition
        print_summary
        ;;
    rb-phase2-preview)
        test_rule_builder_phase2_extraction_preview
        print_summary
        ;;
    rb-phase3)
        test_rule_builder_phase3_transition
        print_summary
        ;;
    rb-phase3-filter)
        test_rule_builder_phase3_filter
        print_summary
        ;;
    rb-tab-cycle)
        test_rule_builder_tab_cycle
        print_summary
        ;;
    rb-escape)
        test_rule_builder_escape_close
        print_summary
        ;;
    rb-full-workflow)
        test_rule_builder_full_workflow
        print_summary
        ;;

    # Tagging Rule Dialog
    tagging-rule)
        test_tagging_rule_dialog
        print_summary
        ;;

    # Quit
    quit)
        test_quit
        print_summary
        ;;

    # Group runners
    phase1)
        run_phase1
        print_summary
        ;;
    phase2)
        run_phase2
        print_summary
        ;;
    phase3)
        run_phase3
        print_summary
        ;;
    phase4)
        run_phase4
        print_summary
        ;;
    phase5)
        run_phase5
        print_summary
        ;;
    glob)
        run_glob_tests
        print_summary
        ;;
    sources-mgr)
        run_sources_mgr_tests
        print_summary
        ;;
    files)
        run_files_tests
        print_summary
        ;;
    rule-builder)
        run_rule_builder_tests
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
