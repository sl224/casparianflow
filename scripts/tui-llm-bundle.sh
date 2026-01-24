#!/bin/bash
# scripts/tui-llm-bundle.sh - Build combined TUI review bundle for LLMs

set -euo pipefail

OUT_ROOT=".test_output"
SNAPSHOT_DIR="$OUT_ROOT/tui_snapshots"
TMUX_DIR="$OUT_ROOT/tui_tmux_captures"
SCENARIO="critical_path"
BUNDLE="$OUT_ROOT/tui_llm_review.md"

mkdir -p "$OUT_ROOT"

cargo run -p casparian -- tui-snapshots --out "$SNAPSHOT_DIR"

./scripts/tui-llm-capture.sh --out "$TMUX_DIR" --scenario "$SCENARIO"

SNAP_MD="$SNAPSHOT_DIR/tui_snapshots.md"
TMUX_MD="$TMUX_DIR/$SCENARIO/${SCENARIO}.md"

{
    echo "# TUI LLM Review Bundle"
    echo ""
    echo "Generated: $(date -u '+%Y-%m-%d %H:%M UTC')"
    if git rev-parse --short HEAD >/dev/null 2>&1; then
        echo "Revision: $(git rev-parse --short HEAD)"
    fi
    echo ""
    echo "## Static Snapshots"
    echo ""
    cat "$SNAP_MD"
    echo ""
    echo "## Tmux Captures"
    echo ""
    cat "$TMUX_MD"
} > "$BUNDLE"

echo "Bundle written to $BUNDLE"
