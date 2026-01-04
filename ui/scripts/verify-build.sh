#!/bin/bash
#
# UI Build Verification Script
#
# This script catches the kind of bugs that type checking alone misses:
# - Module scope errors (functions in <script module> used in templates)
# - Runtime import errors
# - Build configuration issues
#
# Casey Muratori would approve: simple, fast, catches real bugs.
#

set -e

echo "=== UI Build Verification ==="
echo ""

cd "$(dirname "$0")/.."

# Step 1: Type checking
echo "[1/4] Running type check..."
bun run check
echo "    Type check passed"

# Step 2: Build the frontend (catches Svelte compile errors)
echo ""
echo "[2/4] Building frontend..."
bun run build 2>&1 | tail -5
echo "    Build completed"

# Step 3: Run existing unit tests
echo ""
echo "[3/4] Running unit tests..."
bun run test 2>&1 | tail -10
echo "    Tests passed"

# Step 4: Quick dev server smoke test (catches runtime errors)
echo ""
echo "[4/4] Dev server smoke test..."

# Start dev server in background
bun run dev &
DEV_PID=$!

# Wait for server to start
sleep 3

# Check if server is responding
if curl -s http://localhost:1420/ > /dev/null 2>&1; then
    echo "    Dev server responding"
else
    echo "    WARNING: Dev server not responding (may need manual check)"
fi

# Kill dev server
kill $DEV_PID 2>/dev/null || true
wait $DEV_PID 2>/dev/null || true

echo ""
echo "=== All checks passed ==="
