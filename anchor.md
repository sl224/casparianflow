```markdown
# FINAL UPDATED ANCHOR — Casparian Flow (Repo Review + Tape/Playback + Execution Plan)

> This is the canonical saved context across long sessions/model switches.
> - **[FACT]** = verified from repo snapshot; code claims include `path::symbol` (line ranges are best-effort and may drift).
> - **[HYPOTHESIS]** = plausible but not yet confirmed end-to-end.
> - **[RECO]** = recommended target direction / plan.
> - If uncertain: **Unknown**.

---

## 0) Anchor Metadata
- Date: 2026-01-23 (timezone unknown)
- Repo: “Casparian Flow” (name inferred; exact repo slug Unknown)
- Commit/branch: Unknown (zip snapshot)
- Review stage: Pass 2 (system map + critique + findings + tape/playback design + execution plan)

---

## 1) Product Intent (Verified)

### Target users (as stated / implied)
- [FACT] DFIR / Incident Response is explicitly named; EVTX is a flagship example.  
  - Evidence: `README.md`
- [FACT] Broader “turn files into tables” + governance primitives exists in README and architecture.  
  - Evidence: `README.md`

### Jobs-to-be-done (what the code supports)
- [FACT] Scan/discover filesystem corpuses into a catalog (Scout).  
  - Evidence: `crates/casparian/src/scout/scanner.rs::Scanner::scan`
- [FACT] Select files and enqueue work with idempotency keys to skip already materialized outputs.  
  - Evidence: `crates/casparian/src/cli/pipeline.rs::SelectionConfig`  
  - Evidence: `crates/casparian/src/cli/pipeline.rs::enqueue_jobs`  
  - Evidence: `crates/casparian_protocol/src/idempotency.rs::materialization_key`
- [FACT] Dispatch jobs to workers; execute plugins (Python shim + native) to produce record batches.  
  - Evidence: `crates/casparian_protocol/src/types.rs::DispatchCommand`  
  - Evidence: `crates/casparian_sentinel/src/sentinel.rs::dispatch_loop`  
  - Evidence: `crates/casparian_worker/src/worker.rs::execute_job_inner`
- [FACT] Validate output schema, quarantine invalid rows, and inject lineage columns.  
  - Evidence: `crates/casparian_worker/src/schema_validation.rs::validate_record_batch`  
  - Evidence: `crates/casparian_worker/src/worker.rs::split_output_batches`  
  - Evidence: `crates/casparian_sinks/src/lib.rs::inject_lineage_columns`
- [FACT] Persist outputs to sinks (DuckDB/Parquet/CSV) and record materializations for incremental ingestion.  
  - Evidence: `crates/casparian_sinks/src/lib.rs::DuckDbSink::write_batch`  
  - Evidence: `crates/casparian_sentinel/src/sentinel.rs::record_materializations_for_job`  
  - Evidence: `crates/casparian_protocol/src/idempotency.rs::output_target_key`, `materialization_key`

### Constraints (as implemented)
- [FACT] Local-first default home/DB path behavior.  
  - Evidence: `crates/casparian/src/cli/config.rs::ensure_casparian_home`, `active_db_path`
- [FACT] “Unified” runtime spawns Sentinel + Worker threads in one process.  
  - Evidence: `crates/casparian/src/main.rs::run_unified`
- [FACT] Worker bridge includes timeouts + log caps to avoid hangs/OOM.  
  - Evidence: `crates/casparian_worker/src/bridge.rs::CONNECT_TIMEOUT`, `READ_TIMEOUT`, `MAX_LOG_FILE_SIZE`

---

## 2) UI/UX Map (What exists; reality vs intent)

### Surfaces
- [FACT] CLI: scan/pipeline/job/run/start/mcp/etc.  
  - Evidence: `crates/casparian/src/main.rs::run_command` (command router)
- [FACT] TUI: multiple modes (Discover/Parsers/Jobs/Approvals/Query/etc).  
  - Evidence: `crates/casparian/src/cli/tui/app.rs::TuiMode`, `App::handle_key`
- [FACT] Tauri UI (React + Tauri backend): routes exist, but some links target missing routes.  
  - Evidence: `tauri-ui/src/App.tsx`  
  - Evidence: `tauri-ui/src/screens/Discover.tsx` (nav to `/sessions/new`)  
  - Evidence: `tauri-ui/src/screens/Jobs.tsx` (nav to `/sessions/new`, `/jobs/:id`)

### Critical UI mismatches (verified)
- [FACT] Broken navigation blocks critical path early (first-success risk).  
  - Evidence: `tauri-ui/src/screens/Discover.tsx`, `tauri-ui/src/screens/Jobs.tsx`, `tauri-ui/src/App.tsx`
- [FACT] UI “cancel job” currently updates API jobs storage, not executing jobs; does not stop worker execution.  
  - Evidence: `tauri-ui/src-tauri/src/commands/jobs.rs::job_cancel`  
  - Evidence: `crates/casparian_sentinel/src/db/api_storage.rs::cancel_job`
- [FACT] UI “jobs” are backed by `cf_api_jobs`, while execution uses `cf_processing_queue`.  
  - Evidence: `crates/casparian_sentinel/src/db/api_storage.rs::ApiStorage::init_schema`  
  - Evidence: `crates/casparian_sentinel/src/db/queue.rs::JobQueue::init_queue_schema`  
  - Evidence: `tauri-ui/src-tauri/src/commands/jobs.rs::job_list`  
  - Evidence: `crates/casparian_sentinel/src/sentinel.rs::dispatch_loop`

---

## 3) System Map (Architecture as-is)

### Components
- [FACT] Sentinel = control plane (queue + dispatch + deploy + record materializations).  
  - Evidence: `crates/casparian_sentinel/src/sentinel.rs::Sentinel::bind`, `dispatch_loop`, `record_materializations_for_job`
- [FACT] Worker = execution plane (run plugin, validate schema, quarantine, write sinks).  
  - Evidence: `crates/casparian_worker/src/worker.rs::execute_job_inner`
- [FACT] Sinks = output persistence + lineage injection.  
  - Evidence: `crates/casparian_sinks/src/lib.rs::inject_lineage_columns`
- [FACT] DuckDB backend uses an exclusive lock approach.  
  - Evidence: `crates/casparian_db/src/backend.rs::DbConnection::open_duckdb`  
  - Evidence: `crates/casparian_db/src/lock.rs::try_lock_exclusive`
- [FACT] Protocol defines semantic opcodes (already a great “tape boundary”).  
  - Evidence: `crates/casparian_protocol/src/lib.rs::OpCode`

### Concurrency model (as implemented)
- [FACT] Worker processes multiple jobs concurrently (bounded).  
  - Evidence: `crates/casparian_worker/src/worker.rs::MAX_CONCURRENT_JOBS`
- [FACT] Abort/cancel is not true cancellation; it suppresses receipts and can leave side effects.  
  - Evidence: `crates/casparian_worker/src/worker.rs::handle_message` (Abort path)  
  - Evidence: `crates/casparian_worker/src/worker.rs::Worker::run_inner` (drops results)  
  - Evidence: `crates/casparian_worker/src/worker.rs::Worker::wait_for_all_jobs` (drops join handles on timeout)

---

## 4) “Docs vs Reality” (major drift)
- [FACT] DB doc mentions SQLite/Postgres; implementation uses DuckDB.  
  - Evidence: `crates/casparian_db/CLAUDE.md` vs `crates/casparian_db/src/backend.rs::DbConnection::open_duckdb`
- [FACT] Root `CLAUDE.md` references missing `ARCHITECTURE.md`.  
  - Evidence: `CLAUDE.md` (reference) + missing file in snapshot

---

## 5) Findings (F0…F13) — Severity + Evidence + Fix Direction (Summary)

> Full detail is in prior reviews; this section is the canonical list.

### F0 (S0) Data loss: file sink output naming collisions (overwrite)
- Evidence: `crates/casparian_sinks/src/lib.rs::job_prefix`, `output_filename`, `ParquetSink::init`, `CsvSink::init`
- Fix: collision-proof names (full job id or hash); property tests.

### F1 (S0) Cancel/Abort not real; side effects still occur
- Evidence: `crates/casparian_worker/src/worker.rs::handle_message` (Abort), `Worker::run_inner`, `wait_for_all_jobs`
- Fix: cancellation token + kill subprocess + prevent commit; remove “suppress receipt” semantics.

### F2 (S0) Partial/orphan outputs possible and may be untracked
- Evidence: `crates/casparian_sinks/src/lib.rs::DuckDbSink::write_batch`, `SinkRegistry::finish`; success-only materialization patterns in Sentinel
- Fix: staging + atomic promotion; always track side effects.

### F3 (S1) SinkMode modeled but ignored by sinks/execution
- Evidence: `crates/casparian_protocol/src/types.rs::SinkMode`; `crates/casparian_protocol/src/idempotency.rs::output_target_key`; sinks lack sink_mode in constructors; worker groups by sink_uri only
- Fix: thread sink_mode end-to-end; implement Replace/Error at least for DuckDB; fail-fast where unsupported.

### F4 (S1) Default sink exclusion breaks incremental target computation
- Evidence: `crates/casparian/src/cli/pipeline.rs::is_default_sink`, `output_target_keys_for_sinks`, fallback in `enqueue_jobs`
- Fix: compute expected outputs from contracts/manifests; default sinks expand to expected outputs; conservative fallback.

### F5 (S1) Exclusive DB lock blocks daemon+client UX
- Evidence: `crates/casparian_db/src/backend.rs::DbConnection::open_duckdb`; `crates/casparian_sentinel/src/sentinel.rs` long-lived conn; UI/CLI DB writes
- Fix: sentinel as mutation authority + Control API; UI/CLI are clients; readonly DB connections for query.

### F6 (S1) Two job systems split reality: `cf_api_jobs` vs `cf_processing_queue`
- Evidence: `crates/casparian_sentinel/src/db/api_storage.rs::ApiStorage::init_schema`; `crates/casparian_sentinel/src/db/queue.rs::JobQueue::init_queue_schema`; UI uses ApiStorage, execution uses JobQueue
- Fix: canonical job model (execution queue) drives UI; retire or explicitly map API jobs.

### F7 (S1) Lineage injection can be skipped incorrectly (OR vs AND)
- Evidence: `crates/casparian_worker/src/worker.rs::batch_has_lineage_columns`, `inject_lineage_batches`
- Fix: require all lineage columns or none; enforce reserved `_cf_*` namespace.

### F8 (S1 Perf) DuckDB sink row-by-row inserts bottleneck
- Evidence: `crates/casparian_sinks/src/lib.rs::DuckDbSink::write_batch` (per-row execute)
- Fix: bulk/Arrow/appender ingestion.

### F9 (S1) Rejected capacity consumes retry budget → dead-letter under load
- Evidence: `crates/casparian_sentinel/src/sentinel.rs::handle_conclude` (Rejected → requeue); `crates/casparian_sentinel/src/db/queue.rs::MAX_RETRY_COUNT`, `requeue_job`
- Fix: do not increment retry_count on Rejected; separate counters.

### F10 (S1) Broken UI routes / dead ends
- Evidence: `tauri-ui/src/App.tsx`; `tauri-ui/src/screens/Discover.tsx`, `Jobs.tsx`
- Fix: remove or implement routes; route smoke tests.

### F11 (S2) No migrations; schema drift implies DB deletion
- Evidence: schema init patterns like `crates/casparian_sentinel/src/db/queue.rs::JobQueue::init_queue_schema`
- Fix: since product not shipped: implement schema version + reset-on-mismatch (not full migrations).

### F12 (S2) Entrypoint traversal not hardened
- Evidence: `crates/casparian_worker/src/worker.rs::resolve_entrypoint`
- Fix: reject absolute/parent paths; canonicalize and enforce base containment.

### F13 (S1/S2) Python trust posture: signature_verified false; env inherited
- Evidence: `crates/casparian_sentinel/src/sentinel.rs::handle_deploy` (signature flag); `crates/casparian_worker/src/bridge.rs::spawn_guest` (inherits env)
- Fix: explicit policy: allow/disallow unsigned python; docs + UI warnings; (sandboxing later).

---

## 6) Tape + Playback Integration Review (Final Decisions + Design)

### Fixed decisions (constraints)
- **Minimal event taxonomy** (semantic commands, not click streams).
- **NDJSON tape** (`.jsonl`), append-only, versioned envelope `v=1`.
- Every event has: `event_id`, `seq`, `ts`, `class`, `name`, `actor`, `correlation_id`, optional `parent_id`.
- Default redaction mode = **hash**; never record raw paths, plugin source code, or query rows by default.
  - Evidence for existing redaction concept: `crates/casparian_protocol/src/http_types.rs::RedactionMode`
- Record incremental ingestion keys in tape: `output_target_key`, `materialization_key`.
  - Evidence: `crates/casparian_protocol/src/idempotency.rs::output_target_key`, `materialization_key`
- Stable input identity required for replay: include `source_hash` in job outcomes (receipt/tape).
  - Evidence: `crates/casparian_worker/src/worker.rs::compute_source_hash`

### Minimal tape model (summary)
- Envelope fields + hash-chaining optional.
- Event classes:
  - `UICommand`, `DomainEvent`, `SystemResponse`, `ErrorEvent`
- Ordering:
  - Per-file total order via `seq`; multi-actor merge uses correlation+ts+seq (no distributed ordering in v1).
- Causality:
  - `correlation_id` ties a workflow; `parent_id` ties immediate cause.

### Concrete insertion points (final)
- UICommand/SystemResponse:
  - CLI: `crates/casparian/src/main.rs::run_command`
  - Tauri backend: `tauri-ui/src-tauri/src/commands/*` (query/jobs/sessions/approvals)
- DomainEvent:
  - Job lifecycle + conclude: `crates/casparian_sentinel/src/sentinel.rs::assign_job`, `handle_conclude`
  - Materializations: `crates/casparian_sentinel/src/sentinel.rs::record_materializations_for_job`
- Replay modes to implement first:
  1) UI-only replay (mock responses)
  2) Headless “tape explain” (reducer)
  3) Golden session CI validate (schema + invariants)

---

## 7) Agentic Loop Synergies (Evaluated: included only if sound + valuable)

> These loops are considered **sound** because they operate on the minimal tape (semantic + causal + redacted) and do not require “observability sprawl”.

### Ship-first loops (strongly recommended)
1) **Support Bundle**
   - Build a redacted zip containing tapes + metadata + artifact refs.
   - High user value + low risk; unlocks support and reproducibility.

2) **Explain This Run**
   - Deterministic reducer builds timeline, highlights decision points (dispatch/conclude/materialize/quarantine).
   - High trust value; uses existing domain events without extra telemetry.

3) **Golden Sessions CI**
   - Validate tape schema and run UI-only replay against fixture tapes; optional hybrid later.
   - High engineering leverage and protects tape taxonomy from drift.

### Sound, valuable follow-ons (after foundations)
4) **Macro Compiler (Record → Script/Recipe)**
   - Compress UICommands into a parameterized recipe (CLI or internal).
   - Requires stable semantic command vocabulary.

5) **Repro Minimizer**
   - Delta-debugging between passing/failing tapes to produce minimal repro command sequence.
   - Powerful for parser bugs; requires a robust “hybrid replay” harness later.

6) **Quarantine Curator**
   - Clusters quarantine summaries across runs; proposes rule/contract adjustments.
   - Valuable once you emit structured violation categories (may require small schema additions beyond counts).

(Excluded for now: “UX friction mining” and “auto-instrumentation” due to privacy/sprawl risks unless kept strictly local and semantic.)

---

## 8) Target Architecture (Recommended; “make bad states impossible”)

### Target decomposition
- **Control Plane (Sentinel)**: single mutation authority for control-plane state (jobs, approvals, sessions, output catalog). Exposes local Control API (IPC/RPC).
- **Execution Plane (Workers)**: stateless executors; true cancellation; stage→promote atomic writes; emit receipts with stable identities.
- **Frontends (CLI/TUI/Tauri/MCP)**: clients of Control API for mutations; read-only DB for query where needed.
- **Tape subsystem**: shared library + per-component writers; replay engines for UI-only and headless explain.

### Non-negotiable invariants (end state)
- No output collisions (F0).
- No side effects after cancel (F1) + atomic output commits (F2).
- SinkMode obeyed and enforced (F3).
- Incremental ingestion target computation stable and conservative under config evolution (F4).
- Single canonical job model (F6) and truthful UI (F10).

---

## 9) Execution Plan (Updated, with clarifications)

### Clarifications applied
- Line numbers are best-effort; primary anchors are `path::symbol`.
- WS1-04 (DuckDB staging) and WS1-05 (file staging) should NOT be unified first; unify later only at “commit protocol” level if needed.
- `source_hash` in receipt (tape prerequisite) is **independent** and can land early; it blocks “good replay,” not WS1/WS2.
- WS3-01 needs an enabling API to query “ExpectedOutputs”; add WS3-00.
- Minimal fixture plugin should be its own PR: WS8-00.

### Workstreams (final)
- WS1 Output Integrity & Sink Semantics (F0/F2/F3/F8)
- WS2 Execution Correctness (F1/F7/F9)
- WS3 Incremental Ingestion Semantics (F4) + **WS3-00 ExpectedOutputs API**
- WS4 Control Plane Unification & DB Access (F5/F6/F11)
- WS5 UI/UX Alignment (F10 + cancel truthfulness)
- WS6 Security & Trust Hardening (F12/F13)
- WS7 Tape/Playback Foundation (Fixed Decisions)
- WS8 Tests & CI Harness + **WS8-00 Fixture plugin**

(See the PR-by-PR execution plan produced earlier; the ordering is adjusted to include WS3-00 and WS8-00 and to allow WS7-04 early.)

---

## 10) Evidence Index (Most important anchors)

### Protocol / identity
- `crates/casparian_protocol/src/lib.rs::OpCode`
- `crates/casparian_protocol/src/types.rs::DispatchCommand`
- `crates/casparian_protocol/src/types.rs::JobReceipt`
- `crates/casparian_protocol/src/idempotency.rs::{output_target_key, materialization_key, table_name_with_schema}`
- `crates/casparian_protocol/src/http_types.rs::RedactionMode`
- `crates/casparian_protocol/src/types.rs::IdentifyPayload` (worker_id)

### Control plane
- `crates/casparian_sentinel/src/sentinel.rs::{Sentinel::bind, dispatch_loop, assign_job, handle_conclude, record_materializations_for_job}`
- `crates/casparian_sentinel/src/db/queue.rs::{JobQueue::init_queue_schema, requeue_job, MAX_RETRY_COUNT}`
- `crates/casparian_sentinel/src/db/api_storage.rs::{ApiStorage::init_schema, cancel_job}`

### Worker
- `crates/casparian_worker/src/worker.rs::{execute_job_inner, handle_message, Worker::run_inner, wait_for_all_jobs, compute_source_hash, batch_has_lineage_columns, inject_lineage_batches, split_output_batches}`
- `crates/casparian_worker/src/schema_validation.rs::validate_record_batch`
- `crates/casparian_worker/src/bridge.rs::spawn_guest`

### Sinks
- `crates/casparian_sinks/src/lib.rs::{job_prefix, output_filename, ParquetSink::init, CsvSink::init, DuckDbSink::write_batch, SinkRegistry::finish, inject_lineage_columns}`

### DB locking
- `crates/casparian_db/src/backend.rs::DbConnection::open_duckdb`
- `crates/casparian_db/src/lock.rs::try_lock_exclusive`

### UI
- `tauri-ui/src/App.tsx`
- `tauri-ui/src/screens/{Discover.tsx, Jobs.tsx}`
- `tauri-ui/src-tauri/src/commands/{query.rs, jobs.rs, approvals.rs, sessions.rs}`

---
```

```markdown
# Product & Target Architecture Doc — “Casparian Flow” (Target State)

> This document describes: product vision, customer base, problem domain, and the **target architecture/structure** we are aiming for given the repo investigation and the Tape/Playback plan.
> - “As-is” code facts are backed by evidence pointers (see Appendix).
> - “Target” is explicitly marked **[TARGET]**.

---

## 1) What the Product Is

**Casparian Flow** is a **local-first ingestion and governance runtime** that turns messy file corpuses into **typed, queryable tables** with:
- **Incremental ingestion** (don’t redo work; safe reruns)
- **Lineage** (row → file → job → parser version → contract)
- **Quarantine** (bad rows are preserved, not dropped)
- **Schema contracts + approvals** (auditability and controlled evolution)
- **Replayable “event tapes”** that make workflows debuggable and automatable without telemetry sprawl

### Core “promise”
> If you can point Casparian at a directory of files and a parser, you can reliably produce tables you can trust—and you can prove how you got them.

---

## 2) Customer Base & Domain Pain Points

### Primary early customers (explicit + strong fit)
1) **DFIR / Incident Response**
   - They operate in air-gapped / sensitive environments.
   - Inputs: EVTX, registry hives, browser artifacts, logs, forensics bundles.
   - Pain: extracting structured evidence repeatably; proving provenance; generating consistent reports.
   - Repo evidence: DFIR/IR target is explicit; EVTX is mentioned in README. *(See Appendix “Evidence”)*

### Broader customers (the repo is already structurally aimed at them)
2) **Security / compliance analytics teams**
   - Need reproducible, auditable pipelines from raw evidence to structured findings.
3) **Data teams ingesting “semi-structured files”**
   - Inputs: CSV/JSONL/parquet variants, vendor exports, periodic drops, spreadsheets, PDFs→tables (longer-term).
   - Pain: drift, inconsistent schemas, incremental reruns, “why did this number change?”.
4) **Regulated industries**
   - Need redaction, reproducibility, support bundles that don’t leak PII.

### Shared pain points the system must solve
- **Messy inputs and schema drift**
- **Incremental ingestion**
  - New files arrive, old files update, parsers change
  - Need deterministic reprocessing decisions
- **Trust + audit**
  - “Where did this row come from?”
  - “Which parser version produced it?”
  - “What changed between runs?”
- **Operational safety**
  - Cancel must mean stop + no side effects
  - Outputs cannot be partially committed and then “forgotten”

---

## 3) Core User Flows (Target UX)

### Flow A: “Turn a directory into tables” (critical path)
1) Choose a source (directory or bundle)
2) Scan / discover (build catalog; optional)
3) Choose parser(s) and define selection rules
4) Run ingestion
5) Inspect results:
   - Outputs produced (tables/files)
   - Quarantine summary and examples
   - Lineage details
6) Iterate:
   - adjust rules/contracts/parser
   - rerun incrementally

**Success definition**
- Outputs are committed atomically and recorded as materializations.
- Quarantine is visible and actionable (not just a log).

### Flow B: “Incremental ingestion / backfill”
1) Update selection window (since watermark, tags)
2) Update sink destination or output topics
3) Rerun
4) System enqueues exactly what’s needed (conservative, deterministic)

### Flow C: “Support / reproduce / automate”
1) Export support bundle (redacted by default)
2) Replay UI-only (mock) or headless explain
3) Turn workflow into a macro/recipe (optional)

---

## 4) Target Architecture (Clean Decomposition)

### 4.1 Components (TARGET)

#### [TARGET] Control Plane (“Sentinel” as daemon + Control API)
- Single mutation authority for:
  - job queue + job state machine
  - approvals
  - sessions (if retained)
  - output catalog + materializations
  - schema contract registry (deploy + promote)
- Exposes **local IPC/RPC** (“Control API”) for all mutations:
  - enqueue/cancel/retry jobs
  - approve/reject
  - session create/advance
- **Never** require UI/CLI to open writable DB concurrently.

**Rationale**
- Prevent split-brain and exclusive-lock dead ends.
- Make multi-client UX possible without introducing distributed systems complexity.

#### [TARGET] Execution Plane (“Worker”)
- Stateless executor:
  - accepts dispatch commands
  - runs parser plugin
  - validates schema
  - writes outputs via sinks
  - emits receipts with stable IDs (including `source_hash`)
- Must support **true cancellation**:
  - abort kills subprocess
  - prevents commit (or records partial side effects explicitly)

#### [TARGET] Persistence
- Control-plane metadata DB (DuckDB OK for local-first):
  - jobs, catalog, contracts, approvals, materializations, tape index
- Output stores:
  - DuckDB sink (embedded)
  - Parquet/CSV sinks (lake-ish)
  - (future) Iceberg/Delta

#### [TARGET] Frontends
- CLI/TUI/Tauri/MCP are **clients** of the Control API.
- Query console uses **read-only** DB connections.

---

## 5) Target State Model & Invariants

### 5.1 Domain entities (TARGET)
- **InputFile**
  - identity: `source_hash` (blake3) + optional path hash
- **ParserArtifact**
  - identity: `artifact_hash` + `env_hash` + parser version
- **Job**
  - has input(s), parser reference, and expected outputs
  - state machine: Queued → Dispatched → Running → {Completed | Failed | Aborted | Rejected}
- **OutputTarget**
  - identity: `output_target_key` (includes sink URI hash, table name, schema hash, sink_mode)
- **Materialization**
  - identity: `materialization_key` (output_target + source_hash + parser artifact)
- **Contract**
  - schema constraints for outputs; approval gating (optional per customer)

### 5.2 “Bad states impossible” invariants (MUST)
1) **No output collisions**
   - file sink artifact names are globally unique
2) **Atomic commits**
   - staged output is promoted only on success
3) **Cancel means stop**
   - aborted job cannot commit outputs
4) **SinkMode is enforced**
   - Replace/Error/Append semantics are consistent with idempotency keys
5) **Lineage is deterministic**
   - reserved `_cf_*` namespace cannot silently break lineage injection
6) **Incremental decisions are deterministic**
   - default sink configs do not cause “silent skip” when changed
7) **UI is truthful**
   - cancel button and job statuses reflect reality

---

## 6) Tape + Playback (TARGET)

### 6.1 Why tape is foundational
- Enables reproducibility without logging sprawl.
- Provides substrate for automation and agentic loops without raw telemetry.

### 6.2 Tape principles (TARGET constraints)
- Minimal semantic taxonomy (≈10–15 event names)
- Versioned NDJSON envelope, causal linking (`correlation_id`, `parent_id`)
- Redaction-by-default:
  - no raw paths, no plugin code, no query rows by default
- References by hash/pointer:
  - `source_hash`, `artifact_hash`, `materialization_key`

### 6.3 Replay modes (TARGET, shipped incrementally)
1) UI-only replay (mock responses)
2) Headless “tape explain” reducer
3) Golden session validation in CI
4) (later) Hybrid replay (UI→real backend comparisons)
5) (later) Real-worker replay with pinned inputs/artifacts

---

## 7) Agentic Loop Synergies (Only those judged sound + valuable)

> Included loops meet all of:
> - operate on semantic tape (no click-stream)
> - causal/replayable
> - redaction-compatible
> - clear user/engineering value

### Loop 1: Support Bundle (Ship early)
- Produces a redacted zip with:
  - tapes
  - environment metadata
  - artifact refs (hashes)
  - optional minimized repro subset
- Value: High for DFIR/regulatory; de-risks support.

### Loop 2: Explain This Run (Ship early)
- Deterministic reducer reconstructs:
  - job timeline
  - outputs/materializations
  - quarantine summary
  - error hashes and causal attribution
- Value: High user trust and debug clarity.

### Loop 3: Golden Sessions CI (Ship early for engineering)
- Replays UI commands against mocked responses and validates tape schema.
- Protects taxonomy from drift; catches UI regressions.

### Loop 4: Macro Compiler (Ship after command vocabulary stabilizes)
- Converts a successful tape segment into a parameterized “recipe” (CLI script or saved workflow).
- Value: High for power users; automation without “AI magic.”

### Loop 5: Repro Minimizer (Later; high leverage)
- Delta-debugging between passing/failing tapes to produce minimal repro.
- Value: Extremely high for parser/runtime bugs once hybrid replay exists.

### Loop 6: Quarantine Curator (Later; valuable once richer violation structure exists)
- Clusters quarantine summaries by parser version, source cohorts, and rule signatures.
- Recommends contract/rule/parsing adjustments.
- Value: High for large-scale ingestion, but depends on emitting structured violation categories.

(Deliberately excluded for now: UX friction mining and auto-instrumentation loops because they tend to demand larger event taxonomies and raise privacy/telemetry concerns.)

---

## 8) Appendix — Evidence anchors for “as-is” reality
*(Line numbers drift; use `path::symbol` as primary anchor.)*

- Semantic protocol boundary:
  - `crates/casparian_protocol/src/lib.rs::OpCode`
  - `crates/casparian_protocol/src/types.rs::DispatchCommand`
  - `crates/casparian_protocol/src/types.rs::JobReceipt`
- Idempotency primitives:
  - `crates/casparian_protocol/src/idempotency.rs::output_target_key`
  - `crates/casparian_protocol/src/idempotency.rs::materialization_key`
- Worker input hashing exists:
  - `crates/casparian_worker/src/worker.rs::compute_source_hash`
- Sentinel materialization recording:
  - `crates/casparian_sentinel/src/sentinel.rs::record_materializations_for_job`
- UI command boundaries:
  - `tauri-ui/src-tauri/src/commands/query.rs::query_execute`
  - `tauri-ui/src-tauri/src/commands/jobs.rs::job_cancel`
  - `tauri-ui/src-tauri/src/commands/approvals.rs::approval_decide`
  - `tauri-ui/src-tauri/src/commands/sessions.rs::{session_create,session_advance}`
- Existing redaction concept:
  - `crates/casparian_protocol/src/http_types.rs::RedactionMode`
- Known correctness risks (to be fixed):
  - output naming: `crates/casparian_sinks/src/lib.rs::{job_prefix,output_filename,ParquetSink::init,CsvSink::init}`
  - cancellation: `crates/casparian_worker/src/worker.rs::{handle_message,Worker::run_inner,wait_for_all_jobs}`
  - default sink idempotency: `crates/casparian/src/cli/pipeline.rs::{is_default_sink,output_target_keys_for_sinks,enqueue_jobs}`
  - dual job model: `crates/casparian_sentinel/src/db/{api_storage.rs,queue.rs}`
```
