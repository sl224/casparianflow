# ADR-018: Bound Worker Count and Simplify Distributed Control Plane

Status: Accepted  
Date: 2026-01-18  
Owners: Platform + Product  

## Context
Casparian targets regulated, local-first environments (finance, healthcare,
legal, manufacturing, defense). The bottleneck for parse throughput is almost
always shared storage (SMB/NAS) or local disk IO, not CPU. Most deployments
use 1GbE and many small files; adding workers beyond a small number increases
contention and support issues without real throughput gains.

Customers value:
- Predictable throughput on shared storage.
- Deterministic, audit-ready processing.
- Simple operations in air-gapped or offline environments.

## Decision
Bound the worker fleet and simplify the control plane:

1) Hard cap worker count at 8. Default target is 4.  
2) Assume a homogeneous worker pool (same plugins and envs on every worker).  
3) Remove dynamic environment provisioning and "pending env" state in Sentinel.  
4) Store job progress per job only (no multi-worker aggregation).  
5) Keep a single Sentinel (no coordinator service).

Distributed workers still run on different machines; the simplification is
about uniformity and capacity, not single-host deployment.

## Rationale
- The storage/network layer is the throttle. A cap aligned to IO limits avoids
  wasted complexity and reduces operator confusion.
- Homogeneous workers match how regulated teams operate: approved images,
  pre-installed dependencies, and minimal runtime drift.
- Removing dynamic env provisioning avoids runtime network calls and reduces
  control-plane surface area in air-gapped environments.
- Per-job progress is sufficient because jobs are single-file and do not use
  multiple workers for one job.

## Scope
This ADR applies to server DB deployments (Postgres/SQL Server) and distributed
worker fleets. Embedded DB mode remains single worker by policy.

## Non-Goals
- No multi-worker execution for a single job.
- No heterogeneous worker fleets with plugin-specific routing.
- No new coordinator service.
- No dataset branching or shared-blob GC changes.

## Expected Simplifications

### A) Worker Cap Enforcement
Add a configurable `max_workers` (default 4, hard cap 8). Reject new worker
registrations once the cap is reached.

Primary code locations:
- `crates/casparian_sentinel/src/sentinel.rs` (worker registration)
- `crates/casparian_sentinel/src/main.rs` (config)

### B) Homogeneous Workers (No Capability Routing)
Treat all workers as equivalent and eligible for all jobs. Remove plugin-based
dispatch filtering and per-worker capability checks.

Primary code locations:
- `crates/casparian_sentinel/src/sentinel.rs` (dispatch loop, can_handle)
- `crates/casparian_protocol/src/types.rs` (IdentifyPayload capabilities, if no longer used)

### C) Remove Dynamic Env Provisioning
Stop using `PrepareEnv` / `EnvReady` as a provisioning workflow. Envs are
installed out-of-band by operators (offline friendly).

What remains:
- Keep `env_hash` on plugin manifests for auditability.
- Workers must have the env preinstalled and fail fast if missing.

Primary code locations:
- `crates/casparian_sentinel/src/sentinel.rs` (remove pending/ready env state)
- `crates/casparian_protocol/src/lib.rs` and `crates/casparian_protocol/src/types.rs`
  (remove opcodes and payloads)
- `docs/DECISION_WORKER_PYTHON_EXECUTION.md` (update assumptions)

### D) Per-Job Progress Only
Progress is stored per job (single row). Remove multi-worker aggregation
and `(job_id, worker_id)` progress tables.

Primary docs:
- `specs/jobs_progress.md` (use a single-row model)

## Operational Guidance
- Workers should be provisioned from the same base image or install recipe.
- Provide a single operator command to install/update plugin envs on each
  worker (out-of-band, not via Sentinel).
- If a worker is missing an env, it should fail the job with a clear error
  that names the missing `env_hash`.

## Risks and Mitigations
Risk: Operators forget to install envs on a worker.  
Mitigation: add a worker self-check on startup and a clear failure message.

Risk: Some customers want specialized nodes.  
Mitigation: require a separate, homogeneous worker pool per specialization.

## Success Criteria
- Fewer support issues around "job not running due to env provisioning."
- Stable throughput with 4-8 workers on typical NAS/SMB shares.
- Simpler dispatch and progress logic without behavior regressions.
