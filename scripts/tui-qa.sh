#!/bin/bash
# Run the full TUI QA loop: snapshots, UX lint, state graph, and flows.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
OUT_ROOT="${OUT_ROOT:-${ROOT_DIR}/.test_output}"
SNAPSHOT_DIR="${OUT_ROOT}/tui_snapshots"
UX_LINT_OUT="${OUT_ROOT}/tui_ux_lint"
STATE_GRAPH_OUT="${OUT_ROOT}/tui_state_graph"
FLOW_OUT="${OUT_ROOT}/tui_flows"

source "${SCRIPT_DIR}/tui-env.sh"

mkdir -p "${OUT_ROOT}"
rm -rf "${SNAPSHOT_DIR}" "${UX_LINT_OUT}" "${STATE_GRAPH_OUT}"

cargo run -p casparian -- tui-snapshots --out "${SNAPSHOT_DIR}"

cargo run -p casparian -- tui-ux-lint --in "${SNAPSHOT_DIR}" --mode snapshots --out "${UX_LINT_OUT}"

cargo run -p casparian -- tui-state-graph --render --lint --out "${STATE_GRAPH_OUT}"

shopt -s nullglob
flow_specs=("${ROOT_DIR}/specs/tui_flows/"*.json)
if [[ ${#flow_specs[@]} -eq 0 ]]; then
    echo "No flow specs found under ${ROOT_DIR}/specs/tui_flows"
else
    for flow in "${flow_specs[@]}"; do
        cargo run -p casparian -- tui-flow run "${flow}" --headless --out "${FLOW_OUT}"
    done
fi

echo "TUI QA artifacts written under ${OUT_ROOT}"
