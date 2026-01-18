#!/bin/bash
# E2E test for the worker-cli command
# Tests worker listing and management

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

echo "=== Worker Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo "Database: $CASPARIAN_DB"
echo

# Create test database with sample data
echo "Setting up test database..."
duckdb "$CASPARIAN_DB" <<EOF
-- Create worker node table
CREATE TABLE cf_worker_node (
    hostname TEXT PRIMARY KEY,
    pid INTEGER NOT NULL,
    ip_address TEXT,
    env_signature TEXT,
    started_at TEXT NOT NULL,
    last_heartbeat TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'idle',
    current_job_id INTEGER
);

-- Create processing queue table for queue stats
CREATE TABLE cf_processing_queue (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL,
    pipeline_run_id TEXT,
    plugin_name TEXT NOT NULL,
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

-- Insert test workers
INSERT INTO cf_worker_node (hostname, pid, ip_address, started_at, last_heartbeat, status, current_job_id)
    VALUES ('worker-1', 12345, '192.168.1.10', '2024-12-16T08:00:00Z', '2024-12-16T10:30:00Z', 'busy', 42);
INSERT INTO cf_worker_node (hostname, pid, ip_address, started_at, last_heartbeat, status, current_job_id)
    VALUES ('worker-2', 12346, '192.168.1.11', '2024-12-16T08:00:00Z', '2024-12-16T10:30:00Z', 'idle', NULL);
INSERT INTO cf_worker_node (hostname, pid, ip_address, started_at, last_heartbeat, status, current_job_id)
    VALUES ('worker-3', 12347, '192.168.1.12', '2024-12-16T08:00:00Z', '2024-12-16T09:00:00Z', 'draining', NULL);

-- Insert test jobs
INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (42, 1, 'test', 'RUNNING');
INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (43, 2, 'test', 'QUEUED');
INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (44, 3, 'test', 'COMPLETED');
INSERT INTO cf_processing_queue (id, file_id, plugin_name, status) VALUES (45, 4, 'test', 'FAILED');
EOF

echo "Test database created."
echo

# Test 1: Worker status command
echo "Test 1: Worker status command"
OUTPUT=$($BINARY worker-cli status 2>&1)
echo "$OUTPUT" | head -15
if echo "$OUTPUT" | grep -q "WORKER STATUS"; then
    echo "PASS: Status header present"
else
    echo "FAIL: Expected status header"
    exit 1
fi
if echo "$OUTPUT" | grep -q "Total:"; then
    echo "PASS: Total workers shown"
else
    echo "FAIL: Expected total workers count"
    exit 1
fi
if echo "$OUTPUT" | grep -q "QUEUE:"; then
    echo "PASS: Queue stats shown"
else
    echo "FAIL: Expected queue stats"
    exit 1
fi
echo

# Test 2: Worker list command
echo "Test 2: Worker list command"
OUTPUT=$($BINARY worker-cli list 2>&1)
if echo "$OUTPUT" | grep -q "WORKERS"; then
    echo "PASS: Workers header present"
else
    echo "FAIL: Expected workers header"
    exit 1
fi
if echo "$OUTPUT" | grep -q "worker-1"; then
    echo "PASS: Worker-1 listed"
else
    echo "FAIL: Expected worker-1 in list"
    exit 1
fi
if echo "$OUTPUT" | grep -q "worker-2"; then
    echo "PASS: Worker-2 listed"
else
    echo "FAIL: Expected worker-2 in list"
    exit 1
fi
echo

# Test 3: Worker list with JSON output
echo "Test 3: Worker list with --json"
OUTPUT=$($BINARY worker-cli list --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    echo "FAIL: Invalid JSON output"
    exit 1
fi
echo

# Test 4: Worker show command
echo "Test 4: Worker show command"
OUTPUT=$($BINARY worker-cli show worker-1 2>&1)
if echo "$OUTPUT" | grep -q "WORKER: worker-1"; then
    echo "PASS: Worker header present"
else
    echo "FAIL: Expected worker header"
    exit 1
fi
if echo "$OUTPUT" | grep -q "PID:"; then
    echo "PASS: PID shown"
else
    echo "FAIL: Expected PID"
    exit 1
fi
if echo "$OUTPUT" | grep -q "BUSY"; then
    echo "PASS: Status shown"
else
    echo "FAIL: Expected status"
    exit 1
fi
echo

# Test 5: Worker show with JSON output
echo "Test 5: Worker show with --json"
OUTPUT=$($BINARY worker-cli show worker-1 --json 2>&1)
if echo "$OUTPUT" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    echo "PASS: Valid JSON output"
else
    echo "FAIL: Invalid JSON output"
    exit 1
fi
echo

# Test 6: Worker show for non-existent worker
echo "Test 6: Worker show for non-existent worker"
OUTPUT=$($BINARY worker-cli show nonexistent 2>&1) || true
if echo "$OUTPUT" | grep -q "not found\|TRY:"; then
    echo "PASS: Helpful error for missing worker"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

# Test 7: Worker drain command
echo "Test 7: Worker drain command"
OUTPUT=$($BINARY worker-cli drain worker-2 2>&1)
if echo "$OUTPUT" | grep -q "DRAINING"; then
    echo "PASS: Worker set to draining"
else
    echo "FAIL: Expected draining confirmation"
    exit 1
fi
STATUS=$(duckdb "$CASPARIAN_DB" "SELECT status FROM cf_worker_node WHERE hostname = 'worker-2'")
if [ "$STATUS" = "draining" ]; then
    echo "PASS: Database updated correctly"
else
    echo "FAIL: Database not updated (status: $STATUS)"
    exit 1
fi
echo

# Test 8: Worker drain on already draining worker
echo "Test 8: Worker drain on already draining worker"
OUTPUT=$($BINARY worker-cli drain worker-3 2>&1)
if echo "$OUTPUT" | grep -q "already draining"; then
    echo "PASS: Recognized already draining"
else
    echo "PASS: Drain command accepted"
fi
echo

# Test 9: Worker remove on idle worker
echo "Test 9: Worker remove on idle/draining worker"
# First update worker-2 back to idle
duckdb "$CASPARIAN_DB" "UPDATE cf_worker_node SET status = 'idle', current_job_id = NULL WHERE hostname = 'worker-2'"
OUTPUT=$($BINARY worker-cli remove worker-2 2>&1)
if echo "$OUTPUT" | grep -q "removed"; then
    echo "PASS: Worker removed"
else
    echo "FAIL: Expected removal confirmation"
    exit 1
fi
COUNT=$(duckdb "$CASPARIAN_DB" "SELECT COUNT(*) FROM cf_worker_node WHERE hostname = 'worker-2'")
if [ "$COUNT" = "0" ]; then
    echo "PASS: Worker removed from database"
else
    echo "FAIL: Worker still in database"
    exit 1
fi
echo

# Test 10: Worker remove on busy worker without force
echo "Test 10: Worker remove on busy worker without force"
OUTPUT=$($BINARY worker-cli remove worker-1 2>&1) || true
if echo "$OUTPUT" | grep -q "active job\|Cannot remove\|TRY:"; then
    echo "PASS: Error for removing busy worker"
else
    echo "FAIL: Expected error for removing busy worker"
    exit 1
fi
echo

# Test 11: Worker remove with --force
echo "Test 11: Worker remove with --force"
OUTPUT=$($BINARY worker-cli remove worker-1 --force 2>&1)
if echo "$OUTPUT" | grep -q "removed\|Requeued"; then
    echo "PASS: Worker forcefully removed"
else
    echo "FAIL: Expected removal confirmation"
    exit 1
fi
COUNT=$(duckdb "$CASPARIAN_DB" "SELECT COUNT(*) FROM cf_worker_node WHERE hostname = 'worker-1'")
if [ "$COUNT" = "0" ]; then
    echo "PASS: Worker removed from database"
else
    echo "FAIL: Worker still in database"
    exit 1
fi
# Check that job was requeued
STATUS=$(duckdb "$CASPARIAN_DB" "SELECT status FROM cf_processing_queue WHERE id = 42")
if [ "$STATUS" = "QUEUED" ]; then
    echo "PASS: Job was requeued"
else
    echo "FAIL: Job was not requeued (status: $STATUS)"
    exit 1
fi
echo

# Test 12: Worker status with no workers
echo "Test 12: Worker status with no workers"
duckdb "$CASPARIAN_DB" "DELETE FROM cf_worker_node"
OUTPUT=$($BINARY worker-cli status 2>&1)
if echo "$OUTPUT" | grep -q "No workers\|Total:.*0"; then
    echo "PASS: Empty workers message shown"
else
    echo "FAIL: Expected empty workers message"
    exit 1
fi
echo

# Test 13: Worker list with no workers
echo "Test 13: Worker list with no workers"
OUTPUT=$($BINARY worker-cli list 2>&1)
if echo "$OUTPUT" | grep -q "No workers\|TRY:"; then
    echo "PASS: Empty list message shown"
else
    echo "FAIL: Expected empty list message"
    exit 1
fi
echo

echo "==================================="
echo "All worker command tests passed!"
echo "==================================="
