# TUI Phase Status (auto-saved)

## What changed
- Added UX lint tooling + state-graph rendering support.
- Added UI signature assertions to flow runner.
- Added modal scrim and context-aware footer hiding for modal states.
- Refactored Discover source/tag dropdowns into centered modal dialogs.
- Added Discover modal dialogs for filter, tag, bulk tag, and create source states.
- Added snapshot coverage for Discover filter/tag/bulk-tag/create-source dialogs.
- Phase 3: digits reserved for global nav; Discover keybinds moved to S/T (dropdowns), B (bulk tag), c (create source) with help/spec updates.
- Marked tmux TUI scripts as manual-only helpers.

## Files touched
- crates/casparian/src/cli/tui/ux_lint.rs (new)
- crates/casparian/src/cli/tui/state_graph.rs
- crates/casparian/src/cli/tui/flow.rs
- crates/casparian/src/cli/tui/flow_assert.rs
- crates/casparian/src/cli/tui/flow_runner.rs
- crates/casparian/src/cli/tui/mod.rs
- crates/casparian/src/cli/tui/ui.rs
- crates/casparian/src/main.rs
- scripts/tui-test.sh
- scripts/tui-test-workflow.sh

## Tests run
- cargo test -p casparian flow_assert
- cargo test -p casparian ui::
- cargo test -p casparian ui::tests::test_draw_discover_screen

## Next steps
1) Run UX atlas/lint: `casparian tui-state-graph --render --lint`
2) Refresh TUI snapshots if needed after keybinding/help text updates.

## Notes
- Discover manual-tag confirm + confirm-exit modals now render in overlay layer (post-scrim).
- Source/tag dropdowns now render as centered modal dialogs.
