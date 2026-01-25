#!/usr/bin/env bash
set -euo pipefail

tier="${1:-}"

if [[ -z "${tier}" ]]; then
  echo "Usage: $0 t0|t1|t2" >&2
  exit 1
fi

case "${tier}" in
  t0)
    cargo test -p casparian conf_t0_
    ;;
  t1)
    cargo test -p casparian conf_t1_
    cargo test -p casparian_sinks conf_t1_
    cargo test -p casparian_sinks_duckdb conf_t1_
    cargo test -p casparian_worker conf_t1_
    ;;
  t2)
    cargo test -p casparian conf_t2_ -- --ignored
    cargo test -p casparian_sinks conf_t2_ -- --ignored
    cargo test -p casparian_sinks_duckdb conf_t2_ -- --ignored
    cargo test -p casparian_worker conf_t2_ -- --ignored
    ;;
  *)
    echo "Unknown tier: ${tier}" >&2
    echo "Usage: $0 t0|t1|t2" >&2
    exit 1
    ;;
esac
