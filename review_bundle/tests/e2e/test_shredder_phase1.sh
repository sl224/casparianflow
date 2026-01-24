#!/bin/bash
# Phase 1 E2E Test: Shredder CLI
#
# Tests:
# 1. shred analyze - format detection
# 2. shred run - basic shredding
# 3. shred run with freezer - rare types go to _MISC
# 4. Lineage file verification
# 5. Header cloning

set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CASPARIAN="$PROJECT_ROOT/target/release/casparian"
TEST_DIR="/tmp/shredder_e2e_$$"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

pass() { echo -e "${GREEN}PASS${NC}: $1"; }
fail() { echo -e "${RED}FAIL${NC}: $1"; exit 1; }

# Setup
mkdir -p "$TEST_DIR"
trap "rm -rf $TEST_DIR" EXIT

echo "================================================"
echo "  Phase 1 E2E: Shredder CLI Tests"
echo "================================================"
echo ""

# Build if needed
if [ ! -f "$CASPARIAN" ]; then
    echo "Building casparian..."
    cd "$PROJECT_ROOT"
    cargo build --release -p casparian
fi

# ============================================
# Test 1: Analyze CSV file
# ============================================
echo "Test 1: Analyze CSV file with header"

cat > "$TEST_DIR/input1.csv" << 'EOF'
timestamp,message_type,value,status
2024-01-01,TYPE_A,10.5,OK
2024-01-01,TYPE_B,20.5,WARN
2024-01-01,TYPE_A,11.5,OK
EOF

OUTPUT=$("$CASPARIAN" shred analyze "$TEST_DIR/input1.csv" 2>&1)

if echo "$OUTPUT" | grep -q "Confidence: High"; then
    pass "Detected CSV with High confidence"
else
    fail "Did not detect CSV format correctly"
fi

if echo "$OUTPUT" | grep -q "CSV Column"; then
    pass "Detected CSV Column strategy"
else
    fail "Did not detect CSV Column strategy"
fi

# ============================================
# Test 2: Analyze JSON Lines file
# ============================================
echo ""
echo "Test 2: Analyze JSON Lines file"

cat > "$TEST_DIR/input2.jsonl" << 'EOF'
{"timestamp":"2024-01-01","event":"login","user_id":1}
{"timestamp":"2024-01-01","event":"logout","user_id":1}
{"timestamp":"2024-01-01","event":"action","user_id":2}
{"timestamp":"2024-01-01","event":"login","user_id":2}
EOF

OUTPUT=$("$CASPARIAN" shred analyze "$TEST_DIR/input2.jsonl" 2>&1)

if echo "$OUTPUT" | grep -q "JSON Key"; then
    pass "Detected JSON Key strategy"
else
    fail "Did not detect JSON format"
fi

# ============================================
# Test 3: Basic shred operation
# ============================================
echo ""
echo "Test 3: Basic shred with CSV"

cat > "$TEST_DIR/multi.csv" << 'EOF'
timestamp,msg_type,value
2024-01-01,A,1
2024-01-01,B,2
2024-01-01,A,3
2024-01-01,C,4
2024-01-01,B,5
2024-01-01,A,6
EOF

rm -rf "$TEST_DIR/output3"
"$CASPARIAN" shred run "$TEST_DIR/multi.csv" --column 1 --output "$TEST_DIR/output3" 2>&1

# Check shards created
if [ -f "$TEST_DIR/output3/A.csv" ]; then
    pass "Shard A.csv created"
else
    fail "Shard A.csv not created"
fi

if [ -f "$TEST_DIR/output3/B.csv" ]; then
    pass "Shard B.csv created"
else
    fail "Shard B.csv not created"
fi

if [ -f "$TEST_DIR/output3/C.csv" ]; then
    pass "Shard C.csv created"
else
    fail "Shard C.csv not created"
fi

# Verify row counts
A_ROWS=$(wc -l < "$TEST_DIR/output3/A.csv" | tr -d ' ')
B_ROWS=$(wc -l < "$TEST_DIR/output3/B.csv" | tr -d ' ')

if [ "$A_ROWS" = "4" ]; then  # 1 header + 3 data rows
    pass "A.csv has correct row count (4)"
else
    fail "A.csv has wrong row count: $A_ROWS (expected 4)"
fi

if [ "$B_ROWS" = "3" ]; then  # 1 header + 2 data rows
    pass "B.csv has correct row count (3)"
else
    fail "B.csv has wrong row count: $B_ROWS (expected 3)"
fi

# ============================================
# Test 4: Header cloning
# ============================================
echo ""
echo "Test 4: Header cloning"

FIRST_LINE=$(head -1 "$TEST_DIR/output3/A.csv")
if [ "$FIRST_LINE" = "timestamp,msg_type,value" ]; then
    pass "Header cloned correctly to A.csv"
else
    fail "Header not cloned correctly: $FIRST_LINE"
fi

FIRST_LINE_B=$(head -1 "$TEST_DIR/output3/B.csv")
if [ "$FIRST_LINE_B" = "timestamp,msg_type,value" ]; then
    pass "Header cloned correctly to B.csv"
else
    fail "Header not cloned correctly in B.csv"
fi

# ============================================
# Test 5: Lineage file
# ============================================
echo ""
echo "Test 5: Lineage file created"

if [ -f "$TEST_DIR/output3/lineage.idx" ]; then
    pass "Lineage index file created"
    LINEAGE_LINES=$(wc -l < "$TEST_DIR/output3/lineage.idx" | tr -d ' ')
    if [ "$LINEAGE_LINES" -gt "0" ]; then
        pass "Lineage file has content ($LINEAGE_LINES blocks)"
    else
        fail "Lineage file is empty"
    fi
else
    fail "Lineage index file not created"
fi

# ============================================
# Test 6: Freezer for rare types
# ============================================
echo ""
echo "Test 6: Freezer for rare types (top_n=2)"

cat > "$TEST_DIR/many_types.csv" << 'EOF'
ts,type,val
1,COMMON_A,1
2,COMMON_A,2
3,COMMON_A,3
4,COMMON_B,4
5,COMMON_B,5
6,RARE_1,6
7,RARE_2,7
8,RARE_3,8
EOF

rm -rf "$TEST_DIR/output6"
"$CASPARIAN" shred run "$TEST_DIR/many_types.csv" --column 1 --top-n 2 --output "$TEST_DIR/output6" 2>&1

# Top 2 (COMMON_A, COMMON_B) should get dedicated files
if [ -f "$TEST_DIR/output6/COMMON_A.csv" ]; then
    pass "COMMON_A.csv created (top type)"
else
    fail "COMMON_A.csv not created"
fi

if [ -f "$TEST_DIR/output6/COMMON_B.csv" ]; then
    pass "COMMON_B.csv created (top type)"
else
    fail "COMMON_B.csv not created"
fi

# Rare types should go to _MISC
if [ -f "$TEST_DIR/output6/_MISC.csv" ]; then
    pass "_MISC.csv freezer created"
    MISC_LINES=$(wc -l < "$TEST_DIR/output6/_MISC.csv" | tr -d ' ')
    if [ "$MISC_LINES" = "4" ]; then  # 1 header + 3 rare types
        pass "_MISC.csv has 4 lines (1 header + 3 rare types)"
    else
        fail "_MISC.csv has wrong line count: $MISC_LINES (expected 4)"
    fi
else
    fail "_MISC.csv freezer not created"
fi

# ============================================
# Test 7: Tab-delimited file
# ============================================
echo ""
echo "Test 7: Tab-delimited file"

printf 'ts\ttype\tval\n1\tX\t10\n2\tY\t20\n3\tX\t30\n' > "$TEST_DIR/tabbed.tsv"

rm -rf "$TEST_DIR/output7"
"$CASPARIAN" shred run "$TEST_DIR/tabbed.tsv" --column 1 --delimiter 'tab' --output "$TEST_DIR/output7" 2>&1

if [ -f "$TEST_DIR/output7/X.csv" ]; then
    pass "Tab-delimited file shredded correctly"
else
    fail "Tab-delimited file not shredded"
fi

# ============================================
# Test 8: JSON output format
# ============================================
echo ""
echo "Test 8: JSON output format"

OUTPUT=$("$CASPARIAN" shred analyze "$TEST_DIR/input1.csv" --format json 2>&1)

if echo "$OUTPUT" | grep -q '"strategy":'; then
    pass "JSON output format works"
else
    fail "JSON output format failed"
fi

# ============================================
echo ""
echo "================================================"
echo "  ALL PHASE 1 TESTS PASSED!"
echo "================================================"
