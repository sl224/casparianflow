#!/bin/bash
# E2E test for the scan command
# Tests file discovery with various filters

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo "=== Scan Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo

# Setup test files
echo "Setting up test data..."
mkdir -p "$TEST_DIR/nested/deep"
echo "id,name,value" > "$TEST_DIR/sample.csv"
echo "1,foo,100" >> "$TEST_DIR/sample.csv"
echo "2,bar,200" >> "$TEST_DIR/sample.csv"

echo '{"key": "value", "count": 42}' > "$TEST_DIR/sample.json"

echo "id,deep" > "$TEST_DIR/nested/deep.csv"
echo "1,nested" >> "$TEST_DIR/nested/deep.csv"

echo '{"nested": true}' > "$TEST_DIR/nested/data.json"

echo "This is a text file" > "$TEST_DIR/readme.txt"

# Large file for size testing
dd if=/dev/zero of="$TEST_DIR/large.bin" bs=1024 count=100 2>/dev/null

echo "Test data created."
echo

# Test 1: Basic scan
echo "Test 1: Basic scan (non-recursive)"
OUTPUT=$($BINARY scan "$TEST_DIR" 2>&1)
echo "$OUTPUT" | head -5
if echo "$OUTPUT" | grep -q "Found"; then
    echo "PASS: Found files message present"
else
    echo "FAIL: Expected 'Found' in output"
    exit 1
fi
echo

# Test 2: Recursive scan
echo "Test 2: Recursive scan"
OUTPUT=$($BINARY scan "$TEST_DIR" -r 2>&1)
if echo "$OUTPUT" | grep -q "deep.csv"; then
    echo "PASS: Found nested file"
else
    echo "FAIL: Expected to find nested file"
    exit 1
fi
echo

# Test 3: Type filter (CSV only)
echo "Test 3: Type filter --type csv"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --type csv 2>&1)
if echo "$OUTPUT" | grep -q "csv"; then
    echo "PASS: CSV files found"
else
    echo "FAIL: Expected CSV files"
    exit 1
fi
if echo "$OUTPUT" | grep -q "sample.json\|data.json"; then
    echo "FAIL: JSON files should be filtered out"
    exit 1
else
    echo "PASS: JSON files filtered out"
fi
echo

# Test 4: Multiple type filters
echo "Test 4: Multiple type filters --type csv --type json"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --type csv --type json 2>&1)
if echo "$OUTPUT" | grep -q "csv" && echo "$OUTPUT" | grep -q "json"; then
    echo "PASS: Both CSV and JSON files found"
else
    echo "FAIL: Expected both CSV and JSON files"
    exit 1
fi
echo

# Test 5: JSON output
echo "Test 5: JSON output mode"
OUTPUT=$($BINARY scan "$TEST_DIR" --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    echo "FAIL: Invalid JSON output"
    exit 1
fi
echo

# Test 6: Stats output
echo "Test 6: Stats output mode"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --stats 2>&1)
if echo "$OUTPUT" | grep -q "Files:" || echo "$OUTPUT" | grep -q "Total Size:"; then
    echo "PASS: Stats output present"
else
    echo "FAIL: Expected stats in output"
    exit 1
fi
echo

# Test 7: Quiet mode (paths only)
echo "Test 7: Quiet mode"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --quiet 2>&1)
LINE_COUNT=$(echo "$OUTPUT" | wc -l)
if [ "$LINE_COUNT" -gt 0 ]; then
    echo "PASS: File paths output ($LINE_COUNT files)"
else
    echo "FAIL: Expected file paths"
    exit 1
fi
echo

# Test 8: Size filter (min-size)
echo "Test 8: Min size filter"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --min-size 50KB 2>&1)
if echo "$OUTPUT" | grep -q "large.bin"; then
    echo "PASS: Large file found with min-size filter"
else
    echo "FAIL: Expected large file to be found"
    exit 1
fi
# Small files should be filtered out
if echo "$OUTPUT" | grep -q "sample.csv"; then
    echo "FAIL: Small files should be filtered out"
    exit 1
else
    echo "PASS: Small files filtered out"
fi
echo

# Test 9: Depth limit
echo "Test 9: Depth limit"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --depth 1 2>&1)
if echo "$OUTPUT" | grep -q "deep.csv"; then
    echo "FAIL: Deep files should be excluded with depth 1"
    exit 1
else
    echo "PASS: Deep files excluded with depth limit"
fi
echo

# Test 10: Error handling for missing directory
echo "Test 10: Error handling for missing directory"
OUTPUT=$($BINARY scan "/nonexistent/path/that/does/not/exist" 2>&1) || true
if echo "$OUTPUT" | grep -q "TRY:\|ERROR:\|not found"; then
    echo "PASS: Helpful error message shown"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

# Test 11: Error handling for file instead of directory
echo "Test 11: Error handling for file instead of directory"
OUTPUT=$($BINARY scan "$TEST_DIR/sample.csv" 2>&1) || true
if echo "$OUTPUT" | grep -q "TRY:\|ERROR:\|Not a directory\|directory"; then
    echo "PASS: Helpful error for file instead of directory"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

echo "==================================="
echo "All scan command tests passed!"
echo "==================================="
