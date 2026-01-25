---
plan: scout_remap_and_scan_engine
last_updated: 2026-01-25
milestone: M5
step: M5.1
status: completed
baseline:
  scanner_perf: "walk_parallel 11.566-11.914 ms; full_scan batch_size=10000 142.72-148.63 ms; rescan batch_size=10000 208.62-222.09 ms; db_write batch_size=10000 71.612-80.904 ms"
  perf_fixture_scan: "in-process 1000001 files in 183703 ms (5443.57 files/s); subprocess 1000001 files in 112556 ms (8884.48 files/s)"
notes:
  - "M4: Added ScanEngine trait with InProcessEngine/SubprocessEngine + wire framing + casparian-scout-scan binary."
  - "Fixed subprocess bytes_scanned accounting for flushed batches."
  - "Perf fixture: /tmp/casparian_perf_fixture (marker includes 1,000,000 files + marker file)."
  - "Perf DBs: /tmp/casparian_perf_inprocess.duckdb; /tmp/casparian_perf_subprocess2.duckdb."
  - "Ran cargo test -p casparian (pass). Ran cargo bench -p casparian --bench scanner_perf (see benchmark log)."
---

# Goal
Implement exec_path remapping + scout crate split + optional subprocess scan engine with perf tracking.

# Milestones checklist
- [x] M0: Perf CLI + fixture generator + baseline runs
- [x] M1: exec_path schema/types/DB + CLI surface (no migrations; reset DB)
- [x] M2: Sentinel dispatch remap join + tests
- [x] M3: Split scout into casparian_scout crate, keep API stable
- [x] M4: Optional subprocess scan engine + perf toggle
- [x] M5: Re-run perf/bench, verify no regressions

# Benchmark log
- 2026-01-25: cargo bench -p casparian --bench scanner_perf: walk_parallel 11.566-11.914 ms; full_scan batch_size=10000 142.72-148.63 ms; rescan batch_size=10000 208.62-222.09 ms; db_write batch_size=10000 71.612-80.904 ms (db_write batch_size=4096 regressed).
- 2026-01-25: perf scan fixture (1,000,000 files + marker) — in-process: duration_ms 183703, files_per_sec 5443.57; subprocess: duration_ms 112556, files_per_sec 8884.48.

# Decisions / gotchas
- Pre‑v1 rule: no migrations. Schema changes require DB reset.

# Next step
- Decide whether to keep subprocess engine experimental or allow opt-in by default (subprocess was faster on local SSD fixture; re-check on SMB/NFS).
