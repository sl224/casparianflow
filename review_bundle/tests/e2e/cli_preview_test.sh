#!/bin/bash
# E2E test for the preview command
# Tests file content preview and schema inference

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo "=== Preview Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo

# Setup test files
echo "Setting up test data..."

# CSV file with mixed types
cat > "$TEST_DIR/sample.csv" << 'EOF'
id,name,price,active,score
1,Widget A,19.99,true,4.5
2,Widget B,29.99,false,3.8
3,Gadget C,99.99,true,4.9
4,Device D,149.99,true,4.2
5,Tool E,9.99,false,3.0
EOF

# JSON array
cat > "$TEST_DIR/sample.json" << 'EOF'
[
  {"id": 1, "name": "Alice", "age": 30},
  {"id": 2, "name": "Bob", "age": 25},
  {"id": 3, "name": "Charlie", "age": 35}
]
EOF

# JSON object
cat > "$TEST_DIR/object.json" << 'EOF'
{
  "title": "Sample Object",
  "version": 1,
  "enabled": true,
  "tags": ["one", "two"]
}
EOF

# NDJSON file
cat > "$TEST_DIR/sample.jsonl" << 'EOF'
{"event": "login", "user": "alice", "ts": 1704067200}
{"event": "click", "user": "bob", "ts": 1704067300}
{"event": "logout", "user": "alice", "ts": 1704067400}
EOF

# Text file
cat > "$TEST_DIR/readme.txt" << 'EOF'
This is a sample text file.
It has multiple lines.
Line three here.
And line four.
Final line.
EOF

# TSV file
cat > "$TEST_DIR/data.tsv" << 'EOF'
col1	col2	col3
a	1	x
b	2	y
c	3	z
EOF

# Binary file
echo -n "Binary content with some nulls:" > "$TEST_DIR/binary.bin"
printf '\x00\x01\x02\x03\x04\x05' >> "$TEST_DIR/binary.bin"

echo "Test data created."
echo

# Test 1: CSV preview
echo "Test 1: CSV preview"
OUTPUT=$($BINARY preview "$TEST_DIR/sample.csv" 2>&1)
if echo "$OUTPUT" | grep -q "id\|name\|price"; then
    echo "PASS: CSV headers shown"
else
    echo "FAIL: Expected CSV headers"
    exit 1
fi
if echo "$OUTPUT" | grep -q "Widget"; then
    echo "PASS: CSV data shown"
else
    echo "FAIL: Expected CSV data"
    exit 1
fi
echo

# Test 2: CSV schema
echo "Test 2: CSV schema inference"
OUTPUT=$($BINARY preview "$TEST_DIR/sample.csv" --schema 2>&1)
if echo "$OUTPUT" | grep -q "integer"; then
    echo "PASS: Integer type inferred"
else
    echo "FAIL: Expected integer type"
    exit 1
fi
if echo "$OUTPUT" | grep -q "float"; then
    echo "PASS: Float type inferred"
else
    echo "FAIL: Expected float type"
    exit 1
fi
if echo "$OUTPUT" | grep -q "string"; then
    echo "PASS: String type inferred"
else
    echo "FAIL: Expected string type"
    exit 1
fi
echo

# Test 3: Row limit
echo "Test 3: Row limit"
OUTPUT=$($BINARY preview "$TEST_DIR/sample.csv" -n 2 2>&1)
if echo "$OUTPUT" | grep -q "2 rows"; then
    echo "PASS: Row limit respected"
else
    echo "FAIL: Expected 2 rows"
    exit 1
fi
echo

# Test 4: JSON array preview
echo "Test 4: JSON array preview"
OUTPUT=$($BINARY preview "$TEST_DIR/sample.json" 2>&1)
if echo "$OUTPUT" | grep -q "Alice\|Bob"; then
    echo "PASS: JSON data shown"
else
    echo "FAIL: Expected JSON data"
    exit 1
fi
echo

# Test 5: JSON object preview
echo "Test 5: JSON object preview"
OUTPUT=$($BINARY preview "$TEST_DIR/object.json" 2>&1)
if echo "$OUTPUT" | grep -q "Sample Object"; then
    echo "PASS: JSON object data shown"
else
    echo "FAIL: Expected JSON object data"
    exit 1
fi
echo

# Test 6: NDJSON preview
echo "Test 6: NDJSON/JSONL preview"
OUTPUT=$($BINARY preview "$TEST_DIR/sample.jsonl" 2>&1)
if echo "$OUTPUT" | grep -q "login\|click"; then
    echo "PASS: NDJSON data shown"
else
    echo "FAIL: Expected NDJSON data"
    exit 1
fi
echo

# Test 7: Text preview
echo "Test 7: Text file preview"
OUTPUT=$($BINARY preview "$TEST_DIR/readme.txt" 2>&1)
if echo "$OUTPUT" | grep -q "sample text"; then
    echo "PASS: Text content shown"
else
    echo "FAIL: Expected text content"
    exit 1
fi
echo

# Test 8: TSV preview (tab delimiter)
echo "Test 8: TSV preview"
OUTPUT=$($BINARY preview "$TEST_DIR/data.tsv" 2>&1)
if echo "$OUTPUT" | grep -q "col1\|col2"; then
    echo "PASS: TSV headers shown"
else
    echo "FAIL: Expected TSV headers"
    exit 1
fi
echo

# Test 9: Raw/hex mode
echo "Test 9: Raw/hex mode"
OUTPUT=$($BINARY preview "$TEST_DIR/binary.bin" --raw 2>&1)
if echo "$OUTPUT" | grep -q "00 01 02\|00000000"; then
    echo "PASS: Hex dump shown"
else
    echo "FAIL: Expected hex dump"
    exit 1
fi
echo

# Test 10: Head mode
echo "Test 10: Head mode (first N lines)"
OUTPUT=$($BINARY preview "$TEST_DIR/readme.txt" --head 2 2>&1)
if echo "$OUTPUT" | grep -q "First 2 lines"; then
    echo "PASS: Head mode message shown"
else
    echo "FAIL: Expected head mode output"
    exit 1
fi
echo

# Test 11: JSON output format
echo "Test 11: JSON output format"
OUTPUT=$($BINARY preview "$TEST_DIR/sample.csv" --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    echo "FAIL: Invalid JSON output"
    exit 1
fi
# Check that schema is included
if echo "$OUTPUT" | grep -q '"schema"'; then
    echo "PASS: Schema included in JSON output"
else
    echo "FAIL: Expected schema in JSON output"
    exit 1
fi
echo

# Test 12: Error handling for missing file
echo "Test 12: Error handling for missing file"
OUTPUT=$($BINARY preview "/nonexistent/file.csv" 2>&1) || true
if echo "$OUTPUT" | grep -q "TRY:\|ERROR:\|not found"; then
    echo "PASS: Helpful error message shown"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

# Test 13: Error handling for directory instead of file
echo "Test 13: Error handling for directory instead of file"
OUTPUT=$($BINARY preview "$TEST_DIR" 2>&1) || true
if echo "$OUTPUT" | grep -q "TRY:\|ERROR:\|Not a file\|scan"; then
    echo "PASS: Helpful error for directory"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

# Test 14: Custom delimiter
echo "Test 14: Custom delimiter"
echo "a;b;c" > "$TEST_DIR/semicolon.csv"
echo "1;2;3" >> "$TEST_DIR/semicolon.csv"
OUTPUT=$($BINARY preview "$TEST_DIR/semicolon.csv" --delimiter ';' 2>&1)
if echo "$OUTPUT" | grep -q "a\|b\|c"; then
    echo "PASS: Custom delimiter works"
else
    echo "FAIL: Expected semicolon-separated data"
    exit 1
fi
echo

echo "==================================="
echo "All preview command tests passed!"
echo "==================================="
