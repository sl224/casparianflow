#!/usr/bin/env bash
set -euo pipefail

leaks=$(rg -n "\.appender\(" crates | rg -v "crates/casparian_db|crates/casparian_sinks" || true)
if [[ -n "$leaks" ]]; then
  echo "Unexpected DuckDB appender usage outside db/sinks:" >&2
  echo "$leaks" >&2
  exit 1
fi

params=$(rg -n "duckdb::params!" crates | rg -v "crates/casparian_db|crates/casparian_sinks" || true)
if [[ -n "$params" ]]; then
  echo "Unexpected duckdb::params! usage outside db/sinks:" >&2
  echo "$params" >&2
  exit 1
fi
