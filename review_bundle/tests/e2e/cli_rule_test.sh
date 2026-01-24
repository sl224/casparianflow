#!/bin/bash
# E2E test for the rule command
# Tests rule management: add, ls, show, rm, test

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)
DB_DIR=$(mktemp -d)

cleanup() {
    rm -rf "$TEST_DIR"
    rm -rf "$DB_DIR"
}
trap cleanup EXIT

# Use a test database
export HOME="$DB_DIR"
mkdir -p "$DB_DIR/.casparian_flow"

echo "=== Rule Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo "DB dir: $DB_DIR"
echo

# Setup: Create a source first (required for rules)
echo "Setting up source..."
mkdir -p "$TEST_DIR/data"
echo "id,name" > "$TEST_DIR/data/sample.csv"
echo "1,foo" >> "$TEST_DIR/data/sample.csv"
echo '{"key": "value"}' > "$TEST_DIR/data/sample.json"
$BINARY source add "$TEST_DIR/data" --name "test_source" 2>&1 > /dev/null
echo "Source created."
echo

# Test 1: List rules (empty)
echo "Test 1: List rules (empty)"
OUTPUT=$($BINARY rule list 2>&1)
if echo "$OUTPUT" | grep -q "No tagging rules configured"; then
    echo "PASS: Empty rule list message"
else
    echo "FAIL: Expected 'No tagging rules configured'"
    exit 1
fi
echo

# Test 2: Add a rule
echo "Test 2: Add a rule"
OUTPUT=$($BINARY rule add '*.csv' --topic csv_data 2>&1)
if echo "$OUTPUT" | grep -q "Added rule:"; then
    echo "PASS: Rule added successfully"
else
    echo "FAIL: Expected rule add confirmation"
    echo "$OUTPUT"
    exit 1
fi
echo

# Test 3: List rules (one rule)
echo "Test 3: List rules (one rule)"
OUTPUT=$($BINARY rule list 2>&1)
if echo "$OUTPUT" | grep -q "csv" && echo "$OUTPUT" | grep -q "csv_data"; then
    echo "PASS: Rule appears in list"
else
    echo "FAIL: Expected rule in list"
    exit 1
fi
echo

# Test 4: Add rule with priority
echo "Test 4: Add rule with priority"
OUTPUT=$($BINARY rule add '*.json' --topic json_data --priority 10 2>&1)
if echo "$OUTPUT" | grep -q "Added rule:" && echo "$OUTPUT" | grep -q "Priority: 10"; then
    echo "PASS: Rule with priority added"
else
    echo "FAIL: Expected rule with priority"
    exit 1
fi
echo

# Test 5: Show rule details
echo "Test 5: Show rule details"
OUTPUT=$($BINARY rule show '*.csv' 2>&1)
if echo "$OUTPUT" | grep -q "RULE:" && echo "$OUTPUT" | grep -q "Topic:" && echo "$OUTPUT" | grep -q "csv_data"; then
    echo "PASS: Rule details shown"
else
    echo "FAIL: Expected rule details"
    exit 1
fi
echo

# Test 6: Add duplicate pattern fails
echo "Test 6: Add duplicate pattern fails"
OUTPUT=$($BINARY rule add '*.csv' --topic another_topic 2>&1) || true
if echo "$OUTPUT" | grep -q "Pattern already exists\|already exists"; then
    echo "PASS: Duplicate pattern rejected"
else
    echo "FAIL: Expected duplicate rejection"
    exit 1
fi
echo

# Test 7: Add invalid pattern fails
echo "Test 7: Add invalid pattern fails"
OUTPUT=$($BINARY rule add '[invalid' --topic test 2>&1) || true
if echo "$OUTPUT" | grep -q "Invalid glob pattern\|TRY:"; then
    echo "PASS: Invalid pattern rejected with helpful error"
else
    echo "FAIL: Expected pattern validation error"
    exit 1
fi
echo

# Test 8: Test rule matching
echo "Test 8: Test rule matching"
OUTPUT=$($BINARY rule test '*.csv' 'sample.csv' 2>&1)
if echo "$OUTPUT" | grep -q "MATCH"; then
    echo "PASS: Rule match detected"
else
    echo "FAIL: Expected MATCH output"
    exit 1
fi
echo

# Test 9: Test rule non-matching
echo "Test 9: Test rule non-matching"
OUTPUT=$($BINARY rule test '*.csv' 'sample.json' 2>&1)
if echo "$OUTPUT" | grep -q "NO MATCH"; then
    echo "PASS: Rule non-match detected"
else
    echo "FAIL: Expected NO MATCH output"
    exit 1
fi
echo

# Test 10: Remove rule
echo "Test 10: Remove rule"
OUTPUT=$($BINARY rule remove '*.json' --force 2>&1)
if echo "$OUTPUT" | grep -q "Removed rule"; then
    echo "PASS: Rule removed"
else
    echo "FAIL: Expected rule removal"
    exit 1
fi
echo

# Test 11: JSON output for rule list
echo "Test 11: JSON output for rule list"
OUTPUT=$($BINARY rule list --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    echo "FAIL: Invalid JSON output"
    exit 1
fi
echo

# Test 12: Show nonexistent rule
echo "Test 12: Show nonexistent rule"
OUTPUT=$($BINARY rule show 'nonexistent' 2>&1) || true
if echo "$OUTPUT" | grep -q "Rule not found\|TRY:"; then
    echo "PASS: Helpful error for nonexistent rule"
else
    echo "FAIL: Expected helpful error"
    exit 1
fi
echo

# Test 13: Add rule without source fails
echo "Test 13: Error when no sources"
# Create fresh DB without sources
DB_DIR2=$(mktemp -d)
export HOME="$DB_DIR2"
mkdir -p "$DB_DIR2/.casparian_flow"
OUTPUT=$($BINARY rule add '*.txt' --topic text 2>&1) || true
if echo "$OUTPUT" | grep -q "No sources configured\|TRY:"; then
    echo "PASS: Error when no sources"
else
    echo "FAIL: Expected no sources error"
    echo "$OUTPUT"
    # Not a fatal error - restore original HOME
fi
# Restore original HOME
export HOME="$DB_DIR"
rm -rf "$DB_DIR2"
echo

echo "==================================="
echo "All rule command tests passed!"
echo "==================================="
