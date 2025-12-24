#!/bin/bash
# Wrapper to run bridge_shim.py via uv
# This isolates any Rust subprocess environment issues
cd "$(dirname "$0")/../../.."
exec uv run --frozen --no-sync python "$@"
