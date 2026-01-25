#!/bin/bash
# scripts/tui-fixture.sh - Create deterministic TUI fixtures (mock tree + source)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/tui-env.sh"

SOURCE_NAME="fixture_mock"
FIXTURE_ROOT="${CASPARIAN_HOME}/fixtures/mock_tree"
WORKSPACE_NAME=""

usage() {
    echo "Usage: $0 [--name NAME] [--root PATH] [--workspace NAME]"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --name)
            SOURCE_NAME="$2"
            shift 2
            ;;
        --root)
            FIXTURE_ROOT="$2"
            shift 2
            ;;
        --workspace)
            WORKSPACE_NAME="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown arg: $1"
            usage
            exit 1
            ;;
    esac
done

find_binary() {
    if [[ -x "$PROJECT_ROOT/target/release/casparian" ]]; then
        echo "$PROJECT_ROOT/target/release/casparian"
    elif [[ -x "$PROJECT_ROOT/target/debug/casparian" ]]; then
        echo "$PROJECT_ROOT/target/debug/casparian"
    else
        echo ""
    fi
}

BINARY="$(find_binary)"
if [[ -z "$BINARY" ]]; then
    echo "ERROR: casparian binary not found"
    exit 1
fi

# Build mock tree (reuses generator logic from tui-parallel-run.sh)
python3 - "$FIXTURE_ROOT" <<'PY'
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
if root.exists():
    for dirpath, dirnames, filenames in os.walk(root, topdown=False):
        for f in filenames:
            os.remove(Path(dirpath) / f)
        for d in dirnames:
            os.rmdir(Path(dirpath) / d)
    os.rmdir(root)

paths = []
for i in range(24):
    paths.append(f"health/hl7/admission_in/ADT_202401{(i % 28) + 1:02d}_{i}.hl7")
for i in range(22):
    paths.append(f"health/hl7/adm_in/ADT_202401{(i % 28) + 1:02d}_{i}.hl7")
for i in range(20):
    paths.append(f"health/hl7/lab_in/ORU_202401{(i % 28) + 1:02d}_{i}.hl7")
for i in range(18):
    paths.append(f"health/hl7/lab_out/ORU_2024-01-{(i % 28) + 1:02d}_{i}.hl7")
for i in range(16):
    paths.append(f"health/hl7/facility_a/inbound/ADT_202401{(i % 28) + 1:02d}_{i}.hl7")
for i in range(14):
    paths.append(f"health/hl7/facility_a/in/ADT_202401{(i % 28) + 1:02d}_{i}.hl7")

for i in range(28):
    paths.append(f"defense/mission_{i:03d}/satA/2024/01/15/telemetry_{i:04d}.csv")
for i in range(26):
    paths.append(f"defense/msn_{i:03d}/satA/2024/01/15/telemetry_{i:04d}.csv")
for i in range(18):
    paths.append(f"defense/mission_{i:03d}/satA/2024/01/15/telemetry_{i:04d}.json")
for i in range(16):
    paths.append(f"defense/patrol_{i:03d}/uav_01/2024/01/15/telemetry_{i:04d}.csv")
for i in range(16):
    paths.append(f"defense/ptl_{i:03d}/uav_01/2024/01/15/telemetry_{i:04d}.csv")
for i in range(12):
    paths.append(f"defense/mission_{i:03d}/satA/2024/01/15/imagery_{i:04d}.tif")

for i in range(1, 21):
    paths.append(f"finance/netsuite/exports/2024/01/fin_export_202401{i:02d}.csv")
for i in range(1, 19):
    paths.append(f"finance/ns/exports/2024/01/fin_export_202401{i:02d}.xlsx")
for i in range(1, 17):
    paths.append(f"finance/saved_search/ap_aging/2024/01/transactions_202401{i:02d}.csv")
for i in range(1, 17):
    paths.append(f"finance/saved_search/ap_age/2024/01/transactions_202401{i:02d}.xlsx")
for i in range(1, 13):
    paths.append(f"finance/payroll/exports/2024/01/payroll_202401{i:02d}.csv")
for i in range(1, 11):
    paths.append(f"finance/gl/exports/2024/01/general_ledger_202401{i:02d}.csv")

root.mkdir(parents=True, exist_ok=True)
for rel in paths:
    p = root / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text("fixture")
PY

if [[ -n "$WORKSPACE_NAME" ]]; then
    CASPARIAN_HOME="$CASPARIAN_HOME" "$BINARY" workspace create "$WORKSPACE_NAME" \
        || CASPARIAN_HOME="$CASPARIAN_HOME" "$BINARY" workspace set "$WORKSPACE_NAME"
fi

CASPARIAN_HOME="$CASPARIAN_HOME" "$BINARY" source add "$FIXTURE_ROOT" --name "$SOURCE_NAME" --recursive

if ! CASPARIAN_HOME="$CASPARIAN_HOME" "$BINARY" source ls | grep -q "$SOURCE_NAME"; then
    echo "ERROR: Fixture source '$SOURCE_NAME' not found in source list"
    exit 1
fi

echo "Fixture source '$SOURCE_NAME' added from $FIXTURE_ROOT"
