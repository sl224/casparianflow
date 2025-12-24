#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Phase 4: End-to-End Test ===${NC}\n"

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEST_DIR="$SCRIPT_DIR"
OUTPUT_DIR="$TEST_DIR/output"
DB_PATH="$TEST_DIR/test.db"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    pkill -f "casparian-sentinel" || true
    pkill -f "casparian-worker" || true
    rm -f "$DB_PATH"
    rm -rf "$OUTPUT_DIR"
}

# Setup cleanup trap
trap cleanup EXIT

# Step 1: Build binaries
echo -e "${YELLOW}Step 1: Building binaries...${NC}"
cd "$PROJECT_ROOT"
cargo build --release --package casparian_sentinel --package casparian_worker
echo -e "${GREEN}✓ Binaries built${NC}\n"

# Step 2: Setup test database
echo -e "${YELLOW}Step 2: Creating test database...${NC}"
rm -f "$DB_PATH"
sqlite3 "$DB_PATH" < "$TEST_DIR/schema.sql"
echo -e "${GREEN}✓ Database created${NC}\n"

# Step 3: Create test data directory and input file
echo -e "${YELLOW}Step 3: Creating test data...${NC}"
TEST_DATA_DIR="/tmp/casparian_test_data"
rm -rf "$TEST_DATA_DIR"
mkdir -p "$TEST_DATA_DIR"
echo "id,value" > "$TEST_DATA_DIR/test_input.csv"
echo "1,10.5" >> "$TEST_DATA_DIR/test_input.csv"
echo "2,20.3" >> "$TEST_DATA_DIR/test_input.csv"
echo "3,30.1" >> "$TEST_DATA_DIR/test_input.csv"
echo -e "${GREEN}✓ Test data created${NC}\n"

# Step 4: Create output directory
echo -e "${YELLOW}Step 4: Creating output directory...${NC}"
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"
echo -e "${GREEN}✓ Output directory ready${NC}\n"

# Step 5: Link venv (uv-provisioned .venv -> expected path)
echo -e "${YELLOW}Step 5: Linking environment...${NC}"
VENV_DIR="$HOME/.casparian_flow/venvs/test_env_hash_123"
mkdir -p "$(dirname "$VENV_DIR")"
rm -rf "$VENV_DIR"
# Symlink entire venv directory (provisioned by uv, has pyarrow)
ln -s "$PROJECT_ROOT/.venv" "$VENV_DIR"
echo -e "${GREEN}✓ Environment linked: $VENV_DIR -> .venv${NC}\n"

# Step 6: Start Sentinel
echo -e "${YELLOW}Step 6: Starting Sentinel...${NC}"
"$PROJECT_ROOT/target/release/casparian-sentinel" \
    --bind "tcp://127.0.0.1:15557" \
    --database "sqlite://$DB_PATH" \
    > "$TEST_DIR/sentinel.log" 2>&1 &
SENTINEL_PID=$!
sleep 1

if ! kill -0 $SENTINEL_PID 2>/dev/null; then
    echo -e "${RED}✗ Sentinel failed to start${NC}"
    cat "$TEST_DIR/sentinel.log"
    exit 1
fi
echo -e "${GREEN}✓ Sentinel started (PID: $SENTINEL_PID)${NC}\n"

# Step 7: Start Worker
echo -e "${YELLOW}Step 7: Starting Worker...${NC}"
"$PROJECT_ROOT/target/release/casparian-worker" \
    --connect "tcp://127.0.0.1:15557" \
    --output "$OUTPUT_DIR" \
    > "$TEST_DIR/worker.log" 2>&1 &
WORKER_PID=$!
sleep 2

if ! kill -0 $WORKER_PID 2>/dev/null; then
    echo -e "${RED}✗ Worker failed to start${NC}"
    cat "$TEST_DIR/worker.log"
    exit 1
fi
echo -e "${GREEN}✓ Worker started (PID: $WORKER_PID)${NC}\n"

# Step 8: Wait for job processing
echo -e "${YELLOW}Step 8: Waiting for job to be processed (max 10 seconds)...${NC}"
for i in {1..20}; do
    STATUS=$(sqlite3 "$DB_PATH" "SELECT status FROM cf_processing_queue WHERE id = 1" 2>/dev/null || echo "ERROR")

    if [ "$STATUS" = "COMPLETED" ]; then
        echo -e "${GREEN}✓ Job completed successfully!${NC}\n"
        break
    elif [ "$STATUS" = "FAILED" ]; then
        echo -e "${RED}✗ Job failed${NC}"
        sqlite3 "$DB_PATH" "SELECT error_message FROM cf_processing_queue WHERE id = 1"
        exit 1
    fi

    echo -n "."
    sleep 0.5
done
echo ""

# Step 9: Verify results
echo -e "${YELLOW}Step 9: Verifying results...${NC}"

# Check final job status
FINAL_STATUS=$(sqlite3 "$DB_PATH" "SELECT status FROM cf_processing_queue WHERE id = 1")
echo "  Job status: $FINAL_STATUS"

if [ "$FINAL_STATUS" != "COMPLETED" ]; then
    echo -e "${RED}✗ Job did not complete (status: $FINAL_STATUS)${NC}"
    echo -e "\n${YELLOW}Sentinel log:${NC}"
    tail -20 "$TEST_DIR/sentinel.log"
    echo -e "\n${YELLOW}Worker log:${NC}"
    tail -20 "$TEST_DIR/worker.log"
    exit 1
fi

# Check if output file exists
if [ ! -f "$OUTPUT_DIR"/*.parquet ]; then
    echo -e "${RED}✗ No parquet output file found${NC}"
    ls -la "$OUTPUT_DIR"
    exit 1
fi

echo -e "${GREEN}✓ Parquet file created: $(ls $OUTPUT_DIR/*.parquet)${NC}"

# Show final stats
echo -e "\n${YELLOW}=== Test Results ===${NC}"
echo "Job ID: 1"
echo "Status: $FINAL_STATUS"
echo "Plugin: test_plugin"
sqlite3 "$DB_PATH" "SELECT result_summary FROM cf_processing_queue WHERE id = 1" | sed 's/^/Summary: /'
echo "Output: $(ls $OUTPUT_DIR/*.parquet | xargs basename)"

echo -e "\n${GREEN}=== ✓ End-to-End Test PASSED ===${NC}"
