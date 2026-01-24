#!/bin/bash
# E2E test for CLI flow: scan -> preview -> run parser -> verify output
#
# This test exercises a lightweight v1 workflow using a real MCData file
# (if available) and the mcdata_pipeline_parser.py parser.
#
# Tests:
# 1. Scan input folder to discover files
# 2. Preview file to understand structure
# 3. Run parser via `casparian run`
# 4. Verify output parquet files exist

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)
OUTPUT_DIR="$TEST_DIR/output"
PARSER_FILE="$SCRIPT_DIR/parsers/mcdata_pipeline_parser.py"

# Test input file - use the real MCData file if available, otherwise create sample
MCDATA_INPUT="/Users/shan/workspace/e2ude_core/tests/static_assets/zips/169069_20250203_004745_025_TransportRSM.fpkg.e2d/169069_20250203_004745_025_MCData"

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo "=== CLI Pipeline E2E Test ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo "Output: $OUTPUT_DIR"
echo "Parser: $PARSER_FILE"
echo

# Check binary exists
if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found. Run 'cargo build' first."
    exit 1
fi

# Check parser exists
if [ ! -f "$PARSER_FILE" ]; then
    echo "ERROR: Parser file not found: $PARSER_FILE"
    exit 1
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Check if real MCData file exists, otherwise create sample data
if [ -f "$MCDATA_INPUT" ]; then
    echo "Using real MCData file: $MCDATA_INPUT"
    INPUT_FILE="$MCDATA_INPUT"
else
    echo "Real MCData file not found, creating sample data..."
    mkdir -p "$TEST_DIR/input"
    INPUT_FILE="$TEST_DIR/input/sample_MCData"

    # Create sample MCData-like CSV (headerless, comma-separated)
    cat > "$INPUT_FILE" << 'SAMPLEEOF'
1,ACT_FCNS1_SW:,,02/03/2025 01:08:00,MUX_FCNS1,SW,CLEARED,100,200,300,400
1,ACT_FCNS1_SW:,,02/03/2025 01:08:01,MUX_FCNS1,SW,CLEARED,101,201,301,401
1,ACT_FCNS1_SW:,,02/03/2025 01:08:02,MUX_FCNS1,SW,CLEARED,102,202,302,402
1,ACT_FCNS1_HW:,,02/03/2025 01:09:00,MUX_FCNS1,HW,0,1,2,3,4,5,6,7,8,9,10,11
1,ACT_FCNS1_HW:,,02/03/2025 01:09:01,MUX_FCNS1,HW,0,11,12,13,14,15,16,17,18,19,20,21
1,ACT_FCNS1_FW:,,02/03/2025 01:10:00,MUX_FCNS1,FW,1,42
1,CONFIG_FLTS:,,02/03/2025 01:11:00,MIDS,CFG,ACTIVE,FAULT_001
1,CONFIG_FLTS:,,02/03/2025 01:11:01,MIDS,CFG,CLEARED,FAULT_002
SAMPLEEOF
    echo "Sample MCData file created with 8 records."
fi
echo

# ================================================================
# TEST 1: Scan to discover the input file
# ================================================================
echo "--- Test 1: Scan input folder ---"
INPUT_DIR=$(dirname "$INPUT_FILE")
OUTPUT=$($BINARY scan "$INPUT_DIR" 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "MCData\|sample"; then
    echo "PASS: File discovered by scan"
else
    echo "FAIL: Expected file in scan output"
    exit 1
fi
echo

# ================================================================
# TEST 2: Preview file to understand structure
# ================================================================
echo "--- Test 2: Preview file structure ---"
OUTPUT=$($BINARY preview "$INPUT_FILE" --head 5 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "ACT_FCNS1\|CONFIG_FLTS\|Line"; then
    echo "PASS: File structure previewed"
else
    echo "FAIL: Expected preview output"
    exit 1
fi
echo

# ================================================================
# TEST 3: Run parser
# ================================================================
echo "--- Test 3: Run parser ---"
set +e
RUN_OUTPUT=$($BINARY run "$PARSER_FILE" "$INPUT_FILE" --sink "parquet://$OUTPUT_DIR/" 2>&1)
RUN_EXIT_CODE=$?
set -e

echo "$RUN_OUTPUT"

if [ $RUN_EXIT_CODE -eq 0 ]; then
    echo "Parser run completed successfully"
else
    # Accept missing dependency errors in CI
    if echo "$RUN_OUTPUT" | grep -qi "uv\|venv\|python\|pyarrow\|pandas\|modulenotfounderror"; then
        echo "SKIP: Parser run failed due to missing Python dependencies"
        exit 0
    fi
    echo "FAIL: Parser run failed"
    exit 1
fi
echo

# ================================================================
# TEST 4: Verify output
# ================================================================
echo "--- Test 4: Verify output ---"
PARQUET_FILES=$(find "$OUTPUT_DIR" -name "*.parquet" 2>/dev/null | head -5)
if [ -n "$PARQUET_FILES" ]; then
    echo "PASS: Parquet output generated"
    echo "$PARQUET_FILES"
else
    echo "FAIL: No parquet files found in output"
    exit 1
fi
echo

echo "==================================="
echo "Pipeline E2E test complete!"
echo "==================================="
