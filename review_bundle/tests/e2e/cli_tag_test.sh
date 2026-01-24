#!/bin/bash
# E2E test for the tag and untag commands
# Tests tagging files with rules and manual tagging

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)
DB_DIR=$(mktemp -d)
DB_PATH="$DB_DIR/casparian_flow.duckdb"

cleanup() {
    rm -rf "$TEST_DIR"
    rm -rf "$DB_DIR"
}
trap cleanup EXIT

echo "=== Tag Command E2E Tests ==="
echo "Binary: $BINARY"
echo "Test dir: $TEST_DIR"
echo "DB path: $DB_PATH"
echo

# Setup test database
echo "Setting up test database..."
duckdb "$DB_PATH" <<'EOF'
-- Create schema
CREATE TABLE scout_sources (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL,
    source_type TEXT NOT NULL,
    path TEXT NOT NULL,
    poll_interval_secs INTEGER DEFAULT 30,
    enabled INTEGER DEFAULT 1,
    created_at INTEGER,
    updated_at INTEGER
);

CREATE TABLE scout_tagging_rules (
    id BIGINT PRIMARY KEY,
    name TEXT,
    source_id BIGINT NOT NULL,
    pattern TEXT NOT NULL,
    tag TEXT NOT NULL,
    priority INTEGER DEFAULT 0,
    enabled INTEGER DEFAULT 1,
    created_at INTEGER,
    updated_at INTEGER
);

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

-- Insert test source
INSERT INTO scout_sources (id, name, source_type, path) VALUES (1, 'Test Source', 'local', '/test/data');

-- Insert test tagging rules
INSERT INTO scout_tagging_rules (id, source_id, pattern, tag, priority, enabled)
    VALUES (2, 1, '*.csv', 'csv_data', 10, 1);
INSERT INTO scout_tagging_rules (id, source_id, pattern, tag, priority, enabled)
    VALUES (3, 1, '*.json', 'json_data', 5, 1);
INSERT INTO scout_tagging_rules (id, source_id, pattern, tag, priority, enabled)
    VALUES (4, 1, '*.txt', 'text_data', 0, 0);

-- Insert test files (untagged)
INSERT INTO scout_files (source_id, path, rel_path, size, status)
    VALUES (1, '/test/data/sales.csv', 'sales.csv', 1000, 'pending');
INSERT INTO scout_files (source_id, path, rel_path, size, status)
    VALUES (1, '/test/data/invoices.csv', 'invoices.csv', 2000, 'pending');
INSERT INTO scout_files (source_id, path, rel_path, size, status)
    VALUES (1, '/test/data/config.json', 'config.json', 500, 'pending');
INSERT INTO scout_files (source_id, path, rel_path, size, status)
    VALUES (1, '/test/data/readme.txt', 'readme.txt', 100, 'pending');
INSERT INTO scout_files (source_id, path, rel_path, size, status)
    VALUES (1, '/test/data/unknown.xyz', 'unknown.xyz', 50, 'pending');
EOF

echo "Test database created."
echo

# Override home dir for database lookup
export HOME="$DB_DIR"
mkdir -p "$HOME/.casparian_flow"
mv "$DB_PATH" "$HOME/.casparian_flow/casparian_flow.duckdb"
DB_PATH="$HOME/.casparian_flow/casparian_flow.duckdb"

# Test 1: Tag command with --dry-run
echo "Test 1: Tag command with --dry-run"
OUTPUT=$($BINARY tag --dry-run 2>&1)
echo "$OUTPUT" | head -10
if echo "$OUTPUT" | grep -q "DRY RUN"; then
    echo "PASS: Dry run message shown"
else
    echo "FAIL: Expected 'DRY RUN' message"
    exit 1
fi
if echo "$OUTPUT" | grep -q "WOULD TAG"; then
    echo "PASS: Would tag preview shown"
else
    echo "FAIL: Expected 'WOULD TAG' preview"
    exit 1
fi
if echo "$OUTPUT" | grep -q "csv_data"; then
    echo "PASS: CSV rule would apply"
else
    echo "FAIL: Expected csv_data tag"
    exit 1
fi
echo

# Verify no changes were made
TAGGED_COUNT=$(duckdb "$DB_PATH" "SELECT COUNT(*) FROM scout_files WHERE tag IS NOT NULL")
if [ "$TAGGED_COUNT" = "0" ]; then
    echo "PASS: Dry run made no changes"
else
    echo "FAIL: Dry run should not change database"
    exit 1
fi
echo

# Test 2: Apply rules (actually tag files)
echo "Test 2: Apply tagging rules"
OUTPUT=$($BINARY tag 2>&1)
echo "$OUTPUT" | head -10
if echo "$OUTPUT" | grep -q "TAGGING\|Applied tags"; then
    echo "PASS: Tagging applied message shown"
else
    echo "FAIL: Expected tagging confirmation"
    exit 1
fi
echo

# Verify files were tagged
TAGGED_COUNT=$(duckdb "$DB_PATH" "SELECT COUNT(*) FROM scout_files WHERE tag IS NOT NULL")
if [ "$TAGGED_COUNT" = "3" ]; then
    echo "PASS: 3 files tagged (2 CSV + 1 JSON)"
else
    echo "FAIL: Expected 3 files to be tagged, got $TAGGED_COUNT"
    exit 1
fi

# Verify correct tags
CSV_COUNT=$(duckdb "$DB_PATH" "SELECT COUNT(*) FROM scout_files WHERE tag = 'csv_data'")
if [ "$CSV_COUNT" = "2" ]; then
    echo "PASS: 2 files tagged with csv_data"
else
    echo "FAIL: Expected 2 CSV files tagged, got $CSV_COUNT"
    exit 1
fi
echo

# Test 3: Manual tagging
echo "Test 3: Manual tagging"
OUTPUT=$($BINARY tag /test/data/unknown.xyz custom_tag 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "Tagged:.*custom_tag\|-> custom_tag"; then
    echo "PASS: Manual tagging confirmed"
else
    echo "FAIL: Expected manual tagging confirmation"
    exit 1
fi

# Verify manual tag
MANUAL_TAG=$(duckdb "$DB_PATH" "SELECT tag FROM scout_files WHERE path = '/test/data/unknown.xyz'")
if [ "$MANUAL_TAG" = "custom_tag" ]; then
    echo "PASS: Manual tag applied correctly"
else
    echo "FAIL: Expected custom_tag, got $MANUAL_TAG"
    exit 1
fi
MANUAL_SOURCE=$(duckdb "$DB_PATH" "SELECT tag_source FROM scout_files WHERE path = '/test/data/unknown.xyz'")
if [ "$MANUAL_SOURCE" = "manual" ]; then
    echo "PASS: Tag source set to manual"
else
    echo "FAIL: Expected tag_source=manual, got $MANUAL_SOURCE"
    exit 1
fi
echo

# Test 4: Untag command
echo "Test 4: Untag command"
OUTPUT=$($BINARY untag /test/data/unknown.xyz 2>&1)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -q "Untagged:"; then
    echo "PASS: Untag confirmed"
else
    echo "FAIL: Expected untag confirmation"
    exit 1
fi

# Verify tag removed
UNTAGGED=$(duckdb "$DB_PATH" "SELECT COALESCE(tag, 'NULL') FROM scout_files WHERE path = '/test/data/unknown.xyz'")
if [ "$UNTAGGED" = "NULL" ]; then
    echo "PASS: Tag removed"
else
    echo "FAIL: Expected NULL tag, got $UNTAGGED"
    exit 1
fi
UNTAGGED_STATUS=$(duckdb "$DB_PATH" "SELECT status FROM scout_files WHERE path = '/test/data/unknown.xyz'")
if [ "$UNTAGGED_STATUS" = "pending" ]; then
    echo "PASS: Status reset to pending"
else
    echo "FAIL: Expected pending status, got $UNTAGGED_STATUS"
    exit 1
fi
echo

# Test 5: Tag non-existent file
echo "Test 5: Tag non-existent file (error handling)"
OUTPUT=$($BINARY tag /nonexistent/file.csv some_tag 2>&1) || true
if echo "$OUTPUT" | grep -q "TRY:\|ERROR:\|not found"; then
    echo "PASS: Helpful error for non-existent file"
else
    echo "FAIL: Expected helpful error message"
    exit 1
fi
echo

# Test 6: Tag with no untagged files
echo "Test 6: Tag with all files already tagged"
OUTPUT=$($BINARY tag 2>&1) || true
if echo "$OUTPUT" | grep -q "No untagged files\|already been tagged\|UNTAGGED"; then
    echo "PASS: Appropriate message when no work to do"
else
    echo "FAIL: Expected message about no untagged files"
    exit 1
fi
echo

# Test 7: Untag file that's not tagged
echo "Test 7: Untag file that's not tagged"
OUTPUT=$($BINARY untag /test/data/unknown.xyz 2>&1) || true
if echo "$OUTPUT" | grep -q "not tagged"; then
    echo "PASS: Message for already untagged file"
else
    echo "FAIL: Expected 'not tagged' message"
    exit 1
fi
echo

echo "==================================="
echo "All tag command tests passed!"
echo "==================================="
