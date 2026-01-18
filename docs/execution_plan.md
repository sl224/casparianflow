# V1 Execution Plan (Synthesized)

Status: Working Draft (directional, not binding)
Purpose: Align v1 delivery with PMF priorities and current code, while preserving the legacy execution plan for reference.

Inputs:
- docs/v1_scope.md
- docs/v1_checklist.md
- docs/schema_rfc.md (directional)
- docs/execution_plan.md (legacy, below)

Constraints:
- Disk space is limited; avoid large artifacts and long-running local builds.
- v1 is finance-first (trade break workbench); keep scope tight.
- DuckDB + Parquet only for v1 sinks.

Current status snapshot (from checklist + recent code work):
- DataType extended (Decimal, timestamp_tz, list/struct) with backward-compatible serde; tzdb validation added.
- DuckDB sink supports DECIMAL + TIMESTAMPTZ.
- Worker validation/quarantine policy + quarantine schema metadata are implemented; lineage injection + fallback warnings/metrics are implemented.
- JobStatus PartialSuccess wired; scope_id derivation remains a gap.
- CLI preview exists; Parser Bench and Jobs view need alignment with v1 semantics.

## Parallel Tracks (v1)
Aim to run these in parallel with minimal cross-blocking, using feature flags where needed.

### Track 1: Validation + Quarantine Core (P0)
Owner: Eng Core
Deliverables:
- Rust-side schema enforcement (types/nullability/tz/format) and quarantine split.
- Quarantine policy (allow_quarantine + thresholds + per-output status).
- Quarantine schema with required fields: _error_msg, _violation_type, _cf_job_id, and _source_row or _output_row_index.
Dependencies: None; this is the critical path for v1 correctness.

### Track 2: Lineage + Job Status Semantics (P0)
Owner: Eng Core
Deliverables:
- Inject lineage columns into outputs.
- Support __cf_row_id when present; otherwise emit lineage warnings.
- JobStatus -> PartialSuccess with per-output metrics; keep CompletedWithWarnings as compat alias.
Dependencies: Can proceed in parallel with Track 1; integrate at the worker output boundary.

### Track 3: Contract Identity + Storage (P0)
Owner: Eng Core
Deliverables:
- scope_id = parser_id + parser_version + output_name.
- Store logic_hash with contracts for advisory warnings.
- Update schema contract storage to include quarantine_config_json.
Dependencies: Minimal; can land in parallel with Tracks 1/2. Requires migration plan.

### Track 4: CLI/TUI + Workflow (P1)
Owner: Eng Product
Deliverables:
- Parser Bench schema diff view (intent vs observed) and approval flow.
- Jobs view with quarantine summary + lineage visibility.
- CLI quickstart flow (scan -> preview -> run).
Dependencies: Needs Track 1/2 semantics to display real metrics.

### Track 5: Demo + PMF Enablement (P0)
Owner: Product/Eng
Deliverables:
- Small FIX demo dataset + walkthrough (trade break by ClOrdID).
- Pilot onboarding guide and success criteria.
- E2E test that mirrors demo flow.
Dependencies: Needs Track 1/2 to be meaningful; can draft docs early.

## Milestones (Suggested)
- M0: Type system + sink mapping complete (DONE).
- M1: Validation + quarantine policy in worker (Track 1).
- M2: Lineage + job status semantics (Track 2).
- M3: Contract identity + storage migration (Track 3).
- M4: CLI/TUI + demo workflow (Tracks 4/5).

## Disk Space Guidance
- Keep demo datasets small (<10MB). Prefer synthetic FIX logs.
- Avoid large Parquet artifacts in repo; write to temp dirs and clean up.
- Defer full 10M-row performance tests until storage is available.

---

# Legacy Execution Plan (Archived)

Purpose: Provide an end-to-end implementation plan for remaining phases/features, with enough context for another LLM to execute without split-brain.

## Scope
This plan covers:
1) Rust validation + quarantine pipeline
2) Safe Read SDK + parser templates
3) Lineage persistence + queries
4) Quarantine config + retention
5) Approval/audit governance
6) Postgres + MSSQL sinks and staging semantics

## Primary Specs / References
- docs/schema_rfc.md
- docs/schema_rfc_appendix_b
- specs/parser_schema_contract_rfc.md
- specs/pipelines.md
- specs/db.md
- specs/features/export.md
- docs/DUCKDB_MIGRATION_PLAN.md
- docs/ppg_spec.md

## Current Code Anchors (as of now)
- Bridge/shim: crates/casparian_worker/shim/bridge_shim.py, crates/casparian_worker/shim/casparian_types.py
- Worker execution: crates/casparian_worker/src/worker.rs
- Sinks: crates/casparian_sinks/src/lib.rs
- Storage / DB: crates/casparian/src/storage/*, crates/casparian_db/*
- CLI/TUI: crates/casparian/src/cli/*

## Phase 0: Alignment and Inventory
Goal: Map spec requirements to concrete code touch points and data model changes.

- Identify exact spec sections for each feature.
- Identify existing data structures in protocol and storage:
  - casparian_protocol::types (sink config, job status, lineage fields)
  - duckdb schema in casparian storage (jobs, artifacts, quarantine)
- Determine migration needs and backward compatibility (product not launched; no compat required).
- Decide where validation logic lives (worker vs library). Target: worker owns validation after bridge IPC, before sink write.

Output:
- Change list with specific files and new structs.
- Migration plan for new DB tables/columns (lineage, quarantine artifacts, approval state).

## Phase 1: Validation + Quarantine Pipeline
Goal: Validate Arrow batches against contract schemas and split into valid/quarantined outputs.

Tasks:
- Define validator API (Rust) that consumes RecordBatch + Contract, returns:
  - valid RecordBatch
  - quarantine RecordBatch with `_source_row`, `_cf_violation`, and optional raw data
- Implement shallow nested validation only (defer deep nesting to v1.1).
- Add lineage protocol:
  - Accept `__cf_row_id` Int64/UInt64; if present, use for `_source_row`
  - Else fallback to batch index with warning
- Enforce `allow_quarantine` default false (hard fail in prod).
- Ensure quarantine data writes to separate sink path (configurable dir).

Files likely touched:
- crates/casparian_worker/src/* (validation + routing)
- crates/casparian_sinks/src/lib.rs (quarantine sink support)
- casparian_protocol types for new fields
- docs/schema_rfc.md updates if needed

## Phase 2: Safe Read SDK + Parser Templates
Goal: Prevent precision loss before validation (Decimal and fixed-width data).

Tasks:
- Provide Safe Read helpers for pandas/polars:
  - CSV/Fixed width read with explicit dtype=str or Decimal mapping
  - Utilities for timezone parsing and validation
- Update parser generator templates to include safe read guidance / helpers.
- Add docs examples (Safe Read section).

Files likely touched:
- crates/casparian/src/ai/parser_lab/generator.rs
- crates/casparian_worker/shim/bridge_shim.py (if helper injection needed)
- docs/schema_rfc.md

## Phase 3: Lineage Persistence + Queries
Goal: Persist lineage metadata and expose it for auditing and query.

Tasks:
- Add storage tables/columns for:
  - lineage metadata per output artifact
  - quarantine artifacts + `_source_row`
- Implement API in storage layer for queries:
  - by job
  - by output artifact
  - by source file or parser version

Files likely touched:
- crates/casparian/src/storage/duckdb.rs
- crates/casparian_db schema migrations
- CLI/TUI screens for lineage view

## Phase 4: Quarantine Config + Retention
Goal: Support retention controls and separate directory.

Tasks:
- Implement config cascade (CLI flag, config file, DB overrides, default).
- Add `quarantine_dir` and retention settings to config.
- Ensure quarantine outputs are segregated by dir for retention policies.

Files likely touched:
- casparian_protocol types
- casparian_sentinel config flow
- CLI config handling

## Phase 5: Approval / Audit Governance
Goal: Formal approval workflow for schema contracts and audit trail.

Tasks:
- Add approval states + timestamps in DB
- Implement approve/reject operations
- Log approvals in audit log
- TUI/CLI commands to review and approve schema changes

Files likely touched:
- crates/casparian/src/storage/*
- crates/casparian/src/cli/*
- docs/ppg_spec.md

## Phase 6: Sink Parity (Postgres + MSSQL)
Goal: Add production sinks before ship, with staging + promotion semantics.

Tasks:
- Implement Postgres sink (batch insert, type mapping, table create).
- Implement MSSQL sink (bulk insert; type mapping).
- Staging -> promotion semantics consistent with non-transactional sinks.
- Update export spec and tests.

Files likely touched:
- crates/casparian_sinks/src/lib.rs (new sinks)
- crates/casparian_db/* (if shared infra)
- specs/features/export.md

## Testing Plan
- Unit tests for validation/quarantine split
- Concurrency tests for worker
- End-to-end tests covering:
  - Safe read + validation
  - Quarantine output creation
  - Lineage persistence queries
  - Sink writes (parquet/duckdb/csv/postgres/mssql)
- Determinism checks (tzdb pinned)

## Non-Goals (v1)
- Deep nested validation (defer to v1.1)
- Per-output sinks within a job
- Cross-sink atomic commits

## Implementation Order (Recommended)
1) Validation + quarantine pipeline
2) Lineage persistence
3) Safe Read SDK + templates
4) Approval/audit flow
5) Quarantine config + retention
6) Postgres + MSSQL sinks

---

# Unified Execution Plan + Team Allocation (Pipeline Phase)

## Constraints
- **Single DB only.** No dual DB usage.
- **DuckDB is the default DB.** Remove default SQLite references where applicable.
- **Structured filters only** for pipeline selection (no SQL in pipeline contract).
- **Fixed topology:** Materialize (optional) → Parse fan-out (required) → Export (optional).

## Current Progress (as of this doc)
- Added pipeline/selection data models and storage APIs.
- Added pipeline tables + indexes in SQLite job store (to be migrated to DuckDB control path).
- Added selection resolver (structured filters + logical date + watermark).

## Track A: Pipeline Foundation (Eng 1) (IN PROGRESS)
**Goal:** Pipeline data model + scheduler + CLI (DuckDB default).
**Files:**
- `crates/casparian/src/storage/traits.rs`
- `crates/casparian/src/storage/duckdb.rs` (transition path)
- `crates/casparian/src/cli/pipeline.rs` (new)
- `crates/casparian/src/main.rs` (CLI wiring)
**Deliverables:**
- `pipeline apply/run/backfill` (DONE)
- SelectionSnapshot creation + hashing + file join table inserts (DONE)
- Logical date support + backfill resume (DONE)
- Pipeline enqueueing + run status updates (DONE)
- Pipeline E2E test for enqueueing (DONE)

## Track B: Validation + Quarantine (Eng 2)
**Goal:** Worker-side validation split + quarantine outputs.
**Files:**
- `crates/casparian_worker/src/*`
- `crates/casparian_sinks/src/lib.rs`
- `casparian_protocol` types
**Deliverables:**
- Valid/quarantine split
- `completed_with_warnings` semantics

## Track C: Lineage + Jobs UI (Eng 4)
**Goal:** Queryable lineage + operator UI upgrades.
**Files:**
- `crates/casparian/src/storage/duckdb.rs` (query APIs)
- `crates/casparian/src/cli/tui/app.rs`
- `crates/casparian/src/cli/tui/ui.rs`
**Deliverables:**
- Query by file/snapshot/job
- Jobs UI shows logical date, snapshot hash, quarantine, throughput, stragglers

## Track D: Parser Bench Test Flows (Eng 3)
**Goal:** Finish Parser Bench workflows.
**Files:**
- `crates/casparian/src/cli/tui/app.rs`
- `crates/casparian/src/cli/tui/ui.rs`
**Deliverables:**
- `t` test, `n` quick test, `b` backtest flows + results panel

## Critical Refactor: DuckDB Control Plane (Blocking)
**Reason:** Sentinel uses `create_pool()` which rejects DuckDB. Must unify DB.
**Plan:**
1) Replace sentinel DB access with `DbConnection::open_duckdb()` (single-writer). (DONE)
2) Port job queue SQL calls to `DbConnection` (no sqlx pool). (DONE)
3) Remove default SQLite CLI args and docs that imply duckdb://. (DONE)

**Owners:** Eng 1 + Eng 2 (shared)  
**Dependency:** Blocks full pipeline integration.

## Parallelization
Can run in parallel:
- Track A, B, D immediately.
- Track C can start UI work but should wait for pipeline + status fields.
Blocking dependency:
- DuckDB control plane refactor must land before pipeline runs are end-to-end.
