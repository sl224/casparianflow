#!/bin/bash
# process_mcdata.sh - CLI-only MCData processing pipeline
#
# Usage: ./process_mcdata.sh <input_folder> <output.db>
#
# This script:
# 1. Scans the folder for MCData files
# 2. Previews each file's schema
# 3. Parses all MCData files to SQLite

set -e

INPUT_FOLDER="${1:?Usage: $0 <input_folder> <output.db>}"
OUTPUT_DB="${2:-mcdata_output.db}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$PROJECT_ROOT/target/debug/casparian"
PARSER="$SCRIPT_DIR/parsers/mcdata_parser.py"

echo "=============================================="
echo "  MCData Processing Pipeline"
echo "=============================================="
echo "Input:  $INPUT_FOLDER"
echo "Output: $OUTPUT_DB"
echo

# Step 1: Scan folder
echo "Step 1: Scanning for files..."
$BINARY scan "$INPUT_FOLDER" -r --type "" --stats
echo

# Step 2: Find MCData files
echo "Step 2: Finding MCData files..."
MCDATA_FILES=$(find "$INPUT_FOLDER" -name "*MCData" -type f 2>/dev/null)

if [ -z "$MCDATA_FILES" ]; then
    echo "No MCData files found!"
    exit 1
fi

FILE_COUNT=$(echo "$MCDATA_FILES" | wc -l | tr -d ' ')
echo "Found $FILE_COUNT MCData file(s)"
echo

# Step 3: Preview first file
echo "Step 3: Previewing schema..."
FIRST_FILE=$(echo "$MCDATA_FILES" | head -1)
$BINARY preview "$FIRST_FILE" --head 5
echo

# Step 4: Parse all files
echo "Step 4: Parsing to SQLite..."
rm -f "$OUTPUT_DB"  # Start fresh

for file in $MCDATA_FILES; do
    echo "  Processing: $(basename "$file")"
    python3 "$PARSER" "$file" "$OUTPUT_DB"
done
echo

# Step 5: Summary
echo "Step 5: Summary"
echo "=============================================="
duckdb "$OUTPUT_DB" << 'EOF'
SELECT 'Total records: ' || COUNT(*) FROM mcdata;
SELECT 'Event types: ' || COUNT(DISTINCT event_name) FROM mcdata;
SELECT 'Subsystems: ' || COUNT(DISTINCT subsystem) FROM mcdata WHERE subsystem IS NOT NULL;
SELECT '';
SELECT 'Top 10 event types:';
SELECT '  ' || event_name || ': ' || COUNT(*) FROM mcdata GROUP BY event_name ORDER BY COUNT(*) DESC LIMIT 10;
EOF

echo
echo "=============================================="
echo "Output database: $OUTPUT_DB"
echo
echo "Query examples:"
echo "  duckdb $OUTPUT_DB \"SELECT * FROM mcdata LIMIT 10\""
echo "  duckdb $OUTPUT_DB \"SELECT event_name, COUNT(*) FROM mcdata GROUP BY 1\""
echo "=============================================="
