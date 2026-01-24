#!/bin/bash
# MCP E2E Test Runner using Claude Code CLI
#
# This script runs MCP E2E tests using the real Claude Code CLI.
# Uses Claude CLI session authentication by default (no API key required).
#
# Requirements:
# - Claude Code CLI installed (claude)
# - Authenticated Claude CLI session (run: claude login)
# - Casparian binary built
#
# Usage:
#   ./run_with_claude.sh           # Run backtest E2E
#   ./run_with_claude.sh approval  # Run approval flow E2E
#   ./run_with_claude.sh --dry-run # Show what would be run
#   ./run_with_claude.sh --help    # Show help

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
OUTPUT_DIR="$SCRIPT_DIR/results"
BINARY="$PROJECT_ROOT/target/release/casparian"

# Default test
TEST_TYPE="backtest"
DRY_RUN=false
VERBOSE=false

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${GREEN}[MCP-E2E]${NC} $1"; }
warn() { echo -e "${YELLOW}[MCP-E2E]${NC} $1"; }
error() { echo -e "${RED}[MCP-E2E]${NC} $1"; }
info() { echo -e "${BLUE}[MCP-E2E]${NC} $1"; }

usage() {
    cat << EOF
Usage: $0 [OPTIONS] [TEST_TYPE]

Run MCP E2E tests using Claude Code CLI.

TEST_TYPE:
  backtest       Run backtest E2E test (default)
  approval       Run approval flow E2E test
  evtx           Run full EVTX DFIR workflow test (12 steps)

OPTIONS:
  --dry-run, -n    Show what would be run without executing
  --verbose, -v    Show detailed output
  --help, -h       Show this help message

AUTHENTICATION:
  Uses Claude CLI session authentication by default.
  Run 'claude login' if not authenticated.

  Fallback: Set ANTHROPIC_API_KEY environment variable.

EXAMPLES:
  $0                    # Run backtest test
  $0 approval           # Run approval flow test
  $0 evtx               # Run full EVTX workflow test
  $0 --dry-run backtest # Preview backtest test
EOF
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run|-n)
            DRY_RUN=true
            shift
            ;;
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        --help|-h)
            usage
            ;;
        backtest|approval|evtx)
            TEST_TYPE="$1"
            shift
            ;;
        *)
            error "Unknown option: $1"
            usage
            ;;
    esac
done

# Select prompt file
case "$TEST_TYPE" in
    approval)
        PROMPT_FILE="$SCRIPT_DIR/claude_prompt_approval.md"
        ;;
    evtx)
        PROMPT_FILE="$SCRIPT_DIR/claude_prompt_evtx_workflow.md"
        ;;
    *)
        PROMPT_FILE="$SCRIPT_DIR/claude_prompt.md"
        ;;
esac

# ============================================================================
# Prerequisites Check
# ============================================================================

log "Checking prerequisites..."

# Check Claude CLI
if ! command -v claude &> /dev/null; then
    error "Claude Code CLI not found."
    echo ""
    echo "Install Claude Code CLI:"
    echo "  npm install -g @anthropic-ai/claude-code"
    echo ""
    exit 1
fi

# Auth preflight: test if Claude CLI is authenticated
auth_check() {
    log "Checking Claude CLI authentication..."

    # Try a minimal request
    AUTH_RESULT=$(timeout 30 claude -p "respond with exactly: ok" --max-turns 1 2>&1) || true

    if echo "$AUTH_RESULT" | grep -qi "ok"; then
        log "Claude CLI authenticated via session"
        return 0
    fi

    # Check for auth errors
    if echo "$AUTH_RESULT" | grep -qiE "auth|login|credentials|unauthorized|api.key"; then
        return 1
    fi

    # May have other errors but auth seems OK
    if [[ -n "$AUTH_RESULT" ]]; then
        return 0
    fi

    return 1
}

CLAUDE_AUTH_OK=false
if auth_check; then
    CLAUDE_AUTH_OK=true
else
    warn "Claude CLI not authenticated via session"

    if [[ -n "$ANTHROPIC_API_KEY" ]]; then
        log "Falling back to ANTHROPIC_API_KEY"
        CLAUDE_AUTH_OK=true
    else
        error "Claude CLI not authenticated."
        echo ""
        echo "To authenticate, run:"
        echo "  claude login"
        echo ""
        echo "Or set ANTHROPIC_API_KEY environment variable as fallback."
        exit 1
    fi
fi

# Check prompt file
if [[ ! -f "$PROMPT_FILE" ]]; then
    error "Prompt file not found: $PROMPT_FILE"
    exit 1
fi

# Build binary if needed
if [[ ! -f "$BINARY" ]]; then
    log "Building casparian binary..."
    if [[ "$DRY_RUN" == "true" ]]; then
        echo "Would run: cargo build --release -p casparian"
    else
        (cd "$PROJECT_ROOT" && cargo build --release -p casparian)
    fi
fi

# Check .mcp.json
if [[ ! -f "$PROJECT_ROOT/.mcp.json" ]]; then
    error "MCP config not found at $PROJECT_ROOT/.mcp.json"
    exit 1
fi

# ============================================================================
# Setup Test Environment
# ============================================================================

log "Setting up test environment..."

# Create temp CASPARIAN_FLOW_HOME for isolation
TEST_HOME=$(mktemp -d)
export CASPARIAN_FLOW_HOME="$TEST_HOME"

cleanup() {
    if [[ -d "$TEST_HOME" ]]; then
        rm -rf "$TEST_HOME"
    fi
}
trap cleanup EXIT

mkdir -p "$TEST_HOME"
mkdir -p "$OUTPUT_DIR"

# Generate run ID
RUN_ID=$(date +%Y%m%d_%H%M%S)_$(openssl rand -hex 4 2>/dev/null || echo "$$")
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
OUTPUT_FILE="$OUTPUT_DIR/${TEST_TYPE}_${RUN_ID}.json"
RAW_OUTPUT="$OUTPUT_DIR/${TEST_TYPE}_${RUN_ID}.raw"

log "Test: $TEST_TYPE"
log "Run ID: $RUN_ID"
log "CASPARIAN_FLOW_HOME: $TEST_HOME"
log "Output: $OUTPUT_FILE"

# ============================================================================
# Dry Run
# ============================================================================

if [[ "$DRY_RUN" == "true" ]]; then
    log "DRY RUN - Would execute:"
    echo ""
    echo "Working directory: $PROJECT_ROOT"
    echo "Prompt file: $PROMPT_FILE"
    echo "Test home: $TEST_HOME"
    echo ""
    echo "Claude command:"
    echo "  claude -p <prompt> --dangerously-skip-permissions --max-turns 25"
    echo ""
    exit 0
fi

# ============================================================================
# Run Claude
# ============================================================================

log "Running Claude Code CLI..."
cd "$PROJECT_ROOT"

# Read prompt and append run context
PROMPT=$(cat "$PROMPT_FILE")

# Set context based on test type
case "$TEST_TYPE" in
    evtx)
        TEST_FIXTURES="tests/fixtures/evtx/"
        TEST_PARSER="evtx_native"
        ;;
    *)
        TEST_FIXTURES="tests/fixtures/fix/"
        TEST_PARSER="parsers/fix/fix_parser.py"
        ;;
esac

FULL_PROMPT="$PROMPT

## Current Test Run Context
- Run ID: $RUN_ID
- Timestamp: $TIMESTAMP
- Test Type: $TEST_TYPE
- Test Fixtures: $TEST_FIXTURES
- Parser: $TEST_PARSER

Execute the test now and return the JSON result."

# Run Claude with MCP
# --dangerously-skip-permissions: allows non-interactive execution
# --max-turns: limits iterations
# Note: Claude CLI uses session auth or ANTHROPIC_API_KEY automatically
set +e
CLAUDE_OUTPUT=$(claude -p "$FULL_PROMPT" \
    --dangerously-skip-permissions \
    --max-turns 50 \
    2>&1)
CLAUDE_EXIT=$?
set -e

# Save raw output
echo "$CLAUDE_OUTPUT" > "$RAW_OUTPUT"

if [[ $CLAUDE_EXIT -ne 0 ]]; then
    warn "Claude exited with code $CLAUDE_EXIT"
fi

# ============================================================================
# Extract and Validate Result
# ============================================================================

log "Processing results..."

# Try to extract JSON from output
# Look for our expected result format
JSON_RESULT=""

# Method 1: Look for JSON block in markdown
if [[ -z "$JSON_RESULT" ]]; then
    JSON_RESULT=$(echo "$CLAUDE_OUTPUT" | sed -n '/```json/,/```/p' | sed '1d;$d' | head -200)
fi

# Method 2: Look for raw JSON object with our schema (using awk for macOS compat)
if [[ -z "$JSON_RESULT" ]] || ! echo "$JSON_RESULT" | grep -q "test_run_id"; then
    JSON_RESULT=$(echo "$CLAUDE_OUTPUT" | awk '/\{.*"test_run_id"/{found=1} found{print; if(/\}/) exit}' 2>/dev/null || echo "")
fi

# Method 3: Check if entire output is JSON
if [[ -z "$JSON_RESULT" ]] || ! echo "$JSON_RESULT" | grep -q "test_run_id"; then
    if echo "$CLAUDE_OUTPUT" | head -1 | grep -q "^{"; then
        JSON_RESULT=$(echo "$CLAUDE_OUTPUT" | head -100)
    fi
fi

# Validate we got something
if [[ -z "$JSON_RESULT" ]] || ! echo "$JSON_RESULT" | grep -q "test_run_id"; then
    warn "Could not extract structured JSON result"

    # Create failure result
    cat > "$OUTPUT_FILE" << EOF
{
  "test_run_id": "$RUN_ID",
  "timestamp": "$TIMESTAMP",
  "test_type": "$TEST_TYPE",
  "passed": false,
  "error": "Could not extract structured result from Claude output",
  "raw_output_file": "$RAW_OUTPUT"
}
EOF

    error "Test failed - could not parse result"
    echo "Raw output saved to: $RAW_OUTPUT"
    exit 1
fi

# Save JSON result
echo "$JSON_RESULT" > "$OUTPUT_FILE"

# ============================================================================
# Report Results
# ============================================================================

# Parse results (best effort)
if command -v jq &> /dev/null; then
    PASSED=$(jq -r '.passed // .summary.passed // "unknown"' "$OUTPUT_FILE" 2>/dev/null || echo "unknown")

    echo ""
    log "Test Results:"
    jq -r '
        if .summary then
            "  Total: \(.summary.total // 0)",
            "  Passed: \(.summary.passed // 0)",
            "  Failed: \(.summary.failed // 0)"
        elif .passed != null then
            "  Passed: \(.passed)"
        else
            "  (Could not parse summary)"
        end
    ' "$OUTPUT_FILE" 2>/dev/null || echo "  (Could not parse results)"

    if [[ "$VERBOSE" == "true" ]]; then
        echo ""
        log "Full result:"
        jq '.' "$OUTPUT_FILE" 2>/dev/null || cat "$OUTPUT_FILE"
    fi

    # Determine exit code
    if [[ "$PASSED" == "true" ]] || echo "$PASSED" | grep -qE "^[1-9][0-9]*$"; then
        echo ""
        log "Test PASSED"
        exit 0
    elif [[ "$PASSED" == "false" ]] || [[ "$PASSED" == "0" ]]; then
        echo ""
        error "Test FAILED"
        exit 1
    fi
else
    log "Results saved to: $OUTPUT_FILE"
    log "(Install jq for formatted output)"
fi

# Default: check for obvious failure indicators
if grep -q '"passed"\s*:\s*false' "$OUTPUT_FILE"; then
    error "Test FAILED"
    exit 1
fi

log "Test completed"
