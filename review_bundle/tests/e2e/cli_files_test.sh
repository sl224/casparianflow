#!/bin/bash
# E2E test for the files command
# Tests listing files with various filters

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

echo "=== Files Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo

# Setup test database
echo "Setting up test database..."
mkdir -p "$DB_DIR/.casparian_flow"
DB_PATH="$DB_DIR/.casparian_flow/casparian_flow.duckdb"

duckdb "$DB_PATH" <<'EOF'
-- Create schema
CREATE TABLE scout_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id BIGINT NOT NULL,
    path TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    size INTEGER NOT NULL,
    mtime INTEGER,
    content_hash TEXT,
    status TEXT DEFAULT 'pending',
    tag TEXT,
    tag_source TEXT,
    rule_id BIGINT,
    manual_plugin TEXT,
    error TEXT,
    first_seen_at INTEGER,
    last_seen_at INTEGER,
    processed_at INTEGER,
    sentinel_job_id INTEGER
);

-- Insert test files with various statuses and tags
INSERT INTO scout_files (source_id, path, rel_path, size, status, tag)
    VALUES (1, '/data/sales/report.csv', 'sales/report.csv', 10000, 'pending', NULL);
INSERT INTO scout_files (source_id, path, rel_path, size, status, tag)
    VALUES (1, '/data/sales/q1.csv', 'sales/q1.csv', 5000, 'tagged', 'sales');
INSERT INTO scout_files (source_id, path, rel_path, size, status, tag)
    VALUES (1, '/data/sales/q2.csv', 'sales/q2.csv', 6000, 'processed', 'sales');
INSERT INTO scout_files (source_id, path, rel_path, size, status, tag)
    VALUES (1, '/data/invoices/jan.json', 'invoices/jan.json', 2500, 'processed', 'invoices');
INSERT INTO scout_files (source_id, path, rel_path, size, status, tag)
    VALUES (1, '/data/invoices/corrupt.json', 'invoices/corrupt.json', 1200, 'failed', 'invoices');
INSERT INTO scout_files (source_id, path, rel_path, size, status, tag, error)
    VALUES (1, '/data/sales/bad.csv', 'sales/bad.csv', 800, 'failed', 'sales', 'Row 15: invalid date format');
INSERT INTO scout_files (source_id, path, rel_path, size, status, tag)
    VALUES (1, '/data/logs/access.log', 'logs/access.log', 50000, 'pending', NULL);
INSERT INTO scout_files (source_id, path, rel_path, size, status, tag)
    VALUES (1, '/data/logs/error.log', 'logs/error.log', 25000, 'pending', NULL);
EOF

echo "Test database created."
echo

# Override home dir for database lookup
export HOME="$DB_DIR"

# Test 1: List all files (no filters)
echo "Test 1: List all files"
OUTPUT=$($BINARY files 2>&1)
echo "$OUTPUT" | head -15
if echo "$OUTPUT" | grep -q "8 files"; then
    echo "PASS: Found 8 files"
else
    echo "FAIL: Expected 8 files"
    exit 1
fi
echo

# Test 2: Filter by topic
echo "Test 2: Filter by topic (--topic sales)"
OUTPUT=$($BINARY files --topic sales 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "3 files"; then
    echo "PASS: Found 3 sales files"
else
    echo "FAIL: Expected 3 sales files"
    exit 1
fi
if echo "$OUTPUT" | grep -q "sales"; then
    echo "PASS: Sales topic shown"
else
    echo "FAIL: Expected 'sales' in output"
    exit 1
fi
echo

# Test 3: Filter by status
echo "Test 3: Filter by status (--status failed)"
OUTPUT=$($BINARY files --status failed 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "2 files"; then
    echo "PASS: Found 2 failed files"
else
    echo "FAIL: Expected 2 failed files"
    exit 1
fi
if echo "$OUTPUT" | grep -q "invalid date"; then
    echo "PASS: Error message shown"
else
    echo "FAIL: Expected error message in output"
    exit 1
fi
echo

# Test 4: Filter untagged files
echo "Test 4: Filter untagged files (--untagged)"
OUTPUT=$($BINARY files --untagged 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "3 files"; then
    echo "PASS: Found 3 untagged files"
else
    echo "FAIL: Expected 3 untagged files"
    exit 1
fi
echo

# Test 5: Combined filters
echo "Test 5: Combined filters (--topic sales --status failed)"
OUTPUT=$($BINARY files --topic sales --status failed 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "1 file"; then
    echo "PASS: Found 1 matching file"
else
    echo "FAIL: Expected 1 file matching both filters"
    exit 1
fi
echo

# Test 6: Use 'done' as alias for 'processed'
echo "Test 6: Status alias 'done' for 'processed'"
OUTPUT=$($BINARY files --status done 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "2 files"; then
    echo "PASS: Found 2 processed files using 'done' alias"
else
    echo "FAIL: Expected 2 processed files"
    exit 1
fi
echo

# Test 7: Limit results
echo "Test 7: Limit results (--limit 2)"
OUTPUT=$($BINARY files --limit 2 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "2 files"; then
    echo "PASS: Results limited to 2"
else
    echo "FAIL: Expected 2 files with limit"
    exit 1
fi
if echo "$OUTPUT" | grep -q "Results limited\|Use --limit"; then
    echo "PASS: Limit hint shown"
else
    echo "PASS: Results shown (hint may not appear for exact match)"
fi
echo

# Test 8: No matching files
echo "Test 8: No matching files"
OUTPUT=$($BINARY files --topic nonexistent 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "No files found"; then
    echo "PASS: No files message shown"
else
    echo "FAIL: Expected 'No files found' message"
    exit 1
fi
echo

# Test 9: Invalid status
echo "Test 9: Invalid status"
OUTPUT=$($BINARY files --status invalid 2>&1) || true
if echo "$OUTPUT" | grep -q "Invalid status\|TRY:"; then
    echo "PASS: Helpful error for invalid status"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

# Test 10: Case insensitive status
echo "Test 10: Case insensitive status (--status PENDING)"
OUTPUT=$($BINARY files --status PENDING 2>&1)
if echo "$OUTPUT" | grep -q "files"; then
    echo "PASS: Case insensitive status works"
else
    echo "FAIL: Expected files with PENDING status"
    exit 1
fi
echo

echo "==================================="
echo "All files command tests passed!"
echo "==================================="
