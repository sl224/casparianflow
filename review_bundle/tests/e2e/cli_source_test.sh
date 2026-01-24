#!/bin/bash
# E2E test for the source command
# Tests source management: add, ls, show, rm, sync

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

echo "=== Source Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo "DB dir: $DB_DIR"
echo

# Setup test files
echo "Setting up test data..."
mkdir -p "$TEST_DIR/data/nested"
echo "id,name,value" > "$TEST_DIR/data/sample.csv"
echo "1,foo,100" >> "$TEST_DIR/data/sample.csv"
echo '{"key": "value"}' > "$TEST_DIR/data/sample.json"
echo "nested data" > "$TEST_DIR/data/nested/deep.txt"
echo "Test data created."
echo

# Test 1: List sources (empty)
echo "Test 1: List sources (empty)"
OUTPUT=$($BINARY source list 2>&1)
if echo "$OUTPUT" | grep -q "No sources configured"; then
    echo "PASS: Empty source list message"
else
    echo "FAIL: Expected 'No sources configured'"
    exit 1
fi
echo

# Test 2: Add a source
echo "Test 2: Add a source"
OUTPUT=$($BINARY source add "$TEST_DIR/data" --name "test_source" 2>&1)
if echo "$OUTPUT" | grep -q "Added source 'test_source'"; then
    echo "PASS: Source added successfully"
else
    echo "FAIL: Expected source add confirmation"
    echo "$OUTPUT"
    exit 1
fi
echo

# Test 3: List sources (one source)
echo "Test 3: List sources (one source)"
OUTPUT=$($BINARY source list 2>&1)
if echo "$OUTPUT" | grep -q "test_source"; then
    echo "PASS: Source appears in list"
else
    echo "FAIL: Expected source in list"
    exit 1
fi
echo

# Test 4: Show source details
echo "Test 4: Show source details"
OUTPUT=$($BINARY source show test_source 2>&1)
if echo "$OUTPUT" | grep -q "SOURCE: test_source" && echo "$OUTPUT" | grep -q "Path:"; then
    echo "PASS: Source details shown"
else
    echo "FAIL: Expected source details"
    exit 1
fi
echo

# Test 5: Add source with duplicate path fails
echo "Test 5: Add source with duplicate path fails"
OUTPUT=$($BINARY source add "$TEST_DIR/data" 2>&1) || true
if echo "$OUTPUT" | grep -q "Source already exists\|already exists"; then
    echo "PASS: Duplicate path rejected"
else
    echo "FAIL: Expected duplicate rejection"
    exit 1
fi
echo

# Test 6: Add source with duplicate name fails
echo "Test 6: Add source with duplicate name fails"
mkdir -p "$TEST_DIR/other"
OUTPUT=$($BINARY source add "$TEST_DIR/other" --name test_source 2>&1) || true
if echo "$OUTPUT" | grep -q "already exists"; then
    echo "PASS: Duplicate name rejected"
else
    echo "FAIL: Expected duplicate name rejection"
    exit 1
fi
echo

# Test 7: Add source without name (uses directory name)
echo "Test 7: Add source without name (uses directory name)"
mkdir -p "$TEST_DIR/auto_named"
echo "test" > "$TEST_DIR/auto_named/test.txt"
OUTPUT=$($BINARY source add "$TEST_DIR/auto_named" 2>&1)
if echo "$OUTPUT" | grep -q "Added source 'auto_named'"; then
    echo "PASS: Auto-named source added"
else
    echo "FAIL: Expected auto-named source"
    echo "$OUTPUT"
    exit 1
fi
echo

# Test 8: Source sync
echo "Test 8: Source sync"
OUTPUT=$($BINARY source sync test_source 2>&1)
if echo "$OUTPUT" | grep -q "Syncing source 'test_source'"; then
    echo "PASS: Source sync initiated"
else
    echo "FAIL: Expected sync output"
    exit 1
fi
echo

# Test 9: Remove source fails without force (if files exist)
echo "Test 9: Remove source without force"
OUTPUT=$($BINARY source remove test_source 2>&1) || true
# May fail if files exist, or succeed if no files
if echo "$OUTPUT" | grep -q "Removed source\|has.*files\|--force"; then
    echo "PASS: Remove command behaves correctly"
else
    echo "FAIL: Unexpected remove behavior"
    exit 1
fi
echo

# Test 10: Remove source with force
echo "Test 10: Remove source with force"
OUTPUT=$($BINARY source remove test_source --force 2>&1)
if echo "$OUTPUT" | grep -q "Removed source 'test_source'"; then
    echo "PASS: Source removed with force"
else
    # May have already been removed
    if $BINARY source list 2>&1 | grep -q "test_source"; then
        echo "FAIL: Source should be removed"
        exit 1
    else
        echo "PASS: Source already removed"
    fi
fi
echo

# Test 11: Error handling for nonexistent path
echo "Test 11: Error handling for nonexistent path"
OUTPUT=$($BINARY source add "/nonexistent/path" 2>&1) || true
if echo "$OUTPUT" | grep -q "TRY:\|ERROR:\|not found"; then
    echo "PASS: Helpful error for nonexistent path"
else
    echo "FAIL: Expected helpful error"
    exit 1
fi
echo

# Test 12: Error handling for file instead of directory
echo "Test 12: Error handling for file instead of directory"
echo "test" > "$TEST_DIR/testfile.txt"
OUTPUT=$($BINARY source add "$TEST_DIR/testfile.txt" 2>&1) || true
if echo "$OUTPUT" | grep -q "TRY:\|ERROR:\|Not a directory\|directory"; then
    echo "PASS: Helpful error for file instead of directory"
else
    echo "FAIL: Expected helpful error"
    exit 1
fi
echo

# Test 13: JSON output for source list
echo "Test 13: JSON output for source list"
$BINARY source add "$TEST_DIR/data" --name "json_test" 2>&1 > /dev/null
OUTPUT=$($BINARY source list --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    echo "FAIL: Invalid JSON output"
    exit 1
fi
echo

echo "==================================="
echo "All source command tests passed!"
echo "==================================="
