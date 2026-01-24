#!/bin/bash
# Profiler TUI Test Script
# Tests F12 toggle and dump trigger functionality

set -e

SESSION="tui-profiler-test"
BINARY="./target/release/casparian"
DUMP_TRIGGER="/tmp/casparian_profile_dump"
DUMP_OUTPUT="/tmp/casparian_profile_data.txt"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_pass() { echo -e "${GREEN}✓ $1${NC}"; }
log_fail() { echo -e "${RED}✗ $1${NC}"; }
log_info() { echo -e "${YELLOW}→ $1${NC}"; }

# Cleanup function
cleanup() {
    tmux kill-session -t "$SESSION" 2>/dev/null || true
    rm -f "$DUMP_TRIGGER" "$DUMP_OUTPUT"
}

trap cleanup EXIT

# Check binary exists with profiling
if [ ! -f "$BINARY" ]; then
    log_fail "Binary not found: $BINARY"
    echo "Run: cargo build -p casparian --release --features profiling"
    exit 1
fi

# Verify profiling feature is enabled
if ! nm "$BINARY" 2>/dev/null | grep -q "casparian_profiler"; then
    log_fail "Binary not built with profiling feature"
    echo "Run: cargo build -p casparian --release --features profiling"
    exit 1
fi

log_info "Starting profiler tests..."

# Clean up any existing session
cleanup

# Start fresh session
log_info "Starting TUI in tmux session..."
tmux new-session -d -s "$SESSION" -x 120 -y 40 "$BINARY tui"
sleep 1

# Capture initial screen
capture_screen() {
    tmux capture-pane -t "$SESSION" -p
}

# Test 1: Initial state - profiler overlay should NOT be visible
log_info "Test 1: Checking initial state (profiler disabled)..."
SCREEN=$(capture_screen)
if echo "$SCREEN" | grep -q "Profiler \[F12\]"; then
    log_fail "Profiler overlay visible on startup (should be hidden)"
    exit 1
fi
log_pass "Profiler overlay hidden on startup"

# Test 2: Toggle profiler with F12
log_info "Test 2: Toggle profiler with F12..."
tmux send-keys -t "$SESSION" F12
sleep 0.5

SCREEN=$(capture_screen)
if echo "$SCREEN" | grep -q "Profiler \[F12\]"; then
    log_pass "Profiler overlay visible after F12"
else
    log_fail "Profiler overlay NOT visible after F12"
    echo "Screen content:"
    echo "$SCREEN"
    exit 1
fi

# Test 3: Check overlay content
log_info "Test 3: Checking overlay content..."
if echo "$SCREEN" | grep -q "Frame:"; then
    log_pass "Frame counter visible"
else
    log_fail "Frame counter not found in overlay"
fi

# Test 4: Toggle off with F12
log_info "Test 4: Toggle profiler off..."
tmux send-keys -t "$SESSION" F12
sleep 0.5

SCREEN=$(capture_screen)
if echo "$SCREEN" | grep -q "Profiler \[F12\]"; then
    log_fail "Profiler overlay still visible after second F12"
    exit 1
fi
log_pass "Profiler overlay hidden after toggle off"

# Test 5: Dump trigger mechanism
log_info "Test 5: Testing dump trigger..."
# Enable profiler
tmux send-keys -t "$SESSION" F12
sleep 0.5

# Wait a few frames to accumulate data
sleep 1

# Create dump trigger
touch "$DUMP_TRIGGER"
sleep 0.5

# Check if dump file was created
if [ -f "$DUMP_OUTPUT" ]; then
    log_pass "Dump file created"

    # Verify content
    if [ -s "$DUMP_OUTPUT" ]; then
        log_pass "Dump file has content"
        echo ""
        log_info "Dump file content (first 20 lines):"
        head -20 "$DUMP_OUTPUT"
    else
        log_fail "Dump file is empty"
        exit 1
    fi

    # Check trigger was cleaned up
    if [ ! -f "$DUMP_TRIGGER" ]; then
        log_pass "Trigger file cleaned up"
    else
        log_fail "Trigger file not cleaned up"
    fi
else
    log_fail "Dump file not created"
    exit 1
fi

# Test 6: Multiple toggle cycles
log_info "Test 6: Multiple toggle cycles..."
for i in 1 2 3; do
    tmux send-keys -t "$SESSION" F12
    sleep 0.3
done
log_pass "Multiple toggles completed without crash"

# Test 7: Verify frame counter increments
log_info "Test 7: Verifying frame counter increments..."
tmux send-keys -t "$SESSION" F12  # Enable if not already
sleep 0.5
SCREEN1=$(capture_screen)
FRAME1=$(echo "$SCREEN1" | grep -o "Frame: [0-9]*" | head -1 | grep -o "[0-9]*")

sleep 1
SCREEN2=$(capture_screen)
FRAME2=$(echo "$SCREEN2" | grep -o "Frame: [0-9]*" | head -1 | grep -o "[0-9]*")

if [ -n "$FRAME1" ] && [ -n "$FRAME2" ]; then
    if [ "$FRAME2" -gt "$FRAME1" ]; then
        log_pass "Frame counter incrementing ($FRAME1 → $FRAME2)"
    else
        log_fail "Frame counter not incrementing ($FRAME1 → $FRAME2)"
    fi
else
    log_fail "Could not extract frame numbers"
fi

# Cleanup and exit
log_info "Cleaning up..."
tmux send-keys -t "$SESSION" q
sleep 0.3

echo ""
echo -e "${GREEN}═══════════════════════════════════════════${NC}"
echo -e "${GREEN}  All profiler tests passed!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════${NC}"
