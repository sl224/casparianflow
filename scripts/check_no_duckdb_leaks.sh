#!/usr/bin/env bash
set -euo pipefail

if command -v rg >/dev/null 2>&1; then
  leaks=$(rg -n "\.appender\(" crates | rg -v "crates/casparian_db|crates/casparian_sinks" || true)
else
  leaks=$(grep -R -n "\.appender(" crates | grep -v "crates/casparian_db" | grep -v "crates/casparian_sinks" || true)
fi
if [[ -n "$leaks" ]]; then
  echo "Unexpected DuckDB appender usage outside db/sinks:" >&2
  echo "$leaks" >&2
  exit 1
fi

if command -v rg >/dev/null 2>&1; then
  params=$(rg -n "duckdb::params!" crates | rg -v "crates/casparian_db|crates/casparian_sinks" || true)
else
  params=$(grep -R -n "duckdb::params!" crates | grep -v "crates/casparian_db" | grep -v "crates/casparian_sinks" || true)
fi
if [[ -n "$params" ]]; then
  echo "Unexpected duckdb::params! usage outside db/sinks:" >&2
  echo "$params" >&2
  exit 1
fi
