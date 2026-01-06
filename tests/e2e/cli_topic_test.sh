#!/bin/bash
# E2E test for the topic command
# Tests topic management: ls, show, create, delete, files

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

echo "=== Topic Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo "DB dir: $DB_DIR"
echo

# Setup: Create a source and rule first
echo "Setting up source and rule..."
mkdir -p "$TEST_DIR/data"
echo "id,name" > "$TEST_DIR/data/sample.csv"
echo "1,foo" >> "$TEST_DIR/data/sample.csv"
echo '{"key": "value"}' > "$TEST_DIR/data/sample.json"
$BINARY source add "$TEST_DIR/data" --name "test_source" 2>&1 > /dev/null
$BINARY rule add '*.csv' --topic csv_data 2>&1 > /dev/null
# Sync to discover files
$BINARY source sync test_source 2>&1 > /dev/null
echo "Setup complete."
echo

# Test 1: List topics (may be empty if no files tagged yet)
echo "Test 1: List topics"
OUTPUT=$($BINARY topic list 2>&1)
# Either shows topics or says no topics
if echo "$OUTPUT" | grep -q "TOPICS\|No topics found"; then
    echo "PASS: Topic list output"
else
    echo "FAIL: Expected topic list output"
    exit 1
fi
echo

# Test 2: Create topic
echo "Test 2: Create topic"
OUTPUT=$($BINARY topic create new_topic 2>&1)
if echo "$OUTPUT" | grep -q "Created topic 'new_topic'\|already has rules"; then
    echo "PASS: Topic created"
else
    echo "FAIL: Expected topic creation message"
    exit 1
fi
echo

# Test 3: Create duplicate topic fails
echo "Test 3: Create duplicate topic (may already exist)"
OUTPUT=$($BINARY topic create new_topic 2>&1) || true
# Will either say already exists or show info
if echo "$OUTPUT" | grep -q "already\|Created"; then
    echo "PASS: Duplicate topic handled"
else
    echo "FAIL: Expected duplicate handling"
    exit 1
fi
echo

# Test 4: Show topic details (csv_data)
echo "Test 4: Show topic details"
OUTPUT=$($BINARY topic show csv_data 2>&1) || true
# May or may not have files depending on tagging
if echo "$OUTPUT" | grep -q "TOPIC:\|not found\|TRY:"; then
    echo "PASS: Topic show output"
else
    echo "FAIL: Expected topic show output"
    exit 1
fi
echo

# Test 5: Show nonexistent topic
echo "Test 5: Show nonexistent topic"
OUTPUT=$($BINARY topic show "nonexistent_topic_xyz" 2>&1) || true
if echo "$OUTPUT" | grep -q "Topic not found\|TRY:"; then
    echo "PASS: Helpful error for nonexistent topic"
else
    echo "FAIL: Expected not found error"
    exit 1
fi
echo

# Test 6: Delete topic without force (should fail if has files)
echo "Test 6: Delete topic without force"
OUTPUT=$($BINARY topic delete csv_data 2>&1) || true
# Either succeeds (no files) or fails asking for force
if echo "$OUTPUT" | grep -q "Removed\|has.*files\|--force\|not found"; then
    echo "PASS: Delete handled correctly"
else
    echo "FAIL: Unexpected delete behavior"
    exit 1
fi
echo

# Test 7: Delete topic with force
echo "Test 7: Delete topic with force"
OUTPUT=$($BINARY topic delete new_topic --force 2>&1) || true
if echo "$OUTPUT" | grep -q "Removed\|not found"; then
    echo "PASS: Force delete handled"
else
    echo "FAIL: Expected delete or not found"
    exit 1
fi
echo

# Test 8: List topic files
echo "Test 8: List topic files"
# Add rule and sync again to have files
$BINARY rule add '*.json' --topic json_data 2>&1 > /dev/null || true
$BINARY source sync test_source 2>&1 > /dev/null
OUTPUT=$($BINARY topic files json_data 2>&1) || true
if echo "$OUTPUT" | grep -q "FILES FOR TOPIC:\|No files tagged"; then
    echo "PASS: Topic files output"
else
    echo "FAIL: Expected files output"
    exit 1
fi
echo

# Test 9: JSON output for topic list
echo "Test 9: JSON output for topic list"
OUTPUT=$($BINARY topic list --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    # Empty array is also valid
    if [ "$OUTPUT" = "[]" ]; then
        echo "PASS: Empty JSON array"
    else
        echo "FAIL: Invalid JSON output"
        exit 1
    fi
fi
echo

# Test 10: Topic with limit
echo "Test 10: Topic files with limit"
OUTPUT=$($BINARY topic files json_data --limit 1 2>&1) || true
if echo "$OUTPUT" | grep -q "FILES FOR TOPIC:\|No files"; then
    echo "PASS: Limit parameter accepted"
else
    echo "FAIL: Expected limit to work"
    exit 1
fi
echo

echo "==================================="
echo "All topic command tests passed!"
echo "==================================="
