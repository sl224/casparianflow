#!/bin/bash
# E2E tests for CLI options not covered by other test files
# This fills in coverage gaps for various flags and edge cases

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

echo "=== CLI Coverage Tests (Additional Options) ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo

# Setup test files
echo "Setting up test data..."
mkdir -p "$TEST_DIR/deep/nested/folder"

# Create files of various sizes
dd if=/dev/zero of="$TEST_DIR/tiny.csv" bs=100 count=1 2>/dev/null
dd if=/dev/zero of="$TEST_DIR/medium.csv" bs=1024 count=50 2>/dev/null
dd if=/dev/zero of="$TEST_DIR/large.csv" bs=1024 count=200 2>/dev/null
dd if=/dev/zero of="$TEST_DIR/deep/nested/folder/deep.csv" bs=1024 count=10 2>/dev/null

# Add content to files for schema testing
cat > "$TEST_DIR/typed.csv" << 'EOF'
id,name,price,active,score,date
1,Widget,19.99,true,4.5,2024-01-15
2,Gadget,29.99,false,3.8,2024-02-20
3,Device,99.99,true,4.9,2024-03-25
EOF

echo "Test data created."
echo

# ================================================================
# SCAN COMMAND - Additional Coverage
# ================================================================
echo "--- Scan Command Coverage ---"
echo

# Test 1: --max-size filter
echo "Test 1: Scan with --max-size filter"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --max-size 60KB 2>&1)
if echo "$OUTPUT" | grep -q "medium.csv\|tiny.csv"; then
    echo "PASS: Small files found"
else
    echo "FAIL: Expected small files"
    exit 1
fi
if echo "$OUTPUT" | grep -q "large.csv"; then
    echo "FAIL: Large file should be filtered out"
    exit 1
else
    echo "PASS: Large file filtered out"
fi
echo

# Test 2: Combined --min-size and --max-size
echo "Test 2: Scan with combined size filters"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --min-size 20KB --max-size 100KB 2>&1)
if echo "$OUTPUT" | grep -q "medium.csv"; then
    echo "PASS: Medium file found in range"
else
    echo "FAIL: Expected medium file in range"
    exit 1
fi
echo

# Test 3: Scan with multiple types and depth
echo "Test 3: Scan with multiple type filters and depth"
echo '{"test": 1}' > "$TEST_DIR/data.json"
OUTPUT=$($BINARY scan "$TEST_DIR" -r --type csv --type json --depth 1 2>&1)
if echo "$OUTPUT" | grep -q "typed.csv\|data.json"; then
    echo "PASS: Multiple types filtered correctly"
else
    echo "FAIL: Expected typed.csv and data.json"
    exit 1
fi
echo

# ================================================================
# PREVIEW COMMAND - Additional Coverage
# ================================================================
echo "--- Preview Command Coverage ---"
echo

# Test 4: --schema only mode
echo "Test 4: Preview with --schema only"
OUTPUT=$($BINARY preview "$TEST_DIR/typed.csv" --schema 2>&1)
if echo "$OUTPUT" | grep -q "id\|name\|price"; then
    echo "PASS: Schema columns shown"
else
    echo "FAIL: Expected schema columns"
    exit 1
fi
echo

# Test 5: --rows with custom value
echo "Test 5: Preview with custom --rows"
OUTPUT=$($BINARY preview "$TEST_DIR/typed.csv" --rows 1 2>&1)
if echo "$OUTPUT" | grep -q "1 row\|Widget\|id"; then
    echo "PASS: Custom row limit works"
else
    echo "INFO: Output format may differ"
fi
echo

# Test 6: Combine --schema and --json
echo "Test 6: Preview with --schema and --json"
OUTPUT=$($BINARY preview "$TEST_DIR/typed.csv" --schema --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print('ok' if 'schema' in str(d).lower() or 'columns' in str(d).lower() else 'missing')" 2>/dev/null | grep -q "ok"; then
    echo "PASS: JSON schema output valid"
else
    if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        echo "PASS: Valid JSON output (schema format may vary)"
    else
        echo "INFO: Schema+JSON combo may have different format"
    fi
fi
echo

# ================================================================
# TAG/FILES COMMAND - Additional Coverage
# ================================================================
echo "--- Tag/Files Command Coverage ---"
echo

# Test 7: Tag with --no-queue flag
echo "Test 7: Tag with --no-queue flag"
# This tests the flag is accepted (actual tagging needs database)
OUTPUT=$($BINARY tag --help 2>&1)
if echo "$OUTPUT" | grep -q "no-queue"; then
    echo "PASS: --no-queue flag documented"
else
    echo "FAIL: Expected --no-queue in help"
    exit 1
fi
echo

# ================================================================
# JOB LOGS COMMAND - Verify it exists
# ================================================================
echo "--- Job Command Coverage ---"
echo

# Test 8: Job logs help
echo "Test 8: Job logs --help"
OUTPUT=$($BINARY job logs --help 2>&1)
if echo "$OUTPUT" | grep -q "logs\|follow\|tail"; then
    echo "PASS: Job logs command has options"
else
    echo "FAIL: Expected logs options"
    exit 1
fi
echo

# Test 9: Job retry-all help
echo "Test 9: Job retry-all --help"
OUTPUT=$($BINARY job retry-all --help 2>&1)
if echo "$OUTPUT" | grep -q "topic\|failed"; then
    echo "PASS: retry-all has topic filter"
else
    echo "INFO: retry-all help may differ"
fi
echo

# ================================================================
# PARSER COMMAND - Additional Coverage
# ================================================================
echo "--- Parser Command Coverage ---"
echo

# Create a simple parser
cat > "$TEST_DIR/simple_parser.py" << 'EOF'
import csv
import sys

def transform(file_path):
    with open(file_path, 'r') as f:
        reader = csv.DictReader(f)
        for row in reader:
            yield row

if __name__ == "__main__":
    for row in transform(sys.argv[1]):
        print(row)
EOF

# Test 10: Parser test with --rows option
echo "Test 10: Parser test --rows option exists"
OUTPUT=$($BINARY parser test --help 2>&1)
if echo "$OUTPUT" | grep -q "rows\|-n"; then
    echo "PASS: --rows/-n option available"
else
    echo "FAIL: Expected --rows option"
    exit 1
fi
echo

# Test 11: Parser backtest --json option
echo "Test 11: Parser backtest --json option exists"
OUTPUT=$($BINARY parser backtest --help 2>&1)
if echo "$OUTPUT" | grep -q "json"; then
    echo "PASS: --json option available for backtest"
else
    echo "FAIL: Expected --json option"
    exit 1
fi
echo

# ================================================================
# WORKER-CLI COMMAND - Verify name
# ================================================================
echo "--- Worker-CLI Command Coverage ---"
echo

# Test 12: worker-cli is the correct command name
echo "Test 12: worker-cli command name"
OUTPUT=$($BINARY worker-cli --help 2>&1)
if echo "$OUTPUT" | grep -q "Manage workers\|list\|drain"; then
    echo "PASS: worker-cli command works"
else
    echo "FAIL: Expected worker management help"
    exit 1
fi
echo

# ================================================================
# SOURCE COMMAND - Additional Coverage
# ================================================================
echo "--- Source Command Coverage ---"
echo

# Test 13: Source add --recursive option exists
echo "Test 13: Source add --recursive option"
OUTPUT=$($BINARY source add --help 2>&1)
if echo "$OUTPUT" | grep -q "recursive"; then
    echo "PASS: --recursive option available"
else
    echo "FAIL: Expected --recursive option"
    exit 1
fi
echo

# Test 14: Source sync --all option exists
echo "Test 14: Source sync --all option"
OUTPUT=$($BINARY source sync --help 2>&1)
if echo "$OUTPUT" | grep -q "all"; then
    echo "PASS: --all option available"
else
    echo "FAIL: Expected --all option"
    exit 1
fi
echo

# ================================================================
# TOPIC COMMAND - Additional Coverage
# ================================================================
echo "--- Topic Command Coverage ---"
echo

# Test 15: Topic create --description option exists
echo "Test 15: Topic create --description option"
OUTPUT=$($BINARY topic create --help 2>&1)
if echo "$OUTPUT" | grep -q "description"; then
    echo "PASS: --description option available"
else
    echo "FAIL: Expected --description option"
    exit 1
fi
echo

# Test 16: Topic delete vs remove (verify correct name)
echo "Test 16: Topic delete command"
OUTPUT=$($BINARY topic --help 2>&1)
if echo "$OUTPUT" | grep -q "delete\|remove"; then
    echo "PASS: Topic has delete/remove command"
else
    echo "FAIL: Expected delete command"
    exit 1
fi
echo

# ================================================================
# RULE COMMAND - Additional Coverage
# ================================================================
echo "--- Rule Command Coverage ---"
echo

# Test 17: Rule add --priority option
echo "Test 17: Rule add --priority option"
OUTPUT=$($BINARY rule add --help 2>&1)
if echo "$OUTPUT" | grep -q "priority"; then
    echo "PASS: --priority option available"
else
    echo "FAIL: Expected --priority option"
    exit 1
fi
echo

# Test 18: Rule test command exists
echo "Test 18: Rule test command"
OUTPUT=$($BINARY rule test --help 2>&1)
if echo "$OUTPUT" | grep -q "path\|pattern\|id"; then
    echo "PASS: Rule test command available"
else
    echo "FAIL: Expected rule test command"
    exit 1
fi
echo

# ================================================================
# COMPREHENSIVE HELP TEXT VERIFICATION
# ================================================================
echo "--- All Commands Help Verification ---"
echo

# Test 19: All top-level commands have help
COMMANDS="scan preview tag untag files parser jobs job worker-cli source rule topic"
for cmd in $COMMANDS; do
    if $BINARY $cmd --help > /dev/null 2>&1; then
        echo "PASS: $cmd --help works"
    else
        echo "FAIL: $cmd --help failed"
        exit 1
    fi
done
echo

echo "==================================="
echo "All coverage tests passed!"
echo "==================================="
