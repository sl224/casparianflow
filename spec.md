# Casparian Flow - Product Specification (v1 Direction)

**Version:** 1.1
**Status:** Directional (v1)
**Date:** 2026-01-18
**References:** `docs/v1_scope.md`, `docs/schema_rfc.md`, `docs/execution_plan.md`, `docs/v1_checklist.md`

---

## 1. Executive Summary

Casparian Flow is a **deterministic, governed data build system** for file artifacts.
v1 targets DFIR / Incident Response: parse Windows artifacts (EVTX as flagship)
into queryable datasets with strict schema contracts, quarantine, and per-row lineage.

**What Casparian IS (v1):**
- Batch file-at-rest parsing into DuckDB + Parquet
- Deterministic execution with reproducibility guarantees
- Authoritative schema validation in Rust (not Python)
- Quarantine semantics for invalid rows (partial success is safe)
- Per-row lineage for chain of custody

**What Casparian is NOT (v1):**
- NOT a streaming platform (batch only)
- NOT an orchestrator/scheduler (single-machine execution)
- NOT a BI tool (outputs to DuckDB/Parquet for downstream analysis)
- NOT "no-code" (CLI-first, requires technical users)
- NOT AI-dependent (AI assistance is optional, outside critical path)

---

## 2. Core Principles

1. **Schema is contract**: approved schemas are enforced in Rust; violations quarantine
   or fail according to policy. Invalid rows never silently coerce into clean tables.
2. **Local-first**: runs on a single machine or on-prem server; no cloud
   dependencies in v1. Air-gapped and sovereignty-friendly.
3. **Deterministic execution**: source code + lockfile hashes define parser
   identity (content-addressed); same inputs + same parser = identical outputs.
4. **Quarantine over coercion**: bad rows go to quarantine with error context;
   runs may succeed partially (PartialSuccess status).
5. **Per-row lineage**: every output row has `_cf_source_hash`, `_cf_job_id`,
   `_cf_processed_at`, `_cf_parser_version`.
6. **Simple interfaces**: CLI/TUI first; no SDK sprawl.

## 2.1 Trust Primitives / Integrity Guarantees

| Guarantee | Description |
|-----------|-------------|
| **Reproducibility** | Same inputs + same parser bundle hash → identical outputs |
| **Per-row lineage** | Every row has source hash, job id, timestamp, parser version |
| **Authoritative validation** | Schema contracts enforced in Rust; no silent coercion |
| **Quarantine semantics** | Invalid rows isolated with context; partial success is safe |
| **Content-addressed identity** | Parser identity = blake3(parser_content + lockfile) |
| **Backfill planning** | Version changes trigger explicit re-processing planning |

---

## 3. Primary Workflows

### 3.0 DFIR Core Workflows (v1 Target)

| Workflow | Description |
|----------|-------------|
| **Case folder ingestion** | Point Casparian at directory of EVTX files or extracted artifacts |
| **Evidence bundle ingestion** | Normalize an offline collection zip into tagged inputs |
| **Parser execution** | Run EVTX parser pack on corpus; outputs to DuckDB + Parquet |
| **Quarantine triage** | Review quarantine summary; drill into violation types and sample rows |
| **Backfill planning** | When parser changes, see exactly what needs reprocessing |
| **Timeline query** | Query by time range, host, event_id, user to build attacker timeline |

### 3.1 Dev Loop (Iteration)
- Use `casparian run` or `casparian preview` on a single file.
- Uses the local Python environment (non-interactive execution in v1).
- Outputs sample data or a job result to DuckDB/Parquet.

### 3.2 Publish Loop (Deployment)
- Use `casparian publish <parser.py>` to deploy to Sentinel.
- Sentinel stores the plugin manifest (source + lockfile) and hashes
  (`env_hash`, `artifact_hash`) in DuckDB.
- Environments are provisioned out-of-band and must exist on workers.

### 3.3 Execute Loop (Queued Processing)
- Sentinel dispatches jobs to homogeneous workers.
- Worker validates against schema contracts, splits quarantine, and writes
  outputs to configured sinks.
- Job status uses `PartialSuccess` when quarantine occurs; `CompletedWithWarnings`
  is treated as success if encountered but is not emitted in v1.

---

## 4. Architecture Overview

```
[ CLI / TUI ]
      |
      v
[ Sentinel + DuckDB ]  <-- control plane, job queue, manifests
      |
      v
[ Worker + Bridge ]    <-- Python subprocess, Arrow IPC
      |
      v
[ Output Sinks ]       <-- DuckDB / Parquet
```

---

## 5. V1 Scope (Summary)

**In scope**
- EVTX demo parser and quickstart flow.
- Rust validation + quarantine split.
- Lineage injection and per-output status.
- DuckDB + Parquet sinks with per-output routing.

**Out of scope (post-v1)**
- Pipelines/scheduler layer.
- Postgres/MSSQL sinks.
- Multi-node execution or server deployment.
- AI parser generation as a required workflow.

---

## 6. Interfaces

### 6.1 CLI (v1)
- `casparian scan <path>`: file discovery.
- `casparian preview <file>`: schema + sample rows.
- `casparian run <parser.py> <file>`: dev execution.
- `casparian publish <parser.py>`: deploy to Sentinel registry.
- `casparian start`: run Sentinel + Worker.
- `casparian worker`: run Worker only.
- `casparian jobs`: list/replay jobs, dead-letter view.
- `casparian parser list/show/test/backtest/health/resume`: parser ops.

### 6.2 TUI (v1)
- Discover: scan/tag/preview.
- Parser Bench: test + approval flows (minimal).
- Jobs: status + quarantine summary (minimal).

---

## 7. Contracts + Validation

- Contracts are defined in `docs/schema_rfc.md`.
- Validation is authoritative in Rust, not Python.
- Quarantine contains `_error_msg`, `_violation_type`, `_cf_job_id`, and
  a source row pointer.

---

## 8. Storage Policy (v1)

- DuckDB is the only supported DB backend in v1.
- No migrations: breaking schema changes require deleting the local DB.

---

## 9. Post-v1 References

Detailed future-oriented specs live under `specs/` and are marked as
reference only. Use `docs/specs_canonical.md` to find the current sources
of truth.
