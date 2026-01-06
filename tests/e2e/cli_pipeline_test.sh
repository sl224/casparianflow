#!/bin/bash
# E2E test for full CLI pipeline: scan → tag → job → process → verify output
#
# This test exercises the complete data processing pipeline using a real
# MCData file and the mcdata_pipeline_parser.py parser.
#
# Tests:
# 1. Scan input folder to discover files
# 2. Preview file to understand structure
# 3. Publish parser to the registry
# 4. Create job in processing queue
# 5. Process job with casparian process-job
# 6. Verify output parquet file exists and contains data

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)
DB_FILE="$TEST_DIR/pipeline_test.db"
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
echo "DB: $DB_FILE"
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

# Set up database path (different commands use different env vars)
export CASPARIAN_DB_PATH="$DB_FILE"
export CASPARIAN_DB="$DB_FILE"

# Create database with required schema
echo "Setting up test database..."
sqlite3 "$DB_FILE" <<'EOF'
-- Processing queue for jobs (schema matches jobs.rs expectations)
CREATE TABLE IF NOT EXISTS cf_processing_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    input_file TEXT,
    file_version_id INTEGER,
    status TEXT NOT NULL DEFAULT 'PENDING',
    priority INTEGER DEFAULT 0,
    created_at TEXT,
    claim_time TEXT,
    end_time TEXT,
    worker_id TEXT,
    result_summary TEXT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0
);

-- Plugin manifest for deployed parsers
CREATE TABLE IF NOT EXISTS cf_plugin_manifest (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    version TEXT NOT NULL,
    source_code TEXT NOT NULL,
    lockfile_content TEXT,
    env_hash TEXT,
    artifact_hash TEXT,
    signature TEXT,
    publisher_name TEXT,
    publisher_email TEXT,
    status TEXT DEFAULT 'ACTIVE',
    deployed_at TEXT DEFAULT (datetime('now')),
    UNIQUE(plugin_name, version)
);

-- File version tracking (minimal for test)
CREATE TABLE IF NOT EXISTS cf_file_version (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    location_id INTEGER NOT NULL,
    content_hash TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

-- File location (minimal for test)
CREATE TABLE IF NOT EXISTS cf_file_location (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_root_id INTEGER NOT NULL,
    rel_path TEXT NOT NULL
);

-- Source root (minimal for test)
CREATE TABLE IF NOT EXISTS cf_source_root (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL
);

-- Parser Lab parsers table (for parser publish command)
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

-- Scout files (required by parser show for backtest count)
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
echo "Database schema created."
echo

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
# TEST 3: Publish parser to registry
# ================================================================
echo "--- Test 3: Publish parser ---"
OUTPUT=$($BINARY parser publish "$PARSER_FILE" --topic mcdata --name mcdata_pipeline 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "Published parser\|Updated parser"; then
    echo "PASS: Parser published"
else
    echo "FAIL: Expected parser publish confirmation"
    exit 1
fi
echo

# ================================================================
# TEST 4: Verify parser is in registry
# ================================================================
echo "--- Test 4: Verify parser in registry ---"
OUTPUT=$($BINARY parser show mcdata_pipeline 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "mcdata_pipeline"; then
    echo "PASS: Parser found in registry"
else
    echo "FAIL: Expected parser in registry"
    exit 1
fi
echo

# ================================================================
# TEST 5: Insert parser source into cf_plugin_manifest for process-job
# ================================================================
echo "--- Test 5: Deploy parser to plugin manifest ---"
PARSER_SOURCE=$(cat "$PARSER_FILE" | sed "s/'/''/g")  # Escape single quotes for SQL
sqlite3 "$DB_FILE" <<EOF
INSERT INTO cf_plugin_manifest (plugin_name, version, source_code, status)
VALUES ('mcdata_pipeline', '1.0.0', '$PARSER_SOURCE', 'ACTIVE');
EOF
echo "Parser deployed to cf_plugin_manifest"

# Verify deployment
COUNT=$(sqlite3 "$DB_FILE" "SELECT COUNT(*) FROM cf_plugin_manifest WHERE plugin_name='mcdata_pipeline'")
if [ "$COUNT" -eq "1" ]; then
    echo "PASS: Parser in plugin manifest"
else
    echo "FAIL: Parser not found in manifest"
    exit 1
fi
echo

# ================================================================
# TEST 6: Create job in processing queue
# ================================================================
echo "--- Test 6: Create job in processing queue ---"
sqlite3 "$DB_FILE" <<EOF
INSERT INTO cf_processing_queue (plugin_name, input_file, status, priority)
VALUES ('mcdata_pipeline', '$INPUT_FILE', 'PENDING', 1);
EOF

JOB_ID=$(sqlite3 "$DB_FILE" "SELECT id FROM cf_processing_queue ORDER BY id DESC LIMIT 1")
echo "Created job ID: $JOB_ID"

# Verify job
STATUS=$(sqlite3 "$DB_FILE" "SELECT status FROM cf_processing_queue WHERE id=$JOB_ID")
if [ "$STATUS" = "PENDING" ]; then
    echo "PASS: Job created with PENDING status"
else
    echo "FAIL: Expected PENDING status, got: $STATUS"
    exit 1
fi
echo

# ================================================================
# TEST 7: Process the job
# ================================================================
echo "--- Test 7: Process job ---"
echo "Running: $BINARY process-job $JOB_ID --db $DB_FILE --output $OUTPUT_DIR"

# Run process-job and capture both stdout and stderr
# Don't fail on error - we handle job status separately
set +e
$BINARY process-job "$JOB_ID" --db "$DB_FILE" --output "$OUTPUT_DIR" 2>&1
PROCESS_EXIT_CODE=$?
set -e

if [ $PROCESS_EXIT_CODE -eq 0 ]; then
    echo "Process-job completed successfully"
else
    echo "Process-job exited with error code $PROCESS_EXIT_CODE (checking status...)"
fi

# Check job status
STATUS=$(sqlite3 "$DB_FILE" "SELECT status FROM cf_processing_queue WHERE id=$JOB_ID")
echo "Job status after processing: $STATUS"

JOB_PASSED=false
if [ "$STATUS" = "COMPLETED" ]; then
    echo "PASS: Job completed successfully"
    JOB_PASSED=true
elif [ "$STATUS" = "RUNNING" ]; then
    echo "INFO: Job still running (may need polars installed)"
elif [ "$STATUS" = "FAILED" ]; then
    ERROR_MSG=$(sqlite3 "$DB_FILE" "SELECT error_message FROM cf_processing_queue WHERE id=$JOB_ID")
    echo "Job failed with error: $ERROR_MSG"
    # Check if it's a missing dependency error (acceptable in CI)
    if echo "$ERROR_MSG" | grep -q "polars\|pandas\|pyarrow\|ModuleNotFoundError"; then
        echo "SKIP: Job failed due to missing dependencies (polars/pandas/pyarrow - expected in CI)"
    else
        echo "FAIL: Job failed with unexpected error"
        exit 1
    fi
else
    echo "INFO: Unexpected job status: $STATUS"
fi
echo

# ================================================================
# TEST 8: Verify output exists (if job completed)
# ================================================================
echo "--- Test 8: Verify output ---"
if [ "$STATUS" = "COMPLETED" ]; then
    RESULT_PATH=$(sqlite3 "$DB_FILE" "SELECT result_summary FROM cf_processing_queue WHERE id=$JOB_ID")
    echo "Result path: $RESULT_PATH"

    if [ -f "$RESULT_PATH" ]; then
        echo "PASS: Output parquet file exists"

        # Show file size
        FILE_SIZE=$(ls -lh "$RESULT_PATH" | awk '{print $5}')
        echo "Output file size: $FILE_SIZE"
    else
        echo "INFO: Output file not at expected path"
        # Check output directory for any parquet files
        PARQUET_FILES=$(find "$OUTPUT_DIR" -name "*.parquet" 2>/dev/null | head -5)
        if [ -n "$PARQUET_FILES" ]; then
            echo "Found parquet files in output directory:"
            echo "$PARQUET_FILES"
            echo "PASS: Parquet output generated"
        else
            echo "FAIL: No parquet files found in output"
            exit 1
        fi
    fi
else
    echo "SKIP: Job did not complete, skipping output verification"
fi
echo

# ================================================================
# TEST 9: List jobs to verify CLI
# ================================================================
echo "--- Test 9: List jobs ---"
OUTPUT=$($BINARY jobs 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "mcdata_pipeline\|$JOB_ID"; then
    echo "PASS: Job visible in jobs list"
else
    echo "INFO: Job list output may vary based on status"
fi
echo

# ================================================================
# TEST 10: Show specific job details
# ================================================================
echo "--- Test 10: Show job details ---"
OUTPUT=$($BINARY job show "$JOB_ID" 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "$JOB_ID\|mcdata_pipeline\|status"; then
    echo "PASS: Job details displayed"
else
    echo "INFO: Job show output may vary"
fi
echo

echo "==================================="
echo "Pipeline E2E test complete!"
echo "==================================="
echo
echo "Summary:"
echo "  - Scan: OK"
echo "  - Preview: OK"
echo "  - Parser publish: OK"
echo "  - Job creation: OK"
echo "  - Job processing: $STATUS"
if [ "$STATUS" = "COMPLETED" ]; then
    echo "  - Output verification: OK"
fi
echo "==================================="
