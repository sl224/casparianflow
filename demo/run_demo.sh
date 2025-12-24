#!/bin/bash
#
# Casparian Flow E2E Demo
#
# Demonstrates the full pipeline:
# 1. Starts Casparian Deck (Tauri UI with embedded Sentinel)
# 2. Starts a Worker that connects to the Sentinel
# 3. Processes jobs with visible UI updates
#
# Usage:
#   ./run_demo.sh         # Run with Tauri UI
#   ./run_demo.sh --cli   # Run CLI-only (no UI)
#

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DEMO_DIR="$SCRIPT_DIR"
OUTPUT_DIR="$DEMO_DIR/output"
DB_PATH="$DEMO_DIR/demo.db"

# Socket path for IPC
SOCKET_PATH="${TMPDIR:-/tmp}/casparian_demo.sock"
BIND_ADDR="ipc://$SOCKET_PATH"

# Parse arguments
USE_UI=true
if [ "$1" = "--cli" ]; then
    USE_UI=false
fi

echo -e "${BOLD}${CYAN}"
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║           CASPARIAN FLOW - E2E DEMO                         ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    pkill -f "casparian-sentinel" 2>/dev/null || true
    pkill -f "casparian-worker" 2>/dev/null || true
    pkill -f "casparian sentinel" 2>/dev/null || true
    pkill -f "casparian worker" 2>/dev/null || true
    rm -f "$SOCKET_PATH"
    echo -e "${GREEN}Cleanup complete${NC}"
}

trap cleanup EXIT

# Step 1: Build
echo -e "${YELLOW}[1/6] Building binaries...${NC}"
cd "$PROJECT_ROOT"

if [ "$USE_UI" = true ]; then
    echo "  Building unified casparian binary..."
    cargo build --release --package casparian 2>/dev/null || cargo build --release
else
    echo "  Building sentinel and worker..."
    cargo build --release --package casparian_sentinel --package casparian_worker
fi
echo -e "${GREEN}  ✓ Build complete${NC}\n"

# Step 2: Setup database
echo -e "${YELLOW}[2/6] Setting up demo database...${NC}"
rm -f "$DB_PATH"
rm -f "$SOCKET_PATH"
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

# Update the schema with actual paths
sed "s|DEMO_DIR|$DEMO_DIR|g" "$DEMO_DIR/schema.sql" | sqlite3 "$DB_PATH"
echo -e "${GREEN}  ✓ Database created at $DB_PATH${NC}"
echo -e "  ✓ 3 jobs queued for processing\n"

# Step 3: Setup Python environment
echo -e "${YELLOW}[3/6] Setting up Python environment...${NC}"
VENV_DIR="$HOME/.casparian_flow/venvs/demo_env_hash"
mkdir -p "$(dirname "$VENV_DIR")"
rm -rf "$VENV_DIR"
ln -sf "$PROJECT_ROOT/.venv" "$VENV_DIR"
echo -e "${GREEN}  ✓ Environment linked${NC}\n"

# Step 4: Start Sentinel
echo -e "${YELLOW}[4/6] Starting Sentinel...${NC}"

if [ "$USE_UI" = true ]; then
    echo -e "  ${CYAN}Starting Casparian Deck (Tauri UI)...${NC}"
    echo -e "  ${CYAN}The UI will open in a separate window.${NC}"

    # Set environment for Tauri app
    export CASPARIAN_BIND="$BIND_ADDR"
    export CASPARIAN_DATABASE="sqlite://$DB_PATH"

    # Start Tauri app in background (it embeds the Sentinel)
    cd "$PROJECT_ROOT/ui"
    bun run tauri dev &
    TAURI_PID=$!

    # Wait for socket to appear
    echo -n "  Waiting for Sentinel..."
    for i in {1..30}; do
        if [ -S "$SOCKET_PATH" ]; then
            echo -e " ${GREEN}ready!${NC}"
            break
        fi
        echo -n "."
        sleep 1
    done

    if [ ! -S "$SOCKET_PATH" ]; then
        echo -e "\n${RED}  ✗ Sentinel failed to start within 30 seconds${NC}"
        exit 1
    fi
else
    # CLI mode - use standalone sentinel
    "$PROJECT_ROOT/target/release/casparian" sentinel \
        --bind "$BIND_ADDR" \
        --database "sqlite://$DB_PATH" \
        > "$DEMO_DIR/sentinel.log" 2>&1 &
    SENTINEL_PID=$!

    sleep 2
    if ! kill -0 $SENTINEL_PID 2>/dev/null; then
        echo -e "${RED}  ✗ Sentinel failed to start${NC}"
        cat "$DEMO_DIR/sentinel.log"
        exit 1
    fi
    echo -e "${GREEN}  ✓ Sentinel running (PID: $SENTINEL_PID)${NC}"
fi

echo -e "${GREEN}  ✓ Sentinel listening on $BIND_ADDR${NC}\n"

# Step 5: Start Worker
echo -e "${YELLOW}[5/6] Starting Worker...${NC}"

"$PROJECT_ROOT/target/release/casparian" worker \
    --connect "$BIND_ADDR" \
    --output "$OUTPUT_DIR" \
    > "$DEMO_DIR/worker.log" 2>&1 &
WORKER_PID=$!

sleep 2
if ! kill -0 $WORKER_PID 2>/dev/null; then
    echo -e "${RED}  ✗ Worker failed to start${NC}"
    cat "$DEMO_DIR/worker.log"
    exit 1
fi
echo -e "${GREEN}  ✓ Worker running (PID: $WORKER_PID)${NC}\n"

# Step 6: Monitor jobs
echo -e "${YELLOW}[6/6] Processing jobs...${NC}"
echo -e "  ${CYAN}Watch the UI for real-time metrics!${NC}"
echo ""
echo -e "${BOLD}  Processing 3 jobs × 4 batches × 1.5s delay = ~18 seconds total${NC}"
echo ""

# Monitor loop
LAST_COMPLETED=0
while true; do
    # Get job stats
    STATS=$(sqlite3 "$DB_PATH" "
        SELECT
            COUNT(*) FILTER (WHERE status = 'QUEUED'),
            COUNT(*) FILTER (WHERE status = 'RUNNING'),
            COUNT(*) FILTER (WHERE status = 'COMPLETED'),
            COUNT(*) FILTER (WHERE status = 'FAILED')
        FROM cf_processing_queue
    " | tr '|' ' ')

    read QUEUED RUNNING COMPLETED FAILED <<< "$STATS"

    # Progress bar
    TOTAL=3
    DONE=$((COMPLETED + FAILED))
    PCT=$((DONE * 100 / TOTAL))
    BAR_LEN=$((PCT / 5))

    printf "\r  ["
    for ((i=0; i<20; i++)); do
        if ((i < BAR_LEN)); then
            printf "█"
        else
            printf "░"
        fi
    done
    printf "] %3d%% | Q:%d R:%d C:%d F:%d  " "$PCT" "$QUEUED" "$RUNNING" "$COMPLETED" "$FAILED"

    # Check if all done
    if [ "$DONE" -eq "$TOTAL" ]; then
        echo ""
        break
    fi

    sleep 0.5
done

echo ""

# Summary
echo -e "${BOLD}${GREEN}"
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                    DEMO COMPLETE                             ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

echo -e "${CYAN}Results:${NC}"
sqlite3 -header -column "$DB_PATH" "
    SELECT id, plugin_name, status,
           CASE WHEN end_time IS NOT NULL
                THEN round((julianday(end_time) - julianday(claim_time)) * 86400, 1) || 's'
                ELSE '-'
           END as duration
    FROM cf_processing_queue
"

echo ""
echo -e "${CYAN}Output files:${NC}"
ls -lh "$OUTPUT_DIR"/*.parquet 2>/dev/null || echo "  (no output files yet)"

echo ""
if [ "$USE_UI" = true ]; then
    echo -e "${YELLOW}The Tauri UI window is still open.${NC}"
    echo -e "${YELLOW}Press Ctrl+C to exit and close everything.${NC}"
    wait
fi
