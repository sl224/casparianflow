# TUI Snapshot + Review Loop

This document describes the end-to-end flow for deterministic TUI snapshots,
optional tmux captures, and the LLM review bundle.

## Goals

- Catch layout regressions early with deterministic TestBackend snapshots.
- Provide LLM-friendly bundles that combine static frames and real interaction frames.
- Keep the loop fast: generate, review, adjust, repeat.

## Artifacts (All Ignored)

- `.test_output/tui_snapshots/`
  - `<case>/<case>__WxH.txt` (plain)
  - `<case>/<case>__WxH.mask.txt` (bg mask)
  - `<case>/<case>__WxH.layout.json` (layout tree)
  - `<case>/<case>.meta.json` (case metadata)
  - `tui_snapshots.md` (bundle of all static frames)
- `.test_output/tui_tmux_captures/`
  - `<scenario>/NN_<label>.txt` (captured frames)
  - `<scenario>/<scenario>.md` (scenario bundle)
- `.test_output/tui_llm_review.md`
  - combined static + tmux bundle

## Deterministic Snapshot Flow (TestBackend)

1. Render each canonical snapshot case to an off-screen buffer.
2. Dump two views per frame:
   - Plain text grid
   - Background mask (focus/selection hints)
3. Emit a lightweight layout tree JSON for each frame.
4. Run snapshot regression tests with insta.

## Canonical Snapshot Cases

Canonical states are defined in:
- `crates/casparian/src/cli/tui/snapshot_states.rs`

Rules:
- No DB access or background threads.
- No `Local::now()` in builders; use fixed timestamps.
- Keep cases small and focused (one UX state per case).

## Commands

Generate deterministic snapshots (LLM bundle):
```bash
cargo run -p casparian -- tui-snapshots --out .test_output/tui_snapshots
```

Run snapshot regression tests (insta):
```bash
cargo test -p casparian
```

Force snapshot updates (if layouts changed intentionally):
```bash
INSTA_UPDATE=always cargo test -p casparian
```

## Tmux Capture Flow (Optional)

The tmux scenario captures real interaction frames for the critical path.

Generate tmux captures:
```bash
./scripts/tui-llm-capture.sh --out .test_output/tui_tmux_captures
```

## LLM Review Bundle

Generate a single combined bundle:
```bash
./scripts/tui-llm-bundle.sh
```

Output:
- `.test_output/tui_llm_review.md`

Use the prompt template in:
- `docs/tui/llm_review_prompt.md`

## Loop Checklist

1. Make UI changes.
2. Run `cargo test -p casparian`.
3. If snapshots changed intentionally, run `INSTA_UPDATE=always cargo test -p casparian`.
4. Run `./scripts/tui-llm-bundle.sh`.
5. Review in LLM + apply feedback.
6. Repeat.

## Troubleshooting

- If the TUI binary is missing, build it:
  ```bash
  cargo build -p casparian --release
  ```
- If schema changes affect snapshot builders, delete the dev DB:
  ```bash
  rm -f ~/.casparian_flow/casparian_flow.duckdb
  ```
