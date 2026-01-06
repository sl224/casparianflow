#!/bin/bash
# Scout Demo - Bronze Layer Data Ingestion
# Demonstrates: scan, transform, parallel processing, cleanup

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEMO_DIR="$SCRIPT_DIR"

echo "========================================"
echo "  Scout Demo - Bronze Layer Ingestion"
echo "========================================"
echo ""

# Build the binary
echo "[1/6] Building casparian binary..."
cd "$PROJECT_ROOT"
cargo build --release --quiet
CASPARIAN="$PROJECT_ROOT/target/release/casparian"

# Clean up previous demo run
echo "[2/6] Cleaning up previous demo..."
rm -rf "$DEMO_DIR/output"
rm -f "$DEMO_DIR/scout-demo.db"
mkdir -p "$DEMO_DIR/output"

# Show the config
echo "[3/6] Demo configuration:"
echo "  - Source: demo/scout/sample_data/"
echo "  - Routes:"
echo "    * *.csv  -> demo/scout/output/sales/"
echo "    * *.jsonl -> demo/scout/output/events/"
echo "    * *.json  -> demo/scout/output/inventory/"
echo ""

# Show source files
echo "[4/6] Source files:"
ls -la "$DEMO_DIR/sample_data/"
echo ""

# Run scout in one-shot mode
echo "[5/6] Running Scout (one-shot mode with 4 workers)..."
"$CASPARIAN" scout run --config "$DEMO_DIR/scout-demo.toml" --once --workers 4
echo ""

# Show results
echo "[6/6] Results:"
echo ""
echo "Output files created:"
find "$DEMO_DIR/output" -name "*.parquet" -exec ls -lh {} \;
echo ""

echo "Database stats:"
"$CASPARIAN" scout status --db "$DEMO_DIR/scout-demo.db"
echo ""

# Demonstrate cleanup feature
echo "========================================"
echo "  Bonus: Cleanup Feature Demo"
echo "========================================"
echo ""

# Create a temp file to demonstrate delete cleanup
echo "[Bonus] Creating temp file to demonstrate cleanup..."
echo "name,value" > "$DEMO_DIR/sample_data/temp_to_delete.csv"
echo "test,123" >> "$DEMO_DIR/sample_data/temp_to_delete.csv"

echo "Before processing:"
ls "$DEMO_DIR/sample_data/temp_to_delete.csv" 2>/dev/null && echo "  temp_to_delete.csv exists"
echo ""

# Create a config that deletes the temp file
cat > "$DEMO_DIR/cleanup-demo.toml" << 'EOF'
database_path = "demo/scout/cleanup-demo.db"

[[sources]]
id = "temp-source"
name = "Temp Data"
path = "demo/scout/sample_data"
poll_interval_secs = 10
enabled = true
[sources.source_type]
type = "local"

[[routes]]
id = "cleanup-route"
name = "Process and Delete"
source_id = "temp-source"
pattern = "temp_*.csv"
enabled = true
cleanup = "delete"
[routes.transform]
type = "auto"
[routes.sink]
type = "parquet"
path = "demo/scout/output/temp"
EOF

rm -f "$DEMO_DIR/cleanup-demo.db"
"$CASPARIAN" scout run --config "$DEMO_DIR/cleanup-demo.toml" --once --workers 1 2>&1 | grep -E "(Processed|Cleaned)"

echo ""
echo "After processing with cleanup=delete:"
ls "$DEMO_DIR/sample_data/temp_to_delete.csv" 2>/dev/null && echo "  temp_to_delete.csv still exists" || echo "  temp_to_delete.csv was deleted!"
echo ""

# Clean up demo artifacts
rm -f "$DEMO_DIR/cleanup-demo.toml" "$DEMO_DIR/cleanup-demo.db"
rm -rf "$DEMO_DIR/output/temp"

echo "========================================"
echo "  Demo Complete!"
echo "========================================"
echo ""
echo "To inspect the Parquet files:"
echo "  python -c \"import pyarrow.parquet as pq; print(pq.read_table('demo/scout/output/sales/*.parquet').to_pandas())\""
