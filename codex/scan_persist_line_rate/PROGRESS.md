---
plan: scan_persist_line_rate
last_updated: 2026-01-25
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
  - "2026-01-24: cargo test -p casparian passed; cargo bench -p casparian --bench scanner_perf timed out after 300s during build."
  - "2026-01-24: cargo bench -p casparian --bench scanner_perf timed out after 600s; partial results: full_scan batch_size=512 time 207.11-215.47ms (thrpt 23.205-24.141 Kelem/s), batch_size=2048 time 157.36-161.95ms (thrpt 30.875-31.774 Kelem/s), batch_size=10000 time 157.82-163.76ms (thrpt 30.533-31.682 Kelem/s); rescan batch_size=512 time 301.58-317.20ms (thrpt 15.763-16.579 Kelem/s), batch_size=2048 time 244.56-255.77ms (thrpt 19.549-20.445 Kelem/s)."
  - "2026-01-24: cargo check -p casparian-flow-ui failed: tauri distDir \"../dist\" missing (generate_context! panic)."
  - "2026-01-24: cargo test -p casparian passed; cargo bench -p casparian --bench scanner_perf completed (see benchmark log)."
  - "2026-01-25: Non-scan work: fixed bulk_insert_rows appender safety, added DuckDB sink column-order/reserved-word tests, quarantine list API, SQL guard keywords, sentinel DB URL resolution, rg fallback script. Ran: cargo test -p casparian_db; cargo test -p casparian_sinks --features sink-duckdb; cargo test -p casparian_sentinel. Did not run scanner_perf bench (not a scan milestone)."
  - "2026-01-25: Conformance suite Phase 1 (docs + scripts + Tier 0 tests). Ran cargo test -p casparian (pass). Ran cargo bench -p casparian --bench scanner_perf; walk_parallel 11.101-12.068 ms, full_scan batch_size=10000 140.86-146.02 ms, rescan batch_size=10000 203.92-218.73 ms, db_write batch_size=10000 75.048-87.134 ms. Next: add Tier 1 sink/worker tests + lineage hardening."
  - "2026-01-25: Conformance suite Phase 2 (Tier 1 sink/worker tests + lineage hardening). Ran cargo test -p casparian_sinks -p casparian_sinks_duckdb -p casparian_worker (pass). Ran cargo test -p casparian (pass). Ran cargo bench -p casparian --bench scanner_perf; walk_parallel 10.911-11.649 ms, full_scan batch_size=10000 146.94-159.52 ms, rescan batch_size=10000 215.92-323.18 ms, db_write batch_size=10000 68.488-72.230 ms. Next: implement rename-by-file_uid + conformance rename test."
  - "2026-01-25: Conformance suite Phase 3 (rename-by-file_uid + conformance rename test). Ran cargo test -p casparian (pass). Ran cargo bench -p casparian --bench scanner_perf; walk_parallel 16.399-33.960 ms, full_scan batch_size=10000 129.56-134.32 ms, rescan batch_size=10000 197.29-203.69 ms, db_write batch_size=10000 97.979-139.30 ms. Next: review conformance suite coverage or add Tier 2 ignored tests."
  - "2026-01-25: IntentState canonical serde (as_str/FromStr). Ran cargo test -p casparian_intent (pass). Ran cargo test -p casparian (pass). Ran cargo bench -p casparian --bench scanner_perf; walk_parallel 11.612-12.267 ms, full_scan batch_size=10000 126.98-131.00 ms, rescan batch_size=10000 221.52-370.93 ms, db_write batch_size=10000 68.557-74.061 ms. Next: remove pending_question_id surface + add MCP/protocol conversions + parse enums."
  - "2026-01-25: Session pending_question removal + MCP/protocol conversion helpers + enum parsing in sentinel. Ran cargo test -p casparian_intent -p casparian_sentinel -p casparian_mcp (pass). Ran cargo test -p casparian (pass). Ran cargo bench -p casparian --bench scanner_perf; walk_parallel 10.768-10.866 ms, full_scan batch_size=10000 132.92-146.39 ms, rescan batch_size=10000 188.79-201.75 ms, db_write batch_size=10000 63.909-65.333 ms. Next: verify tauri-ui usage of gate questions or add targeted UI tests."
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
- 2026-01-24: cargo bench -p casparian --bench scanner_perf timed out after 300s during build; no criterion output.
- 2026-01-24: cargo bench -p casparian --bench scanner_perf timed out after 600s; partial results for full_scan/rescan (see notes).
- 2026-01-24: Bench run (criterion): walk_parallel 10.056-10.265 ms (thrpt 487.07-497.24 Kelem/s); full_scan batch_size=512 143.42-155.24 ms (32.208-34.862 Kelem/s), batch_size=2048 117.21-125.08 ms (39.973-42.659 Kelem/s), batch_size=10000 111.62-122.27 ms (40.895-44.796 Kelem/s); rescan batch_size=512 183.74-186.91 ms (26.751-27.212 Kelem/s), batch_size=2048 158.69-160.23 ms (31.204-31.508 Kelem/s), batch_size=10000 159.61-162.31 ms (30.806-31.326 Kelem/s); db_write batch_size=256 136.78-140.54 ms, batch_size=1024 73.888-75.702 ms, batch_size=4096 58.573-60.501 ms, batch_size=10000 52.683-54.360 ms.

# Decisions / gotchas
- (append as discovered)

# Next step
- Decide whether to proceed with optional M4 (arena + packed records) given full_scan ~10.9x walk_only.
