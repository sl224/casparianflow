# V1 Delivery Checklist (DFIR Artifact Workbench)

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
- [ ] [P0] ICP confirmed: DFIR consultant / artifact analysis workflows validated. Status: PARTIAL (doc evidence only). Refs: `strategies/dfir.md`, `docs/v1_scope.md`
- [ ] [P0] Demo dataset + walkthrough scripted (EVTX timeline query). Status: GAP. Refs: `docs/v1_scope.md`
- [ ] [P0] Time-to-first-value <= 15 minutes on a fresh machine. Status: UNKNOWN (not measured).
- [ ] [P1] 3 DFIR pilots identified with clear success criteria and access to artifacts. Status: UNKNOWN (not tracked).

## Type System + Schema Contracts
- [ ] [P0] Canonical DataType supports primitives + Decimal + timestamp_tz. Status: DONE. Refs: `crates/casparian_protocol/src/types.rs:463`
- [ ] [P0] timestamp_tz requires explicit timezone; no silent UTC default. Status: DONE. Refs: `crates/casparian_protocol/src/types.rs:690`
- [ ] [P0] tzdb dependency pinned for deterministic parsing. Status: DONE. Refs: `crates/casparian_protocol/Cargo.toml`
- [ ] [P0] DataType serde supports shorthand strings (primitives) and object form (parameterized). Status: DONE. Refs: `crates/casparian_protocol/src/types.rs:517`
- [ ] [P0] Contract approvals reject List/Struct unless feature-flagged. Status: DONE. Refs: `crates/casparian_schema/src/approval.rs:534`
- [ ] [P1] Schema modes implemented: strict, allow_extra, allow_missing_optional. Status: GAP. Refs: `crates/casparian_schema/src/contract.rs`

## Validation + Quarantine
- [ ] [P0] Rust-side validation authoritative (types, nullability, tz, format). Status: DONE (format validated for temporal string fields). Refs: `crates/casparian_worker/src/schema_validation.rs:214`, `crates/casparian_worker/src/schema_validation.rs:355`
- [ ] [P0] Quarantine policy implemented: allow_quarantine + thresholds. Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:772`, `crates/casparian_worker/src/worker.rs:1054`
- [ ] [P0] Quarantine schema includes _error_msg, _violation_type, _cf_job_id, and one of _source_row/_output_row_index. Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:1542`, `crates/casparian_worker/src/worker.rs:1569`
- [ ] [P1] Optional raw row data capture (configurable, default off in prod). Status: PARTIAL (storage has raw_data, not wired). Refs: `crates/casparian/src/storage/sqlite.rs:842`
- [ ] [P1] Quarantine config cascade (system -> global -> contract -> CLI). Status: GAP.

## Lineage
- [ ] [P0] Inject lineage columns: _cf_job_id, _cf_source_hash, _cf_processed_at, _cf_parser_version. Status: DONE. Refs: `crates/casparian_sinks/src/lib.rs:1038`, `crates/casparian_worker/src/worker.rs:1366`
- [ ] [P0] Support __cf_row_id if present for quarantine source row mapping. Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:1571`
- [ ] [P1] Lineage warnings emitted when unavailable. Status: DONE (logs + metrics). Refs: `crates/casparian_worker/src/worker.rs:1580`, `crates/casparian_worker/src/worker.rs:823`

## Job Status + Output Semantics
- [ ] [P0] JobStatus supports PartialSuccess with per-output status tracking. Status: DONE (per-output metrics). Refs: `crates/casparian_protocol/src/types.rs:1029`, `crates/casparian_worker/src/worker.rs:709`, `crates/casparian_worker/src/worker.rs:823`
- [ ] [P0] CompletedWithWarnings treated as success if encountered; v1 emits PartialSuccess. Status: DONE. Refs: `crates/casparian_sentinel/src/sentinel.rs:536`, `crates/casparian_protocol/src/types.rs:1062`
- [ ] [P1] Per-output quarantine metrics exposed in job receipt. Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:833`

## Contract Identity + Storage
- [ ] [P0] scope_id derived from parser_id + parser_version + output_name. Status: DONE. Refs: `crates/casparian_schema/src/approval.rs:436`
- [ ] [P0] logic_hash stored with contracts for advisory warnings. Status: DONE. Refs: `crates/casparian_schema/src/storage.rs:74`
- [ ] [P1] Contract storage includes quarantine_config_json. Status: GAP. Refs: `crates/casparian_schema/src/storage.rs:74`
- [ ] [P1] Contract migration plan documented for existing scopes. Status: GAP (no doc found).

## Preview + Approval Flow
- [ ] [P0] Preview shows inferred schema and sample rows (CLI). Status: DONE. Refs: `crates/casparian/src/cli/preview.rs:82`
- [ ] [P1] Schema intent vs observed diff surfaced (Parser Bench). Status: GAP.
- [ ] [P1] Approval flow writes versioned contracts. Status: PARTIAL (amendment flow increments versions; approval path always starts at version 1 and does not auto-bump on re-approval). Refs: `crates/casparian_schema/src/amendment.rs:620`, `crates/casparian_schema/src/approval.rs:490`

## Execution Pipeline
- [ ] [P0] Worker splits valid vs quarantine based on Rust validation (not just _cf_row_error). Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:1477`
- [ ] [P0] Quarantine outputs written alongside main output (file or table). Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:1384`
- [ ] [P1] Multi-output handling applies per-output status rules. Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:709`, `crates/casparian_worker/src/worker.rs:1115`

## Storage + Sinks
- [ ] [P0] Parquet output stable and validated for Decimal/timestamp_tz. Status: DONE (roundtrip test covers Decimal + timestamp_tz). Refs: `crates/casparian_sinks/src/lib.rs:1185`
- [ ] [P0] DuckDB sink supported for queryable outputs. Status: DONE (v1 types covered). Refs: `crates/casparian_sinks/src/lib.rs:620`, `crates/casparian_sinks/src/lib.rs:1224`
- [ ] [P1] Quarantine table naming: {output}_quarantine for DB sinks. Status: DONE. Refs: `crates/casparian_worker/src/worker.rs:1385`

## CLI/TUI
- [ ] [P0] CLI run/preview/scan workflows stable. Status: PARTIAL (implemented, dev-mode). Refs: `crates/casparian/src/cli/run.rs:1`, `crates/casparian/src/cli/preview.rs:82`, `crates/casparian/src/cli/scan.rs:136`
- [ ] [P0] Quarantine summary visible in CLI/TUI. Status: DONE. Refs: `crates/casparian/src/cli/jobs.rs:517`, `crates/casparian/src/cli/tui/ui.rs:3687`
- [ ] [P1] Jobs view shows status + lineage. Status: PARTIAL (status shown; lineage not yet visible).

## Testing + QA
- [ ] [P0] Unit tests: timestamp_tz validation + Decimal coercion rules. Status: GAP.
- [ ] [P0] E2E: parse EVTX files -> events table -> query by event_id/time range. Status: GAP.
- [ ] [P1] Performance: multi-GB EVTX archive test with quarantine under threshold. Status: GAP.

## Documentation + Enablement
- [ ] [P0] Quickstart with demo EVTX dataset and CLI commands. Status: GAP.
- [ ] [P0] Productized onboarding sprint kit (scope, acceptance criteria, handoff checklist). Status: GAP.
- [ ] [P1] Troubleshooting guide for quarantine and schema failures. Status: GAP.

## MCP Server (AI Integration)

### P0: Core Infrastructure
- [ ] [P0] MCP crate structure (`crates/casparian_mcp/`). Status: GAP. Refs: `docs/execution_plan_mcp.md`
- [ ] [P0] MCP server binary/entrypoint (`casparian mcp serve`). Status: GAP.
- [ ] [P0] Security: Path allowlist + canonicalization (deny traversal). Status: GAP.
- [ ] [P0] Security: Output budget enforcement (1MB default). Status: GAP.
- [ ] [P0] Security: Redaction policy (hash mode default). Status: GAP.
- [ ] [P0] Security: Audit logging (`~/.casparian_flow/mcp_audit.log`). Status: GAP.

### P0: Job Subsystem (Non-blocking operations)
- [ ] [P0] Job manager + persistence (`~/.casparian_flow/mcp_jobs.json`). Status: GAP.
- [ ] [P0] Job concurrency control (1 concurrent default). Status: GAP.
- [ ] [P0] Job timeout + stall detection (30min / 30s). Status: GAP.

### P0: Approval Subsystem (Non-blocking human gates)
- [ ] [P0] Approval manager + file-based storage (`~/.casparian_flow/approvals/`). Status: GAP.
- [ ] [P0] CLI: `casparian approvals list/approve/reject`. Status: GAP.
- [ ] [P0] Approval TTL (1 hour) + auto-cleanup. Status: GAP.

### P0: Read-Only Tools
- [ ] [P0] MCP tool: `casparian_plugins` (list available parsers). Status: GAP.
- [ ] [P0] MCP tool: `casparian_scan` (scan directory, hash_mode opt-in). Status: GAP.
- [ ] [P0] MCP tool: `casparian_preview` (preview with redaction). Status: GAP.
- [ ] [P0] MCP tool: `casparian_query` (read-only DuckDB + SQL allowlist). Status: GAP.

### P0: Job-Based Tools
- [ ] [P0] MCP tool: `casparian_backtest_start` (returns job_id immediately). Status: GAP.
- [ ] [P0] MCP tool: `casparian_run_request` (creates approval request). Status: GAP.
- [ ] [P0] MCP tool: `casparian_job_status` (poll progress/result). Status: GAP.
- [ ] [P0] MCP tool: `casparian_job_cancel` (cancel running job). Status: GAP.
- [ ] [P0] MCP tool: `casparian_job_list` (list recent jobs). Status: GAP.
- [ ] [P0] MCP tool: `casparian_approval_status` (check approval). Status: GAP.
- [ ] [P0] MCP tool: `casparian_approval_list` (list pending). Status: GAP.

## AI Iteration Support (ADR-021) - P1

### P1: EphemeralSchemaContract
- [ ] [P1] EphemeralSchemaContract struct (per-output schemas). Status: GAP. Refs: `docs/decisions/ADR-021-ai-agentic-iteration-workflow.md`
- [ ] [P1] Schema canonicalization + per-output hashing. Status: GAP.
- [ ] [P1] Local persistence (`~/.casparian_flow/ai/contracts/`). Status: GAP.

### P1: ViolationContext
- [ ] [P1] ViolationContext with samples + distributions (redacted). Status: GAP. Refs: `docs/schema_rfc.md`
- [ ] [P1] SuggestedFix generation from violation patterns. Status: GAP.
- [ ] [P1] Integration into backtest job results. Status: GAP.

### P1: Schema Tools
- [ ] [P1] MCP tool: `casparian_schema_propose` (ephemeral schema). Status: GAP.
- [ ] [P1] MCP tool: `casparian_schema_promote` (gated, generates code). Status: GAP.

### P1: Enhanced Progress
- [ ] [P1] Per-output metrics in job progress. Status: GAP.
- [ ] [P1] Stall detection (30s no progress → stalled). Status: GAP. Refs: `specs/jobs_progress.md` §13
