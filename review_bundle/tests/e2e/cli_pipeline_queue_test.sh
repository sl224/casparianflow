#!/bin/bash
# E2E test for pipeline apply/run job enqueueing.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
TEST_DIR=$(mktemp -d)
export CASPARIAN_HOME="$TEST_DIR/home"
export CASPARIAN_DB_BACKEND="duckdb"

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

DB_PATH="$CASPARIAN_HOME/casparian_flow.duckdb"
PIPELINE_FILE="$TEST_DIR/pipeline.yaml"

mkdir -p "$CASPARIAN_HOME"

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found. Run 'cargo build' first."
    exit 1
fi

echo "=== Pipeline Queue E2E Test ==="
echo "DB: $DB_PATH"
echo "Pipeline: $PIPELINE_FILE"
echo

duckdb "$DB_PATH" <<'EOF'
CREATE TABLE scout_files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL,
    tag TEXT,
    status TEXT,
    mtime BIGINT
);

INSERT INTO scout_files (id, path, tag, status, mtime) VALUES
    (1, '/data/demo/a.csv', 'demo', 'pending', 1737187200000),
    (2, '/data/demo/b.csv', 'demo', 'pending', 1737187200000),
    (3, '/data/demo/c.csv', 'demo', 'pending', 1737187200000);
EOF

cat > "$PIPELINE_FILE" <<'YAML'
pipeline:
  name: demo_pipeline
  selection:
    tag: demo
  run:
    parser: demo_parser
YAML

echo "Applying pipeline..."
$BINARY pipeline apply "$PIPELINE_FILE"

echo "Running pipeline..."
$BINARY pipeline run demo_pipeline --logical-date 2025-01-01

COUNT=$(duckdb "$DB_PATH" "COPY (SELECT COUNT(*) FROM cf_processing_queue WHERE plugin_name = 'demo_parser') TO stdout (FORMAT csv, HEADER false);")
if [ "$COUNT" -ne 3 ]; then
    echo "FAIL: Expected 3 jobs, got $COUNT"
    exit 1
fi

echo "PASS: Enqueued $COUNT jobs"
