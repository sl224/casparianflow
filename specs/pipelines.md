# Pipelines & SWE Workflows Specification

**Status:** Draft
**Version:** 0.1
**Parent:** spec.md
**Related:** specs/job_types.md, specs/features/streaming_scanner.md, docs/schema_rfc.md
**Last Updated:** 2026-01-18

---

## 1. Overview

Casparian Flow is analyst-first, but regulated teams also need SWE-grade
repeatability: scheduled runs, deterministic inputs, and audited lineage.
This spec defines pipeline orchestration, file selection semantics, logical
execution time, fixed-phase batch topology, and lineage capture for
production workflows without weakening the single-file parse contract.

Core idea:
- Analysts use CLI/TUI for ad-hoc work.
- SWEs define pipelines that resolve catalog selections into deterministic
  snapshots and run jobs on a schedule.
- Lineage is captured by the system, not by user-managed JSON files.
- Pipelines are shallow but wide: singleton phases + fan-out parse.

---

## 2. Customer + Market Context (for PMF Review)

**Finance (Trade Support Analysts)**:
- Workflow: grep FIX logs, reconstruct timelines under T+1 pressure.
- Constraints: operations teams do not have Databricks access; they need
  repeatable runs on on-prem log shares.
- Value: scheduled parsing of daily FIX logs with audit-ready outputs.

**Legal (Litigation Support)**:
- Workflow: ingest PSTs and load files on recurring productions.
- Constraints: cost-sensitive firms cannot use Relativity at scale; need local
  batch processing and export.
- Value: nightly/batch runs to build consistent datasets and exports.

**Healthcare (Interface Analysts)**:
- Workflow: HL7 archives land on SMB shares with predictable daily folders.
- Constraints: HIPAA, air-gapped hospital IT, no cloud dependency.
- Value: scheduled extraction of HL7 archives into queryable datasets.

**Defense (PCAP / CoT Analysts)**:
- Workflow: data arrives via USB/sneakernet; analysts need repeatable local runs.
- Constraints: DDIL, no network, laptop-first operation.
- Value: offline pipelines that can be triggered on new drops.

**Implication:** A pipeline layer must work offline, be deterministic, and
create an auditable record of what files were processed.

---

## 3. Goals and Non-Goals

### 3.1 Goals
- Provide a SWE-friendly way to publish and schedule flows.
- Keep PARSE as single-file jobs with schema contracts and quarantine.
- Capture deterministic lineage: selection spec + resolved snapshot hash.
- Avoid SDK sprawl; favor stable CLI + config contracts.
- Support air-gapped environments (no cloud scheduler dependency).

### 3.2 Non-Goals
- General-purpose workflow engine (Airflow/Prefect parity).
- Multi-language parser execution.
- Hidden DB query interfaces as the primary API.

---

## 4. Core Concepts

### 4.1 Catalog
The catalog is the source of truth for files:
`scout_files`, tags, path metadata, and extraction fields.

### 4.2 SelectionSpec
A normalized, versioned definition of "which files to process."

Example:
```yaml
selection:
  tag: raw
  ext: .hl7
  since: "P1D"
  source: "hospital_archive"
  watermark: "mtime"
```

Notes:
- `watermark` is optional. If set, snapshot resolution stores the maximum
  observed value (e.g., `mtime`) for the selection window to enable
  incremental runs without re-scanning the full catalog.

### 4.3 SelectionSnapshot
A resolved, immutable snapshot of file identities from a SelectionSpec.
Snapshots are generated at pipeline run time and stored with a hash.

### 4.4 Pipeline
A named, versioned configuration that binds:
- a SelectionSpec
- a job type (PARSE or MATERIALIZE)
- a schedule (optional)
- output configuration

### 4.5 Logical Execution Time
Pipelines use logical execution time (a la Airflow) to avoid gaps and
duplication when runs are delayed or retried.

- Scheduler creates a run for a specific `logical_date`, even if wall-clock
  time is later.
- Relative time windows (e.g., `since: "P1D"`) resolve against `logical_date`,
  not `System.now()`.
- Backfill iterates logical dates over a range.

### 4.6 Fixed-Phase Topology
Pipelines do not allow arbitrary user-defined steps. The topology is fixed:

1. **Materialize (optional, singleton)**: Build context datasets.
2. **Parse Fan-Out (required)**: One file → one parse job (data-parallel).
3. **Export (optional, singleton)**: Deliver domain formats from parsed outputs.

Cycles are impossible by design; no DAG solver required.

### 4.7 Execution Model (Queue-First)
The scheduler generates a work queue from the SelectionSnapshot:
- Resolve snapshot once.
- Bulk enqueue parse jobs (fan-out).
- Workers drain the queue using atomic fetches.

This keeps scheduling overhead O(1) per job and scales to large file sets.

### 4.8 Materialize Job
A multi-file job that builds a stable dataset to be consumed as read-only
context by PARSE jobs. See `specs/job_types.md`.

---

## 5. User Workflows

### 5.1 Analyst (Ad-Hoc)
```
casparian scan /data --tag raw
casparian run --parser hl7_oru --tag raw --since 2024-01-01
```

### 5.2 SWE (Production Pipeline)
```
casparian pipeline apply hl7_oru_daily.yaml
casparian pipeline run hl7_oru_daily
casparian pipeline status hl7_oru_daily
```

Scheduled runs are handled by the built-in scheduler (Section 8) or an
external trigger.

### 5.3 Backfill
```
casparian pipeline backfill hl7_oru_daily --start 2025-01-01 --end 2025-06-01
```

Backfill iterates logical execution dates and resolves selections using
logical dates, not wall-clock time.

---

## 6. Pipeline Spec (YAML)

```yaml
pipeline:
  name: hl7_oru_daily
  schedule: "0 2 * * *"
  selection:
    tag: raw
    ext: .hl7
    since: "P1D"
    source: "hospital_archive"
  context:
    materialize:
      tag: refdata
      output: /data/catalog/reference.parquet
  run:
    parser: hl7_oru
    output: /data/outputs/hl7_oru/
  export:
    name: fhir-r4
    output: /data/exports/hl7_oru_fhir/
```

Notes:
- `selection` is normalized into a SelectionSpec stored in DB.
- `context.materialize` is optional; when present, runs before PARSE.
- `context.materialize.output` is exposed to the parser as
  `CASPARIAN_CONTEXT_REFDATA` for deterministic access.
- `schedule` is optional; manual runs still resolve snapshots.
- `export` is optional; when present, it runs after PARSE completes.

---

## 7. Lineage Model

Each pipeline run records:
- `pipeline_id`
- `selection_spec_id`
- `selection_snapshot_hash`
- `context_snapshot_hash` (if MATERIALIZE used)
- `job_id` for each PARSE/MATERIALIZE job

Lineage is derived from system-captured snapshots, not user-edited files.

---

## 8. Scheduling

Requirements:
- Runs in air-gapped environments.
- No external service dependency.
- Supports cron-style schedules.

Behavior:
- Scheduler wakes, creates a run with `logical_date`, resolves SelectionSpec
  relative to that logical date, then enqueues parse jobs.
- If no files match, run is recorded with status `no_op`.

---

## 9. Pipeline Run State Machine

States:
- `queued` → run created, awaiting execution
- `running` → snapshot resolved and jobs enqueued
- `completed` → all jobs completed successfully
- `failed` → one or more jobs failed without recovery
- `no_op` → selection resolved to zero files

Transitions:
- `queued` → `running`
- `running` → `completed` | `failed` | `no_op`
- `failed` → `queued` (manual retry/backfill resume)

---

## 10. Data Model (Draft)

```sql
CREATE TABLE cf_selection_specs (
  id TEXT PRIMARY KEY,
  spec_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE cf_selection_snapshots (
  id TEXT PRIMARY KEY,
  spec_id TEXT NOT NULL,
  snapshot_hash TEXT NOT NULL,
  logical_date TEXT NOT NULL,
  watermark_value TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE cf_selection_snapshot_files (
  snapshot_id TEXT NOT NULL,
  file_id TEXT NOT NULL,
  PRIMARY KEY (snapshot_id, file_id)
);

CREATE TABLE cf_pipelines (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  version INTEGER NOT NULL,
  config_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_pipelines_name_version
ON cf_pipelines(name, version);

CREATE TABLE cf_pipeline_runs (
  id TEXT PRIMARY KEY,
  pipeline_id TEXT NOT NULL,
  selection_spec_id TEXT NOT NULL,
  selection_snapshot_hash TEXT NOT NULL,
  context_snapshot_hash TEXT,
  logical_date TEXT NOT NULL,
  status TEXT NOT NULL,
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT NOT NULL
);
```

Indexes:
```sql
CREATE INDEX idx_snapshot_files_snapshot ON cf_selection_snapshot_files(snapshot_id);
CREATE INDEX idx_snapshot_files_file ON cf_selection_snapshot_files(file_id);
CREATE INDEX idx_pipeline_runs_pipeline ON cf_pipeline_runs(pipeline_id, logical_date);
```

---

## 11. SDK Positioning

No multi-language SDK in v1.
Provide a stable CLI + pipeline spec to avoid version drift and support burden.
If SDK is needed later, it should be a thin wrapper over CLI + config.

---

## 12. Open Questions

- Pipeline naming: "pipeline" vs "flow".
- Whether to allow direct SQL in SelectionSpec (power vs stability).
- Incremental materialize vs full rebuild semantics.

---

## 13. TUI Integration (Planned)

The Scout/Discovery TUI should support "Export to Pipeline":
- User filters files in TUI.
- System generates a SelectionSpec and pipeline YAML skeleton.
- Reduces YAML burden for analysts while preserving SWE-grade workflow.

Jobs view should emphasize batch progress signals:
- Throughput (files/sec), % complete, ETA.
- Stragglers (slowest N files) and quarantine counts.
- Schema contract status for the parser in use.
