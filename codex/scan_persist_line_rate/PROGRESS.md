---
plan: scan_persist_line_rate
last_updated: 2026-01-24
milestone: M3
step: M3.1
status: completed
baseline:
  scanner_walk_only: "walk_parallel: 10.480 ms (thrpt ~477.11 Kelem/s)"
  scanner_full_scan: "batch_size=10000: 114.40 ms (thrpt ~43.707 Kelem/s)"
  scanner_rescan: "batch_size=10000: 179.73 ms (thrpt ~27.819 Kelem/s)"
  scanner_db_write: "batch_size=10000: 56.895 ms (thrpt ~87.882 Kelem/s)"
notes:
  - "cargo test -p casparian fails: crates/casparian/src/cli/topic.rs:811 uses DbValue::from(topic) where topic is &&str"
  - "cargo bench -p casparian --bench scanner_perf initially timed out during build; later fails to compile: crates/casparian/src/cli/context.rs:8 unresolved import crate::scout"
  - "cargo bench -p casparian --bench scanner_perf also fails with missing workspace_id args in crates/casparian/src/cli/tui/app.rs (pattern_query calls)"
  - "2026-01-24: Implemented TUI snapshot harness + LLM bundle tooling; scan/persist milestones unchanged."
  - "Added ScanConfig.compute_stats to allow skipping stats in fast scans (default true)"
  - "cargo test -p casparian now runs but fails 1 snapshot test: cli::tui::snapshot_tests::test_snapshot_regressions"
  - "2026-01-24: Updated scan progress + telemetry + tracing/logging; refreshed TUI scanning progress snapshots."
  - "cargo test -p casparian now fails only in tests/fix_demo_e2e.rs (missing DuckDB tables fix_order_lifecycle/fix_parse_errors)."
  - "cargo bench -p casparian --bench scanner_perf timed out after ~120s; partial results: full_scan batch_size=512 time ~146.9-153.4ms (thrpt ~32.6-34.0 Kelem/s); batch_size=2048 time ~116.9-122.5ms (thrpt ~40.8-42.8 Kelem/s)."
---

# Goal
Get scanning + persistence as close to filesystem walk ("line rate") as possible without overengineering.

# How to resume
- Read this file first.
- Resume Codex thread if desired: `codex resume --last`
- Continue at the "Next step" section below.

# Milestones checklist
- [ ] M0: Baseline + instrumentation
- [ ] M1: Scanner hot-path cleanup (remove per-file `Utc::now`, faster normalize)
- [ ] M2: DuckDB upsert fast-path (no clones, transaction, stats without prequery)
- [ ] M3: Eliminate post-scan DB-wide folder-cache scan (streaming aggregates)
- [ ] M4 (optional): Arena + packed records
- [ ] M5 (optional): Path token interning / schema compression

# Benchmark log
- 2026-01-24: Bench run (criterion): walk_parallel 10.480 ms; full_scan batch_size=10000 114.40 ms; rescan batch_size=10000 179.73 ms; db_write batch_size=10000 56.895 ms. Full scan ~10.9x walk_only.
- 2026-01-23: M2–M3 applied (DuckDB upsert fast-path + streaming folder cache). Tests/bench still blocked by existing workspace-related compile errors in CLI/TUI.

# Decisions / gotchas
- (append as discovered)

# Next step
- Decide whether to proceed with optional M4 (arena + packed records) given full_scan ~10.9x walk_only.
