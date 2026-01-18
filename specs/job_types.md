# Job Types Specification

**Status:** Draft
**Version:** 0.1
**Parent:** spec.md
**Last Updated:** 2026-01-18

---

## 1. Purpose

Define all Casparian job types, their inputs/outputs, and shared behavior.
This document is the canonical reference for job semantics across CLI, TUI,
and the worker/sentinel pipeline.

---

## 2. Shared Job Semantics

### 2.1 Lifecycle and Status
All job types use the shared lifecycle described in `spec.md` (FS-5/FS-8):
queued → running → completed/failed, with retries for transient errors.

**Status detail (output-aware):**
- `completed`: all outputs succeeded, no quarantine.
- `completed_with_warnings`: outputs succeeded with quarantine rows present.
- `failed`: job failed per contract policy or unrecoverable error.

### 2.2 Lineage and Audit
Every job must record:
- `job_id` (unique)
- `job_type`
- `inputs` (paths, tags, or queries)
- `outputs` (paths, tables, or artifacts)
- `parser/exporter` metadata when applicable
- timestamps and error info

### 2.3 Atomic Output
Jobs that write outputs must use staging + atomic rename where applicable
(`.staging/{job_id}/`), per `spec.md` FS-6.

---

## 3. Job Type Catalog

### 3.1 SCAN
**Purpose:** Discover files and populate Scout tables.  
**Inputs:** Source root path + ignore/glob rules.  
**Outputs:** `scout_files`, `scout_folders`, `scout_sources`.  
**Notes:** Bounded-memory streaming scan per `specs/features/streaming_scanner.md`.

### 3.2 PARSE
**Purpose:** Transform a single file into structured datasets.  
**Inputs:** One file path; parser artifact + schema contract.  
**Outputs:** Parquet/SQLite tables; quarantine artifacts; job logs.  
**Notes:** Core invariant: one file → one job → contract enforcement.

### 3.3 BACKTEST
**Purpose:** Test parser changes against a corpus for schema drift.  
**Inputs:** Parser artifact + selection of files (tag/query).  
**Outputs:** Backtest reports; failure summaries.  
**Notes:** Non-production; read-only inputs.

### 3.4 SCHEMA
**Purpose:** Approve or enforce schema contracts for parsers.  
**Inputs:** Parser + schema intent/observed schema.  
**Outputs:** Contract records; approval artifacts.  
**Notes:** No data transformation; governance-only.

### 3.5 EXPORT
**Purpose:** Transform parsed outputs into domain-specific delivery formats.  
**Inputs:** Parquet/SQLite outputs (many files) + exporter config.  
**Outputs:** Export files + manifest; lineage links to source parse jobs.  
**Spec:** `specs/features/export.md`.

### 3.6 MATERIALIZE
**Purpose:** Build a stable, versioned dataset from many files without
breaking the single-file PARSE contract.  
**Inputs:** Tag/query/glob over cataloged files; optional config.  
**Outputs:** Versioned dataset (Parquet/SQLite) + manifest + contract.  
**Notes:**
- Creates a durable dataset that later PARSE jobs can consume as read-only
  context.
- Uses staging + atomic publish to avoid mixed state.
- Preserves lineage to source files and source scan.

---

## 4. Materialize Job (Detail)

### 4.1 Rationale
Multi-file context is common (reference tables, lookups, attachments). A
materialization step provides that context without changing the core PARSE
invariant.

### 4.2 Input Selection
Inputs are defined via:
- Tag filter (e.g., `tag = "reference_data"`)
- Query (e.g., `SELECT path FROM scout_files WHERE ...`)
- Glob/Rule selection (via existing Discovery/Rules)

### 4.3 Output Contract
Materialize outputs:
- Must define a schema contract (like PARSE outputs).
- Must include `_cf_job_id` lineage columns.
- Must record a manifest with input hashes and selection criteria.

### 4.4 Usage Pattern
1. Materialize reference dataset from multiple files.
2. Run PARSE jobs on single files, reading the materialized dataset as context.

---

## 5. Open Questions

- Naming: "Materialize" vs "Snapshot" vs "Dataset Build".
- Whether MATERIALIZE supports incremental updates vs full rebuild.
- How PARSE job declares and validates context dependency.
