#!/usr/bin/env bash
set -euo pipefail

select_star=$(rg -n "SELECT \*" crates tauri-ui/src-tauri/src --glob '!review_bundle/**' --glob '!**/tests/**' --glob '!**/test/**' || true)
if [[ -n "$select_star" ]]; then
  select_star=$(printf '%s\n' "$select_star" | rg -v -e "crates/casparian_db/src/sql_guard.rs" -e "crates/casparian_mcp/src/tools/query.rs" -e "tauri-ui/src-tauri/src/tests.rs" -e "crates/casparian/src/cli/tui/snapshot_states.rs" -e "crates/casparian_db/src/lib.rs" || true)
fi
if [[ -n "$select_star" ]]; then
  echo "Unexpected SELECT * usage:" >&2
  echo "$select_star" >&2
  exit 1
fi

returning_star=$(rg -n "RETURNING \*|\bSELECT\\s+\\w+\\.\\*" crates --glob '!review_bundle/**' --glob '!**/tests/**' --glob '!**/test/**' || true)
if [[ -n "$returning_star" ]]; then
  returning_star=$(printf '%s\n' "$returning_star" | rg -v -e "crates/casparian_db/src/sql_guard.rs" -e "crates/casparian_mcp/src/tools/query.rs" -e "tauri-ui/src-tauri/src/tests.rs" -e "crates/casparian/src/cli/tui/snapshot_states.rs" -e "crates/casparian_db/src/lib.rs" || true)
fi
if [[ -n "$returning_star" ]]; then
  echo "Unexpected RETURNING * or q.* usage:" >&2
  echo "$returning_star" >&2
  exit 1
fi
