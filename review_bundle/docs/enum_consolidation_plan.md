Enum Consolidation Plan (For LLM Review)
========================================

Status: Internal reference (not a product spec; may diverge from code).

Purpose
-------
This document proposes a concrete consolidation plan for status enums in this repo. It is written for an LLM to review for validity, correctness, and potential regressions. It includes current definitions, storage conventions, and proposed mappings with explicit file references.

Scope
-----
- Consolidate overlapping "status" enums without changing domain semantics.
- Separate queue lifecycle status (ProcessingStatus) from completion outcome (JobStatus).
- Add per-column DB constraints for enum values.
- Keep Scout domain statuses separate (FileStatus, ExtractionStatus, ExtractionLogStatus).

Non-goals
---------
- No large behavior changes to job scheduling or UI semantics beyond clarifying state mapping.
- No schema migration tooling addition beyond standard SQL statements and table rebuild steps.

Operating constraints (from strategy docs)
------------------------------------------
- Local-first, air-gapped deployments with no network calls, telemetry, or license checks.
- File-based storage (SQLite/DuckDB) and manual upgrades on offline machines.
- Limited IT support and budgets; avoid new infrastructure and complex migrations.
- Batch processing of archived files is the dominant mode (not streaming).

Current State: Enum Inventory and Storage
-----------------------------------------
Queue lifecycle and completion outcome
- ProcessingStatus (canonical queue lifecycle)
  - Location: `crates/casparian_protocol/src/types.rs:60`
  - Variants: Pending, Queued, Running, Staged, Completed, Failed, Skipped
  - Serialized: SCREAMING_SNAKE_CASE; `as_str()` returns uppercase strings.
  - DB usage today: queue table uses uppercase strings (see below).
- JobStatus (completion outcome for Worker -> Sentinel)
  - Location: `crates/casparian_protocol/src/types.rs:788`
  - Variants: Success, PartialSuccess, CompletedWithWarnings, Failed, Rejected, Aborted
  - Only used in protocol receipts, not a queue lifecycle state.

Sentinel database model duplication
- StatusEnum
  - Location: `crates/casparian_sentinel/src/db/models.rs:13`
  - Variants: Pending, Queued, Running, Completed, Failed, Skipped
  - This duplicates ProcessingStatus, without Staged.
  - `from_db()` defaults to Pending on unknown values (silent drift risk).
- PluginStatusEnum
  - Location: `crates/casparian_sentinel/src/db/models.rs:38`
  - Variants: Pending, Staging, Active, Rejected
  - DB uses other statuses (SUPERSEDED, possibly DEPLOYED) not represented here.

TUI job status
- Local JobStatus (UI-only)
  - Location: `crates/casparian/src/cli/tui/app.rs:460`
  - Variants: Pending, Running, Completed, PartialSuccess, Failed, Cancelled
  - This is a UI presentation enum, but currently fed by mixed queue/outcome strings.
- String mapping in TUI
  - Location: `crates/casparian/src/cli/tui/app.rs:7247`
  - Mapping mixes queue lifecycle strings (RUNNING/QUEUED) and outcome strings
    (SUCCESS/PARTIAL_SUCCESS) into the UI enum directly.

Scout domain statuses
- FileStatus
  - Location: `crates/casparian/src/scout/types.rs:114`
  - Variants: Pending, Tagged, Queued, Processing, Processed, Failed, Skipped, Deleted
  - Stored in `scout_files.status` as lowercase strings.
- ExtractionStatus
  - Location: `crates/casparian/src/scout/types.rs:332`
  - Variants: Pending, Extracted, Timeout, Crash, Stale, Error
  - Stored in `scout_files.extraction_status` as lowercase strings.
- ExtractionLogStatus
  - Location: `crates/casparian/src/scout/types.rs:436`
  - Variants: Success, Timeout, Crash, Error
  - Stored in `scout_extraction_log.status` as lowercase strings.

Database schemas (selected)
- Queue table (DuckDB/SQLite creation)
  - Location: `crates/casparian_sentinel/src/db/queue.rs:54`
  - `cf_processing_queue.status` is TEXT, default 'QUEUED' (uppercase).
- Plugin manifest table
  - Location: `crates/casparian_sentinel/src/db/queue.rs:88`
  - `cf_plugin_manifest.status` is TEXT, default 'PENDING' (uppercase).
  - Sentinel code uses 'ACTIVE' and sets previous versions to 'SUPERSEDED'
    (`crates/casparian_sentinel/src/sentinel.rs:994`).
  - Query uses 'DEPLOYED' as an allowed active status
    (`crates/casparian_sentinel/src/db/queue.rs:208`).
- Pipeline runs table
  - Status strings set to lowercase: 'running', 'failed', 'completed'
    (`crates/casparian_sentinel/src/sentinel.rs:709`).
  - Table creation in tests uses `status TEXT NOT NULL` without constraints
    (`crates/casparian_sentinel/tests/integration.rs:18`).
- Scout tables
  - Schema defined in `crates/casparian/src/scout/db.rs:59` and
    `crates/casparian/src/scout/db.rs:120`.

Observed Issues
---------------
1) Duplicate queue lifecycle enums:
   - StatusEnum (sentinel) duplicates ProcessingStatus and lacks Staged.
2) Mixed semantics in UI:
   - TUI maps completion outcomes and queue status strings into the same enum.
3) Status strings used without explicit DB constraints:
   - DB accepts any string, and code often defaults unknown to Pending
     (`crates/casparian_sentinel/src/db/models.rs:23`).
4) Plugin manifest status mismatch:
   - DB uses SUPERSEDED; model enum does not include it.
   - Query expects DEPLOYED; it is not created elsewhere.

Proposed Consolidation
----------------------
Canonical definitions
- Queue lifecycle: `ProcessingStatus` (protocol crate).
- Completion outcome: `JobStatus` (protocol crate).
- UI display: a local UI-only enum (rename to `UiJobStatus`) derived from
  `(ProcessingStatus, Option<JobStatus>)`.
- Scout statuses stay local (FileStatus, ExtractionStatus, ExtractionLogStatus).
- Sentinel plugin manifest: define a canonical `PluginStatus` that matches
  actual DB values (PENDING, STAGING, ACTIVE, REJECTED, SUPERSEDED, and decide
  on DEPLOYED).
- Pipeline runs: define `PipelineRunStatus` for queued/running/failed/completed.

Schema ownership and storage
- Store enum values as strings with per-column CHECK constraints (no global
  enum table).
- Keep existing casing:
  - `cf_processing_queue.status`: uppercase (matches ProcessingStatus.as_str()).
  - `cf_pipeline_runs.status`: lowercase.
  - Scout tables: lowercase.

Mapping functions (draft)
-------------------------
Queue status on conclude (when outcome present):
- Success, PartialSuccess, CompletedWithWarnings -> ProcessingStatus::Completed
- Failed, Aborted -> ProcessingStatus::Failed
- Rejected -> ProcessingStatus::Queued (job is requeued; clear completion_status)

UI status from queue + outcome:
```
ui_status(queue, outcome):
  if queue in {Pending, Queued} -> Pending
  if queue in {Running, Staged} -> Running
  if queue == Skipped -> Completed
  if queue == Failed:
    if outcome == Aborted -> Cancelled
    else -> Failed
  if queue == Completed:
    if outcome in {PartialSuccess, CompletedWithWarnings} -> PartialSuccess
    if outcome == Failed -> Failed
    if outcome == Aborted -> Cancelled
    else -> Completed
```
Note: this preserves the existing UI treatment that blends outcome into a
single display status, but makes the mapping explicit and testable. Skipped
continues to render as Completed; if we need visibility later, add a tag in
detail views rather than a new UI enum state.

Proposed DB Constraints (per-column)
------------------------------------
Queue:
- `cf_processing_queue.status` IN
  ('PENDING','QUEUED','RUNNING','STAGED','COMPLETED','FAILED','SKIPPED')
- New `completion_status` column (nullable) with CHECK:
  `completion_status IS NULL OR completion_status IN
  ('SUCCESS','PARTIAL_SUCCESS','COMPLETED_WITH_WARNINGS','FAILED','REJECTED','ABORTED')`

Plugin manifest:
- `cf_plugin_manifest.status` IN
  ('PENDING','STAGING','ACTIVE','REJECTED','SUPERSEDED','DEPLOYED')
  - Treat DEPLOYED as a deprecated alias of ACTIVE (allowed for compatibility).

Pipeline runs:
- `cf_pipeline_runs.status` IN ('queued','running','failed','completed')

Scout:
- `scout_files.status` IN
  ('pending','tagged','queued','processing','processed','failed','skipped','deleted')
- `scout_files.extraction_status` IN
  ('pending','extracted','timeout','crash','stale','error')
- `scout_extraction_log.status` IN
  ('success','timeout','crash','error')

Migration Sketch (DuckDB/SQLite)
--------------------------------
Constraints cannot be added with ALTER in DuckDB/SQLite. Use table rebuild:
0) Rollout: apply constraints for new DBs at creation time. For existing DBs,
   run migration only after upgrading binaries. Avoid dual-write/feature flags.
1) Validate existing rows:
   - `SELECT DISTINCT status FROM ... WHERE status NOT IN (...)`
2) Create new table with CHECK constraints.
3) Copy rows.
4) Drop old table, rename new table.
5) Recreate indexes and sequences.

Targeted changes (high level)
-----------------------------
- Remove `StatusEnum` and use `ProcessingStatus` in sentinel DB models.
- Add `completion_status` (nullable); set only for terminal rows; clear on requeue.
- Replace TUI string mapping with `ProcessingStatus` + optional `JobStatus`
  derived from DB fields.
- Define `PluginStatus` and `PipelineRunStatus` enums that match DB usage.
- Add CHECK constraints and migration steps for the status columns.

Resolved decisions (minimal change, compatibility-first)
--------------------------------------------------------
1) Allow `PENDING` and `SKIPPED` in sentinel constraints. Treat as legacy but
   keep them to avoid breaking existing DBs and tooling.
2) Allow `ProcessingStatus::Staged` in DB constraints even if sentinel does not
   emit it; this avoids breaking CLI flows and preserves future flexibility.
3) Map `JobStatus::Aborted` -> queue Failed; UI Cancelled; store in
   `completion_status` when terminal.
4) Treat `DEPLOYED` as a deprecated alias of ACTIVE; keep it in CHECK and map
   it to ACTIVE in code. Re-evaluate removal after verifying no rows use it.
5) Keep pipeline run status lowercase to avoid migration churn.
6) `completion_status` is nullable; only terminal rows have outcomes, and it is
   cleared on requeue.

Acceptance Criteria for Validity
--------------------------------
- All status columns in DB have explicit allowed sets that match code enums.
- Queue lifecycle and completion outcome are modeled and stored separately.
- UI status is derived from explicit mapping, not implicit string mixing.
- No enum value is removed if still referenced in code paths or tests.
- Migrations include validation steps to catch existing invalid values.
