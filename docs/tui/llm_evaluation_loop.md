# TUI LLM Evaluation Loop

This is the fast, repeatable loop for getting structured UX feedback from an LLM using
stable snapshots plus optional real tmux captures.

## Quick Start

1. Generate bundle:
   ```bash
   ./scripts/tui-llm-bundle.sh
   ```

2. Open the bundle:
   - `.test_output/tui_llm_review.md`

3. Paste into the LLM using:
   - `docs/tui/llm_review_prompt.md`

4. Apply changes and regenerate:
   - `cargo test -p casparian` (insta diffs)
   - `./scripts/tui-llm-bundle.sh`

## What Gets Generated

- Deterministic snapshots: `.test_output/tui_snapshots/`
- Tmux captures: `.test_output/tui_tmux_captures/`
- Combined review doc: `.test_output/tui_llm_review.md`

## Notes

- Snapshots include a background mask and layout tree JSON for focus/selection hints.
- Tmux captures reflect real interaction frames and transition states.
- Keep data deterministic when adjusting snapshot builders (no `Local::now()` inside builders).

## Troubleshooting

- If the TUI binary is missing, build it first:
  ```bash
  cargo build -p casparian --release
  ```
- If schema changes, delete the dev DB as documented in `specs/meta/tui_testing_workflow.md`.
