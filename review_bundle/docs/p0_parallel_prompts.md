# P0 Parallel Work Prompts (v1)

Use one prompt per new Codex session. Each prompt includes worktree setup, scope,
and tests. Base branch is `rust`. Disk space is low: keep datasets small (<10MB),
avoid large builds, and prefer targeted tests.

General setup (run from repo root `/Users/shan/workspace/casparianflow`):
- `git worktree list` to see existing worktrees.
- `git worktree add ../<worktree> -b <branch>` for your task.
- `cd ../<worktree>` to work.
- Use `rg` for search; avoid destructive git commands.

---

## Prompt A: P0 Contracts (scope_id + logic_hash + List/Struct gate)

Worktree:
- `git worktree add ../casparianflow-p0-contracts -b p0/contracts`
- `cd ../casparianflow-p0-contracts`

Goal:
Implement v1 contract identity and storage:
- scope_id = sha256(parser_id + ":" + parser_version + ":" + output_name)
- store logic_hash with the contract (advisory only)
- reject List/Struct types at approval unless feature-flagged

Checklist items:
- Contract approvals reject List/Struct unless feature-flagged (P0)
- scope_id derived from parser_id + parser_version + output_name (P0)
- logic_hash stored with contracts (P0)

Context:
- `crates/casparian_schema/src/approval.rs` (`SchemaApprovalRequest`, `approve_schema`)
- `crates/casparian_schema/src/contract.rs` (`SchemaContract`)
- `crates/casparian_schema/src/storage.rs` (schema_contracts table + save/load)
- `docs/schema_rfc.md` section 6.4 (scope_id + logic_hash)

Tasks:
1) Extend `SchemaApprovalRequest` with `parser_id`, `parser_version`, and optional `logic_hash`.
2) Add a scope_id derivation helper (sha256 of `parser_id:parser_version:output_table_name`).
3) Update `approve_schema()` to compute scope_id from request + output_table_name.
   - If multiple outputs exist, decide on v1 behavior (either error or split contracts);
     document the choice in code comments and tests.
4) Add List/Struct approval gate in `validate_approved_schema()` unless
   `allow_nested_types` is true (add flag to request if needed).
5) Add `logic_hash: Option<String>` to `SchemaContract`, persist in
   `schema_contracts` table, and load it back.
   - Add migration for existing DBs (ALTER TABLE if column missing).
6) Update tests in `crates/casparian_schema/tests/e2e_contracts.rs`
   and any approval/storage tests to cover new fields and List/Struct rejection.

Acceptance:
- `cargo test -p casparian_schema` passes.
- Scope_id uses sha256 of parser_id/version/output_name.
- logic_hash round-trips through storage.
- List/Struct approval is blocked unless explicitly allowed.

---

## Prompt B: P0 Validation Format Enforcement (schema_validation)

Worktree:
- `git worktree add ../casparianflow-p0-validation-format -b p0/validation-format`
- `cd ../casparianflow-p0-validation-format`

Goal:
Enforce per-column `format` in Rust validation (dates/timestamps/time) and
close the gap where format is parsed but unused.

Checklist items:
- Rust-side validation authoritative (types/nullability/tz/format) (P0)

Context:
- `crates/casparian_worker/src/schema_validation.rs`
- `casparian_protocol::DataType::validate_string()` in `crates/casparian_protocol/src/types.rs`

Tasks:
1) Implement format-aware validation for string columns when `format` is provided:
   - Date/Time/Timestamp/TimestampTz should parse using the format string.
   - If format is absent, keep current behavior.
2) Decide behavior for non-string arrays (e.g., already typed columns):
   - Either skip format validation or validate by formatting values back to string.
   - Keep it simple and document the choice.
3) Add unit tests for format failures and success paths.

Acceptance:
- `cargo test -p casparian_worker` passes.
- Format string is enforced when present and fails invalid values.

---

## Prompt C: P0 Parquet Decimal + timestamp_tz Validation

Worktree:
- `git worktree add ../casparianflow-p0-parquet-decimal -b p0/parquet-decimal`
- `cd ../casparianflow-p0-parquet-decimal`

Goal:
Validate Parquet output for Decimal and timestamp_tz types with tests.

Checklist items:
- Parquet output stable and validated for Decimal/timestamp_tz (P0)

Context:
- `crates/casparian_sinks/src/lib.rs` (ParquetSink)

Tasks:
1) Add a Parquet sink test that writes Decimal128 + Timestamp(TZ) columns.
2) Read the Parquet file back (Arrow/Parquet reader) and assert:
   - schema types preserved (Decimal128, Timestamp with timezone)
   - values round-trip for non-null rows
3) Keep dataset tiny (<= 3 rows) to avoid disk usage.

Acceptance:
- `cargo test -p casparian_sinks` passes.

---

## Prompt D: P0 Quarantine Summary in CLI/TUI

Worktree:
- `git worktree add ../casparianflow-p0-quarantine-ui -b p0/quarantine-ui`
- `cd ../casparianflow-p0-quarantine-ui`

Goal:
Expose quarantine summary in CLI and TUI job views.

Checklist items:
- Quarantine summary visible in CLI/TUI (P0)
- Jobs view shows status + lineage (P1, partial)

Context:
- CLI: `crates/casparian/src/cli/jobs.rs`, `crates/casparian/src/cli/job.rs`,
  JSON tests in `crates/casparian/tests/cli_jobs_json.rs`
- TUI: `crates/casparian/src/cli/tui/ui.rs`, job detail panel
- Metrics keys: `quarantine_rows` and per-output metrics inserted in worker
  (`crates/casparian_worker/src/worker.rs`)

Tasks:
1) Add quarantine row counts to CLI job list and job detail outputs.
2) Add quarantine summary in TUI job list/detail (count + label).
3) Update CLI JSON tests if needed (ensure extra fields don’t break parsing).

Acceptance:
- `cargo test -p casparian --tests` passes (or targeted CLI tests).
- Quarantine counts visible in CLI + TUI.

---

## Prompt E: P0 EVTX Demo Dataset + Quickstart + E2E

Worktree:
- `git worktree add ../casparianflow-p0-evtx-demo -b p0/evtx-demo`
- `cd ../casparianflow-p0-evtx-demo`

Goal:
Add a small EVTX demo dataset and a runnable walkthrough, and wire an E2E test
that queries by event_id/time range.

Checklist items:
- Demo dataset + walkthrough scripted (P0)
- Quickstart with demo EVTX dataset and CLI commands (P0)
- E2E: parse EVTX files -> events table -> query by event_id/time range (P0)

Context:
- `docs/v1_scope.md`
- Existing tests in `crates/casparian/tests/*` for E2E patterns

Tasks:
1) Add a tiny EVTX fixture (<= 1MB) under `docs/demo/dfir/evtx/` or `tests/fixtures/evtx/`.
2) Write a quickstart doc showing scan -> preview -> run -> query by event_id/time range.
3) Add an E2E test that uses the fixture and asserts:
   - output table exists
   - query by event_id/time range returns expected rows

Acceptance:
- New doc readable and uses the fixture path.
- E2E test passes locally with tiny fixture.

---

## Prompt F: P0 CLI Run/Preview/Scan Stability Audit

Worktree:
- `git worktree add ../casparianflow-p0-cli-stability -b p0/cli-stability`
- `cd ../casparianflow-p0-cli-stability`

Goal:
Make CLI run/preview/scan stable for v1 (no dev-only gating, clear exit codes).

Checklist items:
- CLI run/preview/scan workflows stable (P0)

Context:
- `crates/casparian/src/cli/run.rs`
- `crates/casparian/src/cli/preview.rs`
- `crates/casparian/src/cli/scan.rs`
- CLI tests in `crates/casparian/tests/*`

Tasks:
1) Audit run/preview/scan for dev-only flags or unstable defaults.
2) Ensure non-zero exit codes on failures and consistent JSON output when `--json`.
3) Add/adjust tests to cover the stabilized behavior.

Acceptance:
- Targeted CLI tests pass (`cargo test -p casparian --tests`).

---

## Prompt G: P0 Support Bundle + Logging Foundation

Worktree:
- `git worktree add ../casparianflow-p0-support-bundle -b p0/support-bundle`
- `cd ../casparianflow-p0-support-bundle`

Goal:
Add persistent logging under CASPARIAN_HOME and a CLI command to collect a
support bundle for offline bug reports.

Checklist items:
- Support bundle for offline bug reports (P0)
- Persistent logs in CASPARIAN_HOME (P0)

Context:
- Logging init: `crates/casparian/src/main.rs`,
  `crates/casparian_sentinel/src/main.rs`
- CASPARIAN_HOME helpers: `crates/casparian/src/cli/config.rs`
- Worker logs currently in /tmp: `crates/casparian_worker/src/bridge.rs`,
  `crates/casparian_worker/src/worker.rs`
- CLI command patterns: `crates/casparian/src/cli/*.rs` and `main.rs`
- DB path in home: `casparian_flow.sqlite3` in CASPARIAN_HOME

Tasks:
1) Create a shared logging initializer that writes to CASPARIAN_HOME/logs/
   with rotation (keep <= 5 files, <= 10MB each).
   - Log to file + stderr for errors/warn; avoid noisy info on TUI unless
     `--verbose`.
2) Persist per-job bridge logs under CASPARIAN_HOME/logs/jobs/<job_id>.log
   (no /tmp deletion).
3) Add CLI command `casparian support-bundle` (or `bug-report`) that zips:
   - logs (app + job logs)
   - `config.toml`
   - DB file `casparian_flow.sqlite3`
   - recent job metadata JSON (if available)
   - optional flags to redact paths or exclude db
4) Add a small unit/integration test for bundle creation (use temp
   CASPARIAN_HOME, run command, assert zip exists and contains key entries).

Acceptance:
- Targeted CLI tests pass (`cargo test -p casparian --tests`).
- Logs appear under CASPARIAN_HOME/logs/.
- Support bundle command produces a zip with expected contents.
