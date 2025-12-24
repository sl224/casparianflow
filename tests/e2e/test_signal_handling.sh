#!/bin/bash
# E2E Test: Signal Handling
#
# Tests that the unified binary handles SIGINT/SIGTERM gracefully:
# 1. Starts the unified binary
# 2. Waits for it to initialize
# 3. Sends SIGINT
# 4. Verifies exit code is 0 (graceful shutdown)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEST_DIR="$SCRIPT_DIR/signal_test_$$"
DB_PATH="$TEST_DIR/test.db"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"
    rm -rf "$TEST_DIR" 2>/dev/null || true
}
trap cleanup EXIT

echo -e "${YELLOW}=== E2E Signal Handling Test ===${NC}\n"

# Step 1: Build unified binary
echo -e "${YELLOW}Step 1: Building unified binary...${NC}"
cd "$PROJECT_ROOT"
cargo build --release --package casparian 2>&1 | tail -5
echo -e "${GREEN}✓ Binary built${NC}\n"

# Step 2: Setup test environment
echo -e "${YELLOW}Step 2: Setting up test environment...${NC}"
mkdir -p "$TEST_DIR"

# Use the full schema from the E2E test (without test data - we don't need jobs)
sqlite3 "$DB_PATH" <<EOF
CREATE TABLE IF NOT EXISTS cf_source_root (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    type TEXT DEFAULT 'local',
    active INTEGER DEFAULT 1
);

CREATE TABLE IF NOT EXISTS cf_file_location (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_root_id INTEGER NOT NULL,
    rel_path TEXT NOT NULL,
    filename TEXT NOT NULL,
    last_known_mtime REAL,
    last_known_size INTEGER,
    current_version_id INTEGER,
    discovered_time TEXT DEFAULT CURRENT_TIMESTAMP,
    last_seen_time TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (source_root_id) REFERENCES cf_source_root(id)
);

CREATE TABLE IF NOT EXISTS cf_file_hash_registry (
    content_hash TEXT PRIMARY KEY,
    first_seen TEXT DEFAULT CURRENT_TIMESTAMP,
    size_bytes INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cf_file_version (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    location_id INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    modified_time TEXT NOT NULL,
    detected_at TEXT DEFAULT CURRENT_TIMESTAMP,
    applied_tags TEXT DEFAULT '',
    FOREIGN KEY (location_id) REFERENCES cf_file_location(id),
    FOREIGN KEY (content_hash) REFERENCES cf_file_hash_registry(content_hash)
);

CREATE TABLE IF NOT EXISTS cf_plugin_manifest (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    version TEXT NOT NULL,
    source_code TEXT NOT NULL,
    source_hash TEXT NOT NULL UNIQUE,
    status TEXT DEFAULT 'PENDING',
    signature TEXT,
    validation_error TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    deployed_at TEXT,
    env_hash TEXT,
    artifact_hash TEXT,
    publisher_id INTEGER,
    system_requirements TEXT
);

CREATE TABLE IF NOT EXISTS cf_processing_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_version_id INTEGER NOT NULL,
    plugin_name TEXT NOT NULL,
    config_overrides TEXT,
    status TEXT NOT NULL DEFAULT 'PENDING',
    priority INTEGER DEFAULT 0,
    worker_host TEXT,
    worker_pid INTEGER,
    claim_time TEXT,
    end_time TEXT,
    result_summary TEXT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    FOREIGN KEY (file_version_id) REFERENCES cf_file_version(id)
);

CREATE INDEX IF NOT EXISTS ix_queue_pop ON cf_processing_queue(status, priority, id);

CREATE TABLE IF NOT EXISTS cf_topic_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    topic_name TEXT NOT NULL,
    uri TEXT NOT NULL,
    mode TEXT DEFAULT 'append',
    schema_json TEXT
);

CREATE INDEX IF NOT EXISTS ix_topic_lookup ON cf_topic_config(plugin_name, topic_name);
EOF
echo -e "${GREEN}✓ Test environment ready${NC}\n"

# Step 3: Start unified binary
echo -e "${YELLOW}Step 3: Starting unified binary...${NC}"
"$PROJECT_ROOT/target/release/casparian" start \
    --database "sqlite://$DB_PATH" \
    --output "$TEST_DIR/output" \
    > "$TEST_DIR/casparian.log" 2>&1 &
BINARY_PID=$!

# Wait for startup (check if process is running)
sleep 2
if ! kill -0 $BINARY_PID 2>/dev/null; then
    echo -e "${RED}✗ Binary failed to start${NC}"
    cat "$TEST_DIR/casparian.log"
    exit 1
fi
echo -e "${GREEN}✓ Binary started (PID: $BINARY_PID)${NC}\n"

# Step 4: Send SIGINT
echo -e "${YELLOW}Step 4: Sending SIGINT...${NC}"
kill -INT $BINARY_PID

# Step 5: Wait for graceful shutdown (max 15 seconds)
echo -e "${YELLOW}Step 5: Waiting for graceful shutdown...${NC}"
SHUTDOWN_TIMEOUT=15
for i in $(seq 1 $SHUTDOWN_TIMEOUT); do
    if ! kill -0 $BINARY_PID 2>/dev/null; then
        break
    fi
    echo -n "."
    sleep 1
done
echo ""

# Step 6: Check if process exited
if kill -0 $BINARY_PID 2>/dev/null; then
    echo -e "${RED}✗ Binary did not exit within ${SHUTDOWN_TIMEOUT}s${NC}"
    echo "Forcing kill..."
    kill -9 $BINARY_PID 2>/dev/null || true
    cat "$TEST_DIR/casparian.log"
    exit 1
fi

# Step 7: Check exit code
wait $BINARY_PID
EXIT_CODE=$?
echo "Exit code: $EXIT_CODE"

if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}✓ Graceful shutdown completed (exit code 0)${NC}\n"
else
    echo -e "${RED}✗ Non-zero exit code: $EXIT_CODE${NC}"
    cat "$TEST_DIR/casparian.log"
    exit 1
fi

# Step 8: Verify log shows graceful shutdown
echo -e "${YELLOW}Step 8: Verifying shutdown logs...${NC}"
if grep -q "Shutdown complete\|Graceful shutdown\|shutdown" "$TEST_DIR/casparian.log"; then
    echo -e "${GREEN}✓ Shutdown message found in logs${NC}\n"
else
    echo -e "${YELLOW}⚠ No explicit shutdown message (but exit was clean)${NC}\n"
fi

echo -e "${GREEN}=== ✓ Signal Handling Test PASSED ===${NC}"
