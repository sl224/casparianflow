# V1 Delivery Checklist (Finance Trade Break Workbench)

Status: Draft
Owner: Product/Eng
Spec posture: Guidance, not law. Prefer PMF and runtime behavior; update docs fast.

Legend:
- [P0] Must ship for v1
- [P1] Strongly desired for v1
- [P2] Nice-to-have

Status key:
- DONE: Implemented in codebase
- PARTIAL: Exists but missing v1 requirements or not wired end-to-end
- GAP: Not implemented / not found
- UNKNOWN: Product/ops item not verified in code

## Product + PMF
- [ ] [P0] ICP confirmed: trade support / FIX ops workflows validated. Status: PARTIAL (doc evidence only). Refs: `docs/product/validated_personas.md`, `docs/v1_scope.md`
- [ ] [P0] Demo dataset + walkthrough scripted (trade break by ClOrdID). Status: GAP. Refs: `docs/v1_scope.md`
- [ ] [P0] Time-to-first-value <= 15 minutes on a fresh machine. Status: UNKNOWN (not measured).
- [ ] [P1] 3 pilots identified with clear success criteria and access to logs. Status: UNKNOWN (not tracked).

## Type System + Schema Contracts
- [ ] [P0] Canonical DataType supports primitives + Decimal + timestamp_tz. Status: GAP. Refs: `crates/casparian_protocol/src/types.rs:219`
- [ ] [P0] timestamp_tz requires explicit timezone; no silent UTC default. Status: GAP. Refs: `crates/casparian_protocol/src/types.rs:237`
- [ ] [P0] tzdb dependency pinned for deterministic parsing. Status: GAP. Refs: `crates/casparian_worker/Cargo.toml:47`
- [ ] [P0] DataType serde backward-compatible (string + object formats). Status: GAP. Refs: `crates/casparian_protocol/src/types.rs:219`
- [ ] [P0] Contract approvals reject List/Struct unless feature-flagged. Status: PARTIAL (blocked by missing types, no explicit gate). Refs: `crates/casparian_schema/src/approval.rs:418`
- [ ] [P1] Schema modes implemented: strict, allow_extra, allow_missing_optional. Status: GAP. Refs: `crates/casparian_schema/src/contract.rs:133`

## Validation + Quarantine
- [ ] [P0] Rust-side validation authoritative (types, nullability, tz, format). Status: GAP. Refs: `crates/casparian_worker/src/worker.rs:768`
- [ ] [P0] Quarantine policy implemented: allow_quarantine + thresholds. Status: GAP. Refs: `crates/casparian_worker/src/worker.rs:531`
- [ ] [P0] Quarantine schema includes _error_msg, _violation_type, _cf_job_id, and one of _source_row/_output_row_index. Status: GAP. Refs: `crates/casparian_worker/src/worker.rs:768`
- [ ] [P1] Optional raw row data capture (configurable, default off in prod). Status: PARTIAL (storage has raw_data, not wired). Refs: `crates/casparian/src/storage/sqlite.rs:777`
- [ ] [P1] Quarantine config cascade (system -> global -> contract -> CLI). Status: GAP.

## Lineage
- [ ] [P0] Inject lineage columns: _cf_job_id, _cf_source_hash, _cf_processed_at, _cf_parser_version. Status: PARTIAL (function exists, not wired). Refs: `crates/casparian_sinks/src/lib.rs:1011`, `crates/casparian_worker/src/worker.rs:719`
- [ ] [P0] Support __cf_row_id if present for quarantine source row mapping. Status: GAP.
- [ ] [P1] Lineage warnings emitted when unavailable. Status: GAP.

## Job Status + Output Semantics
- [ ] [P0] JobStatus supports PartialSuccess with per-output status tracking. Status: GAP. Refs: `crates/casparian_protocol/src/types.rs:510`
- [ ] [P0] CompletedWithWarnings mapped to PartialSuccess for compatibility. Status: GAP. Refs: `crates/casparian_protocol/src/types.rs:510`
- [ ] [P1] Per-output quarantine metrics exposed in job receipt. Status: PARTIAL (total only). Refs: `crates/casparian_worker/src/worker.rs:531`

## Contract Identity + Storage
- [ ] [P0] scope_id derived from parser_id + parser_version + output_name. Status: GAP. Refs: `crates/casparian_schema/src/approval.rs:429`
- [ ] [P0] logic_hash stored with contracts for advisory warnings. Status: GAP. Refs: `crates/casparian_schema/src/storage.rs:62`
- [ ] [P1] Contract storage includes quarantine_config_json. Status: GAP. Refs: `crates/casparian_schema/src/storage.rs:62`
- [ ] [P1] Contract migration plan documented for existing scopes. Status: GAP (no doc found).

## Preview + Approval Flow
- [ ] [P0] Preview shows inferred schema and sample rows (CLI). Status: DONE. Refs: `crates/casparian/src/cli/preview.rs:82`
- [ ] [P1] Schema intent vs observed diff surfaced (Parser Bench). Status: GAP.
- [ ] [P1] Approval flow writes versioned contracts. Status: PARTIAL (versioning exists, scope_id derivation is wrong). Refs: `crates/casparian_schema/src/storage.rs:310`, `crates/casparian_schema/src/approval.rs:429`

## Execution Pipeline
- [ ] [P0] Worker splits valid vs quarantine based on Rust validation (not just _cf_row_error). Status: GAP. Refs: `crates/casparian_worker/src/worker.rs:768`
- [ ] [P0] Quarantine outputs written alongside main output (file or table). Status: PARTIAL (parser-driven quarantine only). Refs: `crates/casparian_worker/src/worker.rs:692`
- [ ] [P1] Multi-output handling applies per-output status rules. Status: GAP. Refs: `crates/casparian_worker/src/worker.rs:528`

## Storage + Sinks
- [ ] [P0] Parquet output stable and validated for Decimal/timestamp_tz. Status: GAP. Refs: `crates/casparian_protocol/src/types.rs:219`
- [ ] [P0] DuckDB sink supported for queryable outputs. Status: PARTIAL (type coverage incomplete). Refs: `crates/casparian_sinks/src/lib.rs:608`
- [ ] [P1] Quarantine table naming: {output}_quarantine for DB sinks. Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:705`

## CLI/TUI
- [ ] [P0] CLI run/preview/scan workflows stable. Status: PARTIAL (implemented, dev-mode). Refs: `crates/casparian/src/cli/run.rs:1`, `crates/casparian/src/cli/preview.rs:82`, `crates/casparian/src/cli/scan.rs:136`
- [ ] [P0] Quarantine summary visible in CLI/TUI. Status: GAP.
- [ ] [P1] Jobs view shows status + lineage. Status: GAP.

## Testing + QA
- [ ] [P0] Unit tests: timestamp_tz validation + Decimal coercion rules. Status: GAP.
- [ ] [P0] E2E: parse FIX logs -> lifecycle table -> query by ClOrdID. Status: GAP.
- [ ] [P1] Performance: 10M row log test with quarantine under threshold. Status: GAP.

## Documentation + Enablement
- [ ] [P0] Quickstart with demo FIX dataset and CLI commands. Status: GAP.
- [ ] [P0] Pilot onboarding guide (inputs, expected outputs, support path). Status: GAP.
- [ ] [P1] Troubleshooting guide for quarantine and schema failures. Status: GAP.
