#!/bin/bash
# Generate a full TUI snapshot bundle for all views and overlays.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

OUT_ROOT="${OUT_ROOT:-${ROOT_DIR}/.test_output}"
OUT_DIR="${OUT_DIR:-${OUT_ROOT}/tui_snapshots_all}"
ZIP_NAME="${ZIP_NAME:-tui_snapshots_all.zip}"

source "${SCRIPT_DIR}/tui-env.sh"

rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}"

cargo run -p casparian -- tui-snapshots --out "${OUT_DIR}"

(
    cd "${OUT_ROOT}"
    zip -qr "${ROOT_DIR}/${ZIP_NAME}" "$(basename "${OUT_DIR}")"
)

echo "Snapshot bundle written to ${ROOT_DIR}/${ZIP_NAME}"
