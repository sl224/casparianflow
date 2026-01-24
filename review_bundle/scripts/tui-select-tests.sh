#!/bin/bash
# TUI Test Selection Script
# Per specs/meta/tui_validation_workflow.md
#
# Analyzes git changes and recommends which TUI tests to run
#
# Usage:
#   ./scripts/tui-select-tests.sh              # Analyze uncommitted changes
#   ./scripts/tui-select-tests.sh HEAD~1       # Analyze vs previous commit
#   ./scripts/tui-select-tests.sh main         # Analyze vs main branch
#   ./scripts/tui-select-tests.sh --run        # Analyze and run tests
#   ./scripts/tui-select-tests.sh --run main   # Analyze vs main and run

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${CYAN}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_section() {
    echo -e "\n${BOLD}$1${NC}"
    echo "────────────────────────────────────────"
}

# Parse arguments
RUN_TESTS=false
COMPARE_REF=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --run)
            RUN_TESTS=true
            shift
            ;;
        *)
            COMPARE_REF="$1"
            shift
            ;;
    esac
done

# Get changed files
if [[ -z "$COMPARE_REF" ]]; then
    # Uncommitted changes (staged + unstaged)
    CHANGED_FILES=$(git diff --name-only HEAD 2>/dev/null || git diff --name-only)
    if [[ -z "$CHANGED_FILES" ]]; then
        CHANGED_FILES=$(git diff --name-only --cached)
    fi
    log_info "Analyzing uncommitted changes..."
else
    CHANGED_FILES=$(git diff --name-only "$COMPARE_REF" 2>/dev/null)
    log_info "Analyzing changes vs $COMPARE_REF..."
fi

if [[ -z "$CHANGED_FILES" ]]; then
    log_warn "No changes detected. Nothing to test."
    exit 0
fi

log_section "Changed Files"
echo "$CHANGED_FILES" | while read -r file; do
    echo "  $file"
done

# Initialize test flags
NEED_SMOKE=false
NEED_HOME=false
NEED_DISCOVER=false
NEED_RULE_DIALOG=false
NEED_GLOB_HANDOFF=false
NEED_LATENCY=false
NEED_JOBS=false
NEED_FULL_MANUAL=false

# Analyze each changed file
log_section "Impact Analysis"

echo "$CHANGED_FILES" | while read -r file; do
    case "$file" in
        # Core TUI infrastructure - run everything
        crates/casparian/src/cli/tui/app.rs)
            echo "  $file -> ALL TESTS (core state machine)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            echo "NEED_HOME=true" >> /tmp/tui_test_flags
            echo "NEED_DISCOVER=true" >> /tmp/tui_test_flags
            echo "NEED_RULE_DIALOG=true" >> /tmp/tui_test_flags
            echo "NEED_GLOB_HANDOFF=true" >> /tmp/tui_test_flags
            echo "NEED_LATENCY=true" >> /tmp/tui_test_flags
            echo "NEED_JOBS=true" >> /tmp/tui_test_flags
            ;;
        crates/casparian/src/cli/tui/ui.rs)
            echo "  $file -> ALL VISUAL TESTS (rendering)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            echo "NEED_HOME=true" >> /tmp/tui_test_flags
            echo "NEED_DISCOVER=true" >> /tmp/tui_test_flags
            echo "NEED_RULE_DIALOG=true" >> /tmp/tui_test_flags
            echo "NEED_FULL_MANUAL=true" >> /tmp/tui_test_flags
            ;;
        crates/casparian/src/cli/tui/event.rs)
            echo "  $file -> ALL INPUT TESTS (key handling)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            echo "NEED_DISCOVER=true" >> /tmp/tui_test_flags
            echo "NEED_RULE_DIALOG=true" >> /tmp/tui_test_flags
            echo "NEED_LATENCY=true" >> /tmp/tui_test_flags
            ;;

        # Scout/Scanner - Discover mode
        crates/casparian/src/scout/*.rs)
            echo "  $file -> Discover mode (file discovery)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            echo "NEED_DISCOVER=true" >> /tmp/tui_test_flags
            ;;

        # Scan command
        crates/casparian/src/cli/scan.rs)
            echo "  $file -> Discover mode (scan functionality)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            echo "NEED_DISCOVER=true" >> /tmp/tui_test_flags
            ;;

        # Jobs mode
        crates/casparian/src/cli/tui/*job*.rs)
            echo "  $file -> Jobs mode"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            echo "NEED_JOBS=true" >> /tmp/tui_test_flags
            ;;

        # View specs - manual review
        specs/views/*.md)
            echo "  $file -> Manual verification (spec changed)"
            echo "NEED_FULL_MANUAL=true" >> /tmp/tui_test_flags
            ;;

        # Test scripts themselves
        scripts/tui-*.sh)
            echo "  $file -> Smoke tests (test infrastructure)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            ;;

        # TUI tests
        crates/casparian/tests/tui*.rs)
            echo "  $file -> Smoke tests (test code)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            ;;

        # Extraction feature
        crates/casparian/src/cli/tui/extraction.rs)
            echo "  $file -> Discover mode (extraction)"
            echo "NEED_DISCOVER=true" >> /tmp/tui_test_flags
            echo "NEED_RULE_DIALOG=true" >> /tmp/tui_test_flags
            ;;

        # LLM integration
        crates/casparian/src/cli/tui/llm/*.rs)
            echo "  $file -> LLM features (if used in TUI)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            ;;

        # Other Rust files in casparian crate
        crates/casparian/src/*.rs)
            echo "  $file -> Smoke tests (general)"
            echo "NEED_SMOKE=true" >> /tmp/tui_test_flags
            ;;

        *)
            echo "  $file -> (no TUI impact)"
            ;;
    esac
done

# Read flags from temp file
if [[ -f /tmp/tui_test_flags ]]; then
    source /tmp/tui_test_flags 2>/dev/null || true
    rm -f /tmp/tui_test_flags
fi

# Dedupe flags by re-sourcing
NEED_SMOKE=$(grep -c "NEED_SMOKE=true" /tmp/tui_test_flags 2>/dev/null || echo 0)
NEED_HOME=$(grep -c "NEED_HOME=true" /tmp/tui_test_flags 2>/dev/null || echo 0)
NEED_DISCOVER=$(grep -c "NEED_DISCOVER=true" /tmp/tui_test_flags 2>/dev/null || echo 0)
NEED_RULE_DIALOG=$(grep -c "NEED_RULE_DIALOG=true" /tmp/tui_test_flags 2>/dev/null || echo 0)
NEED_GLOB_HANDOFF=$(grep -c "NEED_GLOB_HANDOFF=true" /tmp/tui_test_flags 2>/dev/null || echo 0)
NEED_LATENCY=$(grep -c "NEED_LATENCY=true" /tmp/tui_test_flags 2>/dev/null || echo 0)
NEED_JOBS=$(grep -c "NEED_JOBS=true" /tmp/tui_test_flags 2>/dev/null || echo 0)
NEED_FULL_MANUAL=$(grep -c "NEED_FULL_MANUAL=true" /tmp/tui_test_flags 2>/dev/null || echo 0)

# Clean up
rm -f /tmp/tui_test_flags 2>/dev/null || true

# Generate recommendations
log_section "Recommended Tests"

TESTS_TO_RUN=""

# Re-analyze for recommendations
TUI_IMPACT=false
echo "$CHANGED_FILES" | grep -q "crates/casparian/src/cli/tui\|crates/casparian/src/scout\|scripts/tui" && TUI_IMPACT=true

if [[ "$TUI_IMPACT" == "true" ]]; then
    # Check for core infrastructure changes
    if echo "$CHANGED_FILES" | grep -q "app.rs\|ui.rs\|event.rs"; then
        log_warn "Core TUI infrastructure changed - recommend full test suite"
        echo "  ./scripts/tui-test-workflow.sh smoke"
        TESTS_TO_RUN="smoke"
    else
        # Targeted tests based on specific files
        if echo "$CHANGED_FILES" | grep -q "scout\|scan.rs"; then
            log_success "Discover mode: test-discover"
            TESTS_TO_RUN="${TESTS_TO_RUN} test-discover"
        fi

        if echo "$CHANGED_FILES" | grep -q "app.rs" | grep -qi "rule\|dialog"; then
            log_success "Rule dialog: test-rule-dialog"
            TESTS_TO_RUN="${TESTS_TO_RUN} test-rule-dialog"
        fi

        if echo "$CHANGED_FILES" | grep -q "glob\|explorer"; then
            log_success "Glob handoff: test-glob-handoff"
            TESTS_TO_RUN="${TESTS_TO_RUN} test-glob-handoff"
        fi

        if [[ -z "$TESTS_TO_RUN" ]]; then
            log_success "Smoke tests (minimal impact)"
            echo "  ./scripts/tui-test-workflow.sh smoke"
            TESTS_TO_RUN="smoke"
        fi
    fi
else
    log_success "No TUI-impacting changes detected"
    echo "  No TUI tests required"
    exit 0
fi

# Check for spec changes requiring manual review
if echo "$CHANGED_FILES" | grep -q "specs/views/"; then
    log_warn "View specs changed - recommend manual verification"
    echo "  ./scripts/tui-test-workflow.sh attach"
    echo "  Review: specs/meta/tui_testing_workflow.md Phase 5 checklist"
fi

# Print command
log_section "Run Command"

if [[ "$TESTS_TO_RUN" == "smoke" ]]; then
    echo "  ./scripts/tui-test-workflow.sh smoke"
    RUN_CMD="./scripts/tui-test-workflow.sh smoke"
else
    echo "  ./scripts/tui-test-workflow.sh smoke"
    RUN_CMD="./scripts/tui-test-workflow.sh smoke"
    for test in $TESTS_TO_RUN; do
        if [[ "$test" != "smoke" ]]; then
            echo "  ./scripts/tui-test-workflow.sh $test"
        fi
    done
fi

# Run tests if requested
if [[ "$RUN_TESTS" == "true" ]]; then
    log_section "Running Tests"

    # Always run smoke first
    ./scripts/tui-test-workflow.sh smoke
    RESULT=$?

    if [[ $RESULT -ne 0 ]]; then
        log_warn "Smoke tests failed with $RESULT failures"
        exit $RESULT
    fi

    # Run additional targeted tests
    for test in $TESTS_TO_RUN; do
        if [[ "$test" != "smoke" ]]; then
            ./scripts/tui-test-workflow.sh "$test"
            RESULT=$?
            if [[ $RESULT -ne 0 ]]; then
                log_warn "$test failed with $RESULT failures"
                exit $RESULT
            fi
        fi
    done

    log_success "All TUI tests passed!"
fi
