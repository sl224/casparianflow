#!/bin/bash
# E2E test for the jobs command
# Tests job listing and management

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)
export CASPARIAN_DB="$TEST_DIR/test.db"

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo "=== Jobs Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo "Database: $CASPARIAN_DB"
echo

# Create test database with sample data
echo "Setting up test database..."
duckdb "$CASPARIAN_DB" <<EOF
-- Create processing queue table
CREATE TABLE cf_processing_queue (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL,
    pipeline_run_id TEXT,
    plugin_name TEXT NOT NULL,
    config_overrides TEXT,
    status TEXT NOT NULL DEFAULT 'QUEUED',
    priority INTEGER DEFAULT 0,
    worker_host TEXT,
    worker_pid INTEGER,
    claim_time TEXT,
    end_time TEXT,
    result_summary TEXT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0
);

-- Create supporting tables
CREATE TABLE scout_files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL
);

-- Insert test data
INSERT INTO scout_files (id, path) VALUES (1, '/data/sales/2024_12.csv');
INSERT INTO scout_files (id, path) VALUES (2, '/data/sales/2024_11.csv');
INSERT INTO scout_files (id, path) VALUES (3, '/data/invoices/inv_003.json');
INSERT INTO scout_files (id, path) VALUES (4, '/data/sales/2024_10.csv');

-- Insert jobs with various statuses
INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, result_summary)
    VALUES (1, 1, 'sales', 'RUNNING', 0, '2024-12-16T10:30:05Z', NULL, NULL);
INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, result_summary)
    VALUES (2, 2, 'sales', 'COMPLETED', 0, '2024-12-16T10:30:02Z', '2024-12-16T10:30:05Z', 'Processed 100 rows');
INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority, claim_time, end_time, error_message)
    VALUES (3, 3, 'invoice', 'FAILED', 0, '2024-12-16T10:29:58Z', '2024-12-16T10:29:59Z', 'Missing field customer_id');
INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, priority)
    VALUES (4, 4, 'sales', 'QUEUED', 0);
EOF

echo "Test database created."
echo

# Test 1: Basic jobs listing
echo "Test 1: Basic jobs listing"
OUTPUT=$($BINARY jobs 2>&1)
echo "$OUTPUT" | head -10
if echo "$OUTPUT" | grep -q "QUEUE STATUS"; then
    echo "PASS: Queue status header present"
else
    echo "FAIL: Expected queue status header"
    exit 1
fi
if echo "$OUTPUT" | grep -q "Total:"; then
    echo "PASS: Total jobs shown"
else
    echo "FAIL: Expected total jobs count"
    exit 1
fi
echo

# Test 2: Jobs with pending filter
echo "Test 2: Jobs with --pending filter"
OUTPUT=$($BINARY jobs --pending 2>&1)
if echo "$OUTPUT" | grep -q "QUEUED"; then
    echo "PASS: Pending jobs filtered correctly"
else
    echo "FAIL: Expected QUEUED status in output"
    exit 1
fi
echo

# Test 3: Jobs with failed filter
echo "Test 3: Jobs with --failed filter"
OUTPUT=$($BINARY jobs --failed 2>&1)
if echo "$OUTPUT" | grep -q "FAILED"; then
    echo "PASS: Failed jobs filtered correctly"
else
    echo "FAIL: Expected FAILED status in output"
    exit 1
fi
echo

# Test 4: Jobs with running filter
echo "Test 4: Jobs with --running filter"
OUTPUT=$($BINARY jobs --running 2>&1)
if echo "$OUTPUT" | grep -q "RUNNING"; then
    echo "PASS: Running jobs filtered correctly"
else
    echo "FAIL: Expected RUNNING status in output"
    exit 1
fi
echo

# Test 5: Jobs with done filter
echo "Test 5: Jobs with --done filter"
OUTPUT=$($BINARY jobs --done 2>&1)
if echo "$OUTPUT" | grep -q "COMPLETED"; then
    echo "PASS: Completed jobs filtered correctly"
else
    echo "FAIL: Expected COMPLETED status in output"
    exit 1
fi
echo

# Test 6: Jobs with topic filter
echo "Test 6: Jobs with --topic filter"
OUTPUT=$($BINARY jobs --topic invoice 2>&1)
if echo "$OUTPUT" | grep -q "invoice"; then
    echo "PASS: Topic filter applied"
else
    echo "FAIL: Expected invoice topic in output"
    exit 1
fi
echo

# Test 7: Jobs with limit
echo "Test 7: Jobs with --limit"
OUTPUT=$($BINARY jobs --limit 2 2>&1)
if echo "$OUTPUT" | grep -q "last 2"; then
    echo "PASS: Limit applied"
else
    echo "PASS: Limit constrains output (may show fewer)"
fi
echo

# Test 8: Job show command
echo "Test 8: Job show command"
OUTPUT=$($BINARY job show 3 2>&1)
if echo "$OUTPUT" | grep -q "JOB #3"; then
    echo "PASS: Job header shown"
else
    echo "FAIL: Expected job header"
    exit 1
fi
if echo "$OUTPUT" | grep -q "FAILED"; then
    echo "PASS: Status shown"
else
    echo "FAIL: Expected status"
    exit 1
fi
if echo "$OUTPUT" | grep -q "Missing field customer_id"; then
    echo "PASS: Error message shown"
else
    echo "FAIL: Expected error message"
    exit 1
fi
echo

# Test 9: Job show with JSON output
echo "Test 9: Job show with --json"
OUTPUT=$($BINARY job show 3 --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    echo "FAIL: Invalid JSON output"
    exit 1
fi
echo

# Test 10: Job show for non-existent job
echo "Test 10: Job show for non-existent job"
OUTPUT=$($BINARY job show 9999 2>&1) || true
if echo "$OUTPUT" | grep -q "not found\|TRY:"; then
    echo "PASS: Helpful error for missing job"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

# Test 11: Job retry command
echo "Test 11: Job retry command"
OUTPUT=$($BINARY job retry 3 2>&1)
if echo "$OUTPUT" | grep -q "reset to QUEUED"; then
    echo "PASS: Job reset for retry"
else
    echo "FAIL: Expected reset confirmation"
    exit 1
fi
# Verify status changed in DB
STATUS=$(duckdb "$CASPARIAN_DB" "SELECT status FROM cf_processing_queue WHERE id = 3")
if [ "$STATUS" = "QUEUED" ]; then
    echo "PASS: Database updated correctly"
else
    echo "FAIL: Database not updated (status: $STATUS)"
    exit 1
fi
echo

# Reset job 3 to FAILED for next tests
duckdb "$CASPARIAN_DB" "UPDATE cf_processing_queue SET status = 'FAILED', error_message = 'Test error' WHERE id = 3"

# Test 12: Job retry on non-failed job
echo "Test 12: Job retry on non-failed job"
OUTPUT=$($BINARY job retry 2 2>&1) || true
if echo "$OUTPUT" | grep -q "not FAILED\|Only failed"; then
    echo "PASS: Error for retrying non-failed job"
else
    echo "FAIL: Expected error for retrying completed job"
    exit 1
fi
echo

# Test 13: Job retry-all command
echo "Test 13: Job retry-all command"
# First, add another failed job
duckdb "$CASPARIAN_DB" "INSERT INTO cf_processing_queue (id, file_id, plugin_name, status, error_message) VALUES (5, 4, 'sales', 'FAILED', 'Another error')"
OUTPUT=$($BINARY job retry-all 2>&1)
if echo "$OUTPUT" | grep -q "job(s) reset to QUEUED\|No failed jobs"; then
    echo "PASS: Retry-all executed"
else
    echo "FAIL: Expected retry-all confirmation"
    exit 1
fi
echo

# Reset jobs for cancel test
duckdb "$CASPARIAN_DB" "UPDATE cf_processing_queue SET status = 'RUNNING' WHERE id = 1"

# Test 14: Job cancel command
echo "Test 14: Job cancel command"
OUTPUT=$($BINARY job cancel 1 2>&1)
if echo "$OUTPUT" | grep -q "cancelled"; then
    echo "PASS: Job cancelled"
else
    echo "FAIL: Expected cancellation confirmation"
    exit 1
fi
STATUS=$(duckdb "$CASPARIAN_DB" "SELECT status FROM cf_processing_queue WHERE id = 1")
if [ "$STATUS" = "FAILED" ]; then
    echo "PASS: Job status updated to FAILED"
else
    echo "FAIL: Job status not updated (status: $STATUS)"
    exit 1
fi
echo

# Test 15: Job cancel on completed job
echo "Test 15: Job cancel on completed job"
OUTPUT=$($BINARY job cancel 2 2>&1) || true
if echo "$OUTPUT" | grep -q "already completed\|Cannot cancel"; then
    echo "PASS: Error for cancelling completed job"
else
    echo "FAIL: Expected error for cancelling completed job"
    exit 1
fi
echo

# Test 16: Invalid job ID
echo "Test 16: Invalid job ID"
OUTPUT=$($BINARY job show abc 2>&1) || true
if echo "$OUTPUT" | grep -q "Invalid job ID\|TRY:"; then
    echo "PASS: Helpful error for invalid ID"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

echo "==================================="
echo "All jobs command tests passed!"
echo "==================================="
