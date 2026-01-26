# Casparian Flow Configuration Matrix

**Purpose:** Make storage roles explicit so operators and contributors can answer:
- Where does control-plane state live?
- Where do outputs live?
- What do users query?

---

## Two planes, two responsibilities

### Control plane = State Store (transactional metadata)
Contains:
- job queue, claims, status updates
- approvals, sessions
- topic config, plugin registry
- scan bookkeeping + file catalog (scout)
- rules/tags/sources/workspaces

**Backends (configurable):**
- Local minimal-dependency: `sqlite` file
- Enterprise: `postgres` or `sqlserver`

### Data plane = Output Sinks (datasets users query)
Contains:
- output tables/datasets written by jobs

**Backends (configurable per topic/output):**
- Local default: `parquet://...` (concurrency-friendly)
- Optional local: `duckdb://...` (single-writer, small/single-worker only)
- Enterprise: `postgres://...` or `sqlserver://...` (concurrent writers, BI tooling)

### Local SQL UX (no infra)
Provide a **DuckDB query catalog** file (separate from state store) that exposes
stable table names over Parquet via `parquet_scan(...)` views.

This gives “standard SQL” + BI connectivity without running Postgres locally.

---

## Configuration matrix

### 1) Local / DFIR / air-gapped
- **state_store**: `sqlite` (default)
- **default_sink**: `parquet://~/.casparian_flow/output/`
- **query_catalog**: `~/.casparian_flow/query.duckdb` (DuckDB views over Parquet)
- **infra**: none (no services required)

### 2) Enterprise / existing DB
- **state_store**: `postgres` or `sqlserver`
- **sink(s)**: `postgres` or `sqlserver`
- **query**: use existing BI/SQL tooling on the sink tables

### 3) Hybrid
- **state_store**: `sqlite`
- **sink(s)**: `postgres` or `sqlserver`
- **query**: BI/SQL against enterprise sink; local state stays lightweight

---

## Rationale

- **Why SQLite for local state store?**
  - Embedded, transactional, stable.
  - Avoids DuckDB single-writer lock semantics for control-plane state.

- **Why Parquet for local outputs?**
  - Concurrent writers without coordination.
  - Easy to move/share.
  - Works with DuckDB query catalog for local SQL.

- **Why DuckDB still matters?**
  - It provides the local SQL/BI experience.
  - But it is **not** the state store.

---

## Operational notes (pre-v1)

- Schema changes are destructive in pre-v1:
  - Delete `~/.casparian_flow/state.sqlite` for state store changes.
  - Delete `~/.casparian_flow/query.duckdb` if you need a fresh query catalog.
  - Output Parquet files can be deleted or replaced as needed.

