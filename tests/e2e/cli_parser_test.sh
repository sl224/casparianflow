#!/bin/bash
# E2E test for the parser command
# Tests parser management: list, test, publish, show, unpublish, backtest

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)
DB_FILE="$TEST_DIR/test.db"

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo "=== Parser Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo "DB: $DB_FILE"
echo

# Set up database path
export CASPARIAN_DB_PATH="$DB_FILE"

# Create a minimal SQLite database with the required schema
echo "Setting up test database..."
duckdb "$DB_FILE" <<'EOF'
-- Parser Lab parsers table
CREATE TABLE IF NOT EXISTS parser_lab_parsers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    file_pattern TEXT NOT NULL DEFAULT '',
    pattern_type TEXT DEFAULT 'all',
    source_code TEXT,
    validation_status TEXT DEFAULT 'pending',
    validation_error TEXT,
    validation_output TEXT,
    last_validated_at INTEGER,
    messages_json TEXT,
    schema_json TEXT,
    sink_type TEXT DEFAULT 'parquet',
    sink_config_json TEXT,
    published_at INTEGER,
    published_plugin_id INTEGER,
    is_sample INTEGER DEFAULT 0,
    output_mode TEXT DEFAULT 'single',
    detected_topics_json TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Parser Lab test files
CREATE TABLE IF NOT EXISTS parser_lab_test_files (
    id TEXT PRIMARY KEY,
    parser_id TEXT NOT NULL REFERENCES parser_lab_parsers(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_size INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE(parser_id, file_path)
);

-- Scout files for backtest
CREATE TABLE IF NOT EXISTS scout_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    path TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime INTEGER NOT NULL,
    content_hash TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    tag TEXT,
    tag_source TEXT,
    rule_id TEXT,
    manual_plugin TEXT,
    error TEXT,
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    processed_at INTEGER,
    sentinel_job_id INTEGER,
    UNIQUE(source_id, path)
);
EOF
echo "Database created."
echo

# Setup test files
echo "Setting up test data..."
mkdir -p "$TEST_DIR/data"

# Create sample CSV file
cat > "$TEST_DIR/data/sample.csv" << 'EOF'
id,name,value,active
1,Alice,100,true
2,Bob,200,false
3,Charlie,300,true
EOF

# Create sample parser Python file
cat > "$TEST_DIR/test_parser.py" << 'PYEOF'
"""A simple test parser that transforms CSV data."""

def transform(df):
    """Transform the input DataFrame."""
    # Just return the dataframe as-is for testing
    return df
PYEOF

# Create a parser with a bug for testing error handling
cat > "$TEST_DIR/bad_parser.py" << 'PYEOF'
"""A parser with intentional issues."""

def transform(df):
    # This will fail because 'nonexistent' column doesn't exist
    return df.select(['nonexistent_column'])
PYEOF

# Create parser with syntax error
cat > "$TEST_DIR/syntax_error.py" << 'PYEOF'
"""A parser with syntax errors."""

def transform(df)  # Missing colon
    return df
PYEOF

echo "Test data created."
echo

# Test 1: List parsers (initially empty)
echo "Test 1: List parsers (initially empty)"
OUTPUT=$($BINARY parser ls 2>&1)
if echo "$OUTPUT" | grep -q "No parsers found"; then
    echo "PASS: Empty parser list message shown"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected 'No parsers found' message"
    exit 1
fi
echo

# Test 2: Test a parser against a file (no database required)
echo "Test 2: Test parser against sample file"
# This test may fail if polars/pandas not installed, that's OK
OUTPUT=$($BINARY parser test "$TEST_DIR/test_parser.py" --input "$TEST_DIR/data/sample.csv" 2>&1) || true
if echo "$OUTPUT" | grep -q "Parser Test Results"; then
    echo "PASS: Parser test output shown"
else
    echo "Output was: $OUTPUT"
    # Don't fail if polars/pandas not installed
    if echo "$OUTPUT" | grep -q "polars\|pandas\|ModuleNotFoundError"; then
        echo "SKIP: polars/pandas not installed (expected in CI)"
    else
        echo "FAIL: Expected 'Parser Test Results' in output"
        exit 1
    fi
fi
echo

# Test 3: Test parser with JSON output
echo "Test 3: Test parser with JSON output"
OUTPUT=$($BINARY parser test "$TEST_DIR/test_parser.py" --input "$TEST_DIR/data/sample.csv" --json 2>&1) || true
# Check if it's valid JSON (even if it shows an error)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    # If not JSON, might be an error message about missing polars/pandas
    if echo "$OUTPUT" | grep -q "polars\|pandas\|ModuleNotFoundError"; then
        echo "SKIP: polars/pandas not installed (expected in CI)"
    else
        echo "FAIL: Invalid JSON output"
        echo "Output was: $OUTPUT"
        exit 1
    fi
fi
echo

# Test 4: Publish a parser
echo "Test 4: Publish a parser"
OUTPUT=$($BINARY parser publish "$TEST_DIR/test_parser.py" --topic test_topic 2>&1)
if echo "$OUTPUT" | grep -q "Published parser\|Updated parser"; then
    echo "PASS: Parser published successfully"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected parser publish confirmation"
    exit 1
fi
echo

# Test 5: List parsers (should show one now)
echo "Test 5: List parsers (should show one)"
OUTPUT=$($BINARY parser ls 2>&1)
if echo "$OUTPUT" | grep -q "test_parser"; then
    echo "PASS: Published parser listed"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected 'test_parser' in list"
    exit 1
fi
echo

# Test 6: List parsers with JSON output
echo "Test 6: List parsers JSON output"
OUTPUT=$($BINARY parser ls --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; data=json.load(sys.stdin); assert len(data) >= 1" 2>/dev/null; then
    echo "PASS: Valid JSON with at least one parser"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Invalid JSON or no parsers"
    exit 1
fi
echo

# Test 7: Show parser details
echo "Test 7: Show parser details"
OUTPUT=$($BINARY parser show test_parser 2>&1)
if echo "$OUTPUT" | grep -q "Parser: test_parser" && echo "$OUTPUT" | grep -q "test_topic"; then
    echo "PASS: Parser details shown"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected parser details"
    exit 1
fi
echo

# Test 8: Show parser with JSON output
echo "Test 8: Show parser JSON output"
OUTPUT=$($BINARY parser show test_parser --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; data=json.load(sys.stdin); assert data['name']=='test_parser'" 2>/dev/null; then
    echo "PASS: Valid JSON parser details"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Invalid JSON or wrong parser"
    exit 1
fi
echo

# Test 9: Publish a parser with custom name
echo "Test 9: Publish parser with custom name"
OUTPUT=$($BINARY parser publish "$TEST_DIR/test_parser.py" --topic custom_topic --name my_custom_parser 2>&1)
if echo "$OUTPUT" | grep -q "Published parser 'my_custom_parser'\|Updated parser 'my_custom_parser'"; then
    echo "PASS: Parser published with custom name"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected parser publish with custom name"
    exit 1
fi
echo

# Test 10: Backtest with no files for topic (should show helpful message)
echo "Test 10: Backtest with no files"
OUTPUT=$($BINARY parser backtest test_parser 2>&1)
if echo "$OUTPUT" | grep -q "No files found for topic"; then
    echo "PASS: Helpful message for empty topic"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected 'No files found' message"
    exit 1
fi
echo

# Test 11: Add some tagged files and run backtest
echo "Test 11: Add tagged files and backtest"
# Insert test files into scout_files with the parser's topic
NOW=$(date +%s)000
duckdb "$DB_FILE" <<EOF
INSERT INTO scout_files (source_id, path, rel_path, size, mtime, status, tag, first_seen_at, last_seen_at)
VALUES ('test-source', '$TEST_DIR/data/sample.csv', 'sample.csv', 100, $NOW, 'tagged', 'test_topic', $NOW, $NOW);
EOF

OUTPUT=$($BINARY parser backtest test_parser 2>&1) || true
# Check for progress or results (even if parser fails due to missing polars/pandas)
if echo "$OUTPUT" | grep -q "Testing\|RESULTS\|files"; then
    echo "PASS: Backtest attempted"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected backtest output"
    exit 1
fi
echo

# Test 12: Backtest with JSON output
echo "Test 12: Backtest with JSON output"
OUTPUT=$($BINARY parser backtest test_parser --json 2>&1) || true
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON backtest output"
else
    # May fail if no data library installed
    if echo "$OUTPUT" | grep -q "No files\|polars\|pandas"; then
        echo "SKIP: No files or data library not installed"
    else
        echo "Output was: $OUTPUT"
        echo "FAIL: Invalid JSON backtest output"
        exit 1
    fi
fi
echo

# Test 13: Unpublish a parser
echo "Test 13: Unpublish a parser"
OUTPUT=$($BINARY parser unpublish my_custom_parser 2>&1)
if echo "$OUTPUT" | grep -q "Unpublished parser: my_custom_parser"; then
    echo "PASS: Parser unpublished"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected unpublish confirmation"
    exit 1
fi
echo

# Test 14: Verify unpublished parser is gone
echo "Test 14: Verify parser is removed"
OUTPUT=$($BINARY parser show my_custom_parser 2>&1) || true
if echo "$OUTPUT" | grep -q "not found\|ERROR"; then
    echo "PASS: Unpublished parser not found"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected parser not found error"
    exit 1
fi
echo

# Test 15: Error handling - test missing parser file
echo "Test 15: Error handling - missing parser file"
OUTPUT=$($BINARY parser test "$TEST_DIR/nonexistent.py" --input "$TEST_DIR/data/sample.csv" 2>&1) || true
if echo "$OUTPUT" | grep -q "not found\|ERROR\|TRY"; then
    echo "PASS: Helpful error for missing parser"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected helpful error"
    exit 1
fi
echo

# Test 16: Error handling - test missing input file
echo "Test 16: Error handling - missing input file"
OUTPUT=$($BINARY parser test "$TEST_DIR/test_parser.py" --input "$TEST_DIR/nonexistent.csv" 2>&1) || true
if echo "$OUTPUT" | grep -q "not found\|ERROR\|TRY"; then
    echo "PASS: Helpful error for missing input"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected helpful error"
    exit 1
fi
echo

# Test 17: Error handling - publish non-Python file
echo "Test 17: Error handling - non-Python file"
echo "not python" > "$TEST_DIR/notpython.txt"
OUTPUT=$($BINARY parser publish "$TEST_DIR/notpython.txt" --topic test 2>&1) || true
if echo "$OUTPUT" | grep -q "Python file\|.py\|ERROR"; then
    echo "PASS: Helpful error for non-Python file"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected error for non-Python file"
    exit 1
fi
echo

# Test 18: Error handling - parser with syntax error
echo "Test 18: Error handling - parser with syntax error"
OUTPUT=$($BINARY parser publish "$TEST_DIR/syntax_error.py" --topic test 2>&1) || true
if echo "$OUTPUT" | grep -q "syntax\|ERROR\|invalid"; then
    echo "PASS: Syntax error detected"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected syntax error detection"
    exit 1
fi
echo

# Test 19: Error handling - show nonexistent parser
echo "Test 19: Error handling - show nonexistent parser"
OUTPUT=$($BINARY parser show nonexistent_parser 2>&1) || true
if echo "$OUTPUT" | grep -q "not found\|ERROR\|TRY"; then
    echo "PASS: Helpful error for missing parser"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected helpful error"
    exit 1
fi
echo

# Test 20: Error handling - unpublish nonexistent parser
echo "Test 20: Error handling - unpublish nonexistent parser"
OUTPUT=$($BINARY parser unpublish nonexistent_parser 2>&1) || true
if echo "$OUTPUT" | grep -q "not found\|ERROR\|TRY"; then
    echo "PASS: Helpful error for missing parser"
else
    echo "Output was: $OUTPUT"
    echo "FAIL: Expected helpful error"
    exit 1
fi
echo

echo "==================================="
echo "All parser command tests passed!"
echo "==================================="
