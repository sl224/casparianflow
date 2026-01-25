#!/bin/bash
# Shared environment setup for TUI scripts.

if [[ -n "${__CASPARIAN_TUI_ENV_LOADED:-}" ]]; then
    return 0
fi
__CASPARIAN_TUI_ENV_LOADED=1

__CASPARIAN_TUI_HOME_CREATED=0
if [[ -z "${CASPARIAN_HOME:-}" ]]; then
    CASPARIAN_HOME="$(mktemp -d -t casparian_tui.XXXXXX)"
    export CASPARIAN_HOME
    __CASPARIAN_TUI_HOME_CREATED=1
fi

echo "CASPARIAN_HOME=${CASPARIAN_HOME}"

__casparian_tui_cleanup() {
    if [[ "${KEEP_TUI_HOME:-}" == "1" ]]; then
        return 0
    fi
    if [[ "${__CASPARIAN_TUI_HOME_CREATED}" == "1" && -n "${CASPARIAN_HOME:-}" ]]; then
        rm -rf "${CASPARIAN_HOME}"
    fi
}

trap __casparian_tui_cleanup EXIT
