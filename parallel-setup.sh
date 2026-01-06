#!/bin/bash
# Parallel Development Setup for Casparian Flow MVP
# Run from repo root: ./parallel-setup.sh

set -e

echo "=== Creating git worktrees ==="

# Create worktree directories alongside the main repo
git worktree add ../cf-w1 -b feat/job-loop 2>/dev/null || echo "W1 worktree exists"
git worktree add ../cf-w2 -b feat/status-sync 2>/dev/null || echo "W2 worktree exists"
git worktree add ../cf-w3 -b feat/failure-capture 2>/dev/null || echo "W3 worktree exists"
git worktree add ../cf-w4 -b feat/versioning 2>/dev/null || echo "W4 worktree exists"

echo ""
echo "=== Worktrees created ==="
echo ""
echo "Directory structure:"
echo "  ../cf-w1  (feat/job-loop)      - Job processing loop"
echo "  ../cf-w2  (feat/status-sync)   - Status sync Sentinel->Scout"
echo "  ../cf-w3  (feat/failure-capture) - Failure context capture"
echo "  ../cf-w4  (feat/versioning)    - Parser versioning"
echo ""
echo "=== Next steps ==="
echo ""
echo "Open 4 terminal windows/tabs and run:"
echo ""
echo "  Terminal 1:  cd ../cf-w1/ui && claude"
echo "  Terminal 2:  cd ../cf-w2/ui && claude"
echo "  Terminal 3:  cd ../cf-w3/ui && claude"
echo "  Terminal 4:  cd ../cf-w4/ui && claude"
echo ""
echo "Then paste the corresponding prompt from WORKER_PROMPTS.md"
echo ""
echo "=== Cleanup (when done) ==="
echo ""
echo "After merging all branches:"
echo "  git worktree remove ../cf-w1"
echo "  git worktree remove ../cf-w2"
echo "  git worktree remove ../cf-w3"
echo "  git worktree remove ../cf-w4"
echo "  git branch -d feat/job-loop feat/status-sync feat/failure-capture feat/versioning"
