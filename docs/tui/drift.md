# TUI Spec Drift Notes

Last updated: 2026-01-25

## Current drift vs specs/tui.md
- Rail order mismatch: UI rail shows Sessions before Settings, but app nav index maps Settings before Sessions, so rail selection/enter routes incorrectly for those two items.
- Global nav keys: [5] Approvals is now wired, but command palette navigation suggestions still omit Query + Sessions.
- Keybinding conflicts: Jobs uses global-conflicting keys (pipeline summary toggle on `P`, clear filter on `0`), and Discover doesn’t consistently intercept `1/2/3` in all sub-states.
- Sessions state parsing: loader expects legacy state strings (e.g., interpret_intent) while manifest stores `IntentState::as_str()` values (e.g., `S0_INTERPRET_INTENT`).
- Workflow diagram: numeric parsing of state fails on `S0_*` labels, so progress styling is wrong.
- Command palette dispatch: `/approve` and `/query` are suggested, but the dispatcher doesn’t handle them; start-intent flow is stubbed and doesn’t create a session.

## Notes
These items will be addressed in the tui_refresh plan (M1–M4) to restore parity with the current backend and specs.
