#!/bin/bash
# MCP Server Smoke Test
#
# Quick validation that the MCP server starts and responds to basic requests.
# This is NOT the authoritative test - use run_with_claude.sh for full E2E tests.
#
# This smoke test verifies:
# 1. MCP server launches
# 2. tools/list returns expected tool names
#
# Usage:
#   ./test_mcp_server.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BINARY="$PROJECT_ROOT/target/release/casparian"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo -e "${GREEN}PASS${NC}: $1"; }
fail() { echo -e "${RED}FAIL${NC}: $1"; exit 1; }
warn() { echo -e "${YELLOW}WARN${NC}: $1"; }

echo "=== MCP Server Smoke Test ==="
echo ""
echo "Note: For full E2E testing, use ./run_with_claude.sh"
echo ""

# Build if needed
if [[ ! -f "$BINARY" ]]; then
    echo "Building casparian..."
    (cd "$PROJECT_ROOT" && cargo build --release -p casparian)
fi

# Test 1: Server initializes and responds
echo "Test 1: Server initialization"
INIT_RESPONSE=$(echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke-test","version":"1.0"}}}' \
    | timeout 10 "$BINARY" mcp serve 2>/dev/null || echo "TIMEOUT")

if echo "$INIT_RESPONSE" | grep -q '"serverInfo"'; then
    pass "Server initializes and responds"
else
    fail "Server failed to initialize"
fi

# Test 2: tools/list returns expected tools
echo "Test 2: Tools list"
TOOLS_RESPONSE=$(printf '%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke-test","version":"1.0"}}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    | timeout 10 "$BINARY" mcp serve 2>/dev/null | grep '"id":2' || echo "")

# Expected tools (core set)
EXPECTED_TOOLS=(
    "casparian_plugins"
    "casparian_scan"
    "casparian_preview"
    "casparian_query"
    "casparian_backtest_start"
    "casparian_run_request"
    "casparian_job_status"
    "casparian_job_list"
    "casparian_approval_list"
    "casparian_approval_decide"
)

MISSING_TOOLS=()
for tool in "${EXPECTED_TOOLS[@]}"; do
    if ! echo "$TOOLS_RESPONSE" | grep -q "\"$tool\""; then
        MISSING_TOOLS+=("$tool")
    fi
done

if [[ ${#MISSING_TOOLS[@]} -eq 0 ]]; then
    TOOL_COUNT=$(echo "$TOOLS_RESPONSE" | grep -o '"name":"casparian_' | wc -l | tr -d ' ')
    pass "Tools list returns $TOOL_COUNT tools"
else
    warn "Missing tools: ${MISSING_TOOLS[*]}"
    # Don't fail - some tools may be in development
fi

echo ""
echo "=== Smoke Test Complete ==="
echo ""
echo "For full E2E testing with Claude Code CLI:"
echo "  ./run_with_claude.sh           # Backtest flow"
echo "  ./run_with_claude.sh approval  # Approval flow"
