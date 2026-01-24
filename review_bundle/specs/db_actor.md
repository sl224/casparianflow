# DB Actor Boundary Spec (DuckDB)

## Purpose
Own the async boundary for all DB backends with a single, explicit concurrency
model. Provide a stable async API while preventing UI/event-loop blocking and
avoiding multi-writer contention for DuckDB's single-writer semantics.

## Context (Casparian-specific)
- Local-first, air-gapped, single DB file.
- Control-plane writes are small and frequent; data-plane reads are heavier.
- DuckDB is sync; "async" only means "do not block the UI/event loop."
- DuckDB allows multiple readers but only one writer at a time.
- Embedded DB deployments are single-worker by product policy; concurrent
  workers require a server backend (post-v1).

## Goals
- Single async API (`DbConnection`) with a DuckDB-backed actor in v1.
- Dedicated DB actor thread that serializes writes and isolates sync work.
- Deterministic backpressure controls.
- Clear shutdown semantics and error propagation.
- Instrumentation for latency/queue depth.
- Keep the implementation concrete and minimal (no trait adapters in v1).
- Make the embedded mode concurrency policy explicit (single worker).

## Non-Goals (v1)
- Multi-writer parallelism for DuckDB.
- Transparent automatic reconnect/retry policies.
- Implicit write batching (only explicit batching/transactions).
- Read-scaling via a separate read pool (can be added later).
- Priority lanes across request types.

## Architecture Overview
- `DbActor` runs on a dedicated OS thread.
- `DbConnection` is an async facade; each method sends a request over an
  `mpsc` channel and awaits a `oneshot` response.
- `DbConnection` is a concrete, cheap-cloneable handle (no trait-object adapter
  in v1). If we add Postgres later, use an internal enum backend or a separate
  concrete type without changing the public API surface.
- Request types:
  - `Execute`
  - `ExecuteBatch`
  - `QueryAll`
  - `QueryOne`
  - `QueryOptional`
  - `QueryScalar` (if needed)
- Backend-specific execution is hidden inside the actor.
- Transaction API: closure-based `transaction(|tx| async { ... })` that is
  executed on the actor thread. There is no exposed transaction handle that
  lives across awaits outside the actor.

## Concurrency Model
- Single writer discipline for DuckDB. All writes are serialized in the
  actor thread.
- Reads run in the actor thread in v1 for simplicity and determinism.
  (Later: split reads into a pool or allow read-only connections.)
- Requests are processed FIFO from a single channel. No priority lanes in v1.
- Any async method may block the caller until the actor completes the request,
  but will not block the UI/event loop.
- Embedded deployments are single-worker by product policy. If a deployment
  requires multiple concurrent workers, use a server backend instead of
  DuckDB.

## Batching and Transactions
- No implicit batching in v1.
- `ExecuteBatch` runs caller-provided SQL as a single batch.
- `transaction` runs the closure as a single transaction on the actor.
- Reads are not batched.
- If implicit cross-request batching is ever added, use explicit savepoints
  (or only batch caller-provided groups) to avoid "bad apple" rollback across
  unrelated requests.

## Backpressure
- Use a bounded channel.
- Default behavior: await capacity (caller backpressured).
- Optional future policy: drop low-priority telemetry writes.

## Shutdown Semantics
- `shutdown(graceful = true)` drains queue and completes all requests.
- Forced shutdown cancels pending requests with a clear error.
- No automatic restart in v1.
- "Drain" means: stop accepting new requests, process all queued requests,
  then close the actor thread.

## Error Handling
- `ExecuteBatch` failures return a single error to the caller.
- Transaction failures return the error to the caller.
- Errors are surfaced through the async API, preserving backend codes.
- If a caller drops its future, the actor still completes the request and
  discards the response (ignore `oneshot` send errors).

## Instrumentation
Emit metrics/logs for:
- Queue depth
- P50/P95/P99 request latency
- Time-in-queue vs time-in-DB (separate wait time from execution time)
- Batch size and batch interval
- Read/write ratio
- Failure counts by request type

## Columnar Alignment (DuckDB-specific)
DuckDB prefers append-heavy workloads. For columnar friendliness, model job
updates as append-only events and derive state by query or periodic compaction.

Example (not columnar-friendly):
```
UPDATE jobs SET status = 'done', updated_at = now() WHERE id = ?;
```

Columnar-friendly alternative:
```
INSERT INTO job_events(job_id, status, occurred_at) VALUES (?, 'done', now());
```

Derived state:
```
SELECT job_id, arg_max(status, occurred_at) AS status
FROM job_events
GROUP BY job_id;
```

For the control plane (small, frequent writes), favor append-only modeling or
DuckDB appenders to stay columnar-friendly.

## Tradeoff Notes (Actor vs async-duckdb)
- Actor boundary keeps a single, owned async model around DuckDB.
- It is more code we own but avoids mismatched abstractions around single-writer
  semantics and gives us explicit backpressure/instrumentation control.
- async-duckdb is less code we own but still requires single-writer discipline.

## Extension: Multi-writer Backends (Future)
The boundary stays the same; only execution strategy changes:
- DuckDB: serialize writes.
- Postgres/SQL Server: actor dispatches to a small worker pool but still owns
  backpressure, retries, and batching rules.

## Test Matrix (Casparian-specific)

1) Single writer guarantee
- 100 concurrent tasks calling execute/query.
- Expect: no deadlocks, deterministic write order.

2) Backpressure
- Bounded queue saturated with writes.
- Expect: callers await capacity; no request loss.

3) Explicit batch correctness
- `ExecuteBatch` with multiple statements.
- Expect: succeeds or returns a clear error.

4) Read/write interleave
- Continuous writes + periodic reads.
- Expect: reads complete within bounded latency (no starvation).

5) Cancellation storms
- Many callers drop futures before completion.
- Expect: actor completes/cleans up without leaks or panics.

6) Shutdown behavior
- Graceful shutdown drains queue and returns success.
- Forced shutdown cancels pending requests with clear errors.

7) DuckDB UI protection
- Long-running DuckDB operation should not block async runtime/UI loop.

8) Crash recovery
- Simulate actor panic mid-batch.
- Expect: DB file remains consistent and can be reopened.

9) Mixed workload soak
- Sustained mixed reads/writes for N minutes.
- Expect: steady latency, no unbounded queue growth.

10) "Bad apple" batch (future)
- If implicit batching is introduced: one invalid write should not poison
  unrelated writes (savepoint or explicit batch grouping required).

11) Slow reader blocking (current risk)
- Long-running `QueryAll` followed by a UI `Execute`.
- Expect: `Execute` waits (v1); note as a known tradeoff until read pool exists.

12) Channel saturation + dropped futures
- Fill `mpsc` buffer and drop callers awaiting capacity.
- Expect: actor does not deadlock; dropped oneshots are ignored.

13) DuckDB small-write soak (if using DuckDB for control plane)
- Many tiny writes over time.
- Expect: no pathological file growth or unacceptable latency (measure).

## Test Harness Plan (v1)
Where: `crates/casparian_db/tests/db_actor.rs` (integration) + targeted unit
tests in `crates/casparian_db/src/backend.rs`.

Core building blocks:
- Helper to open temp DuckDB DB and return `DbConnection`.
- Helper to spawn N tasks, collect latency stats, and assert ordering.
- Shared barrier for precise concurrency start (avoid sleeps).

Scenarios to implement first:
1) Transaction isolation (DuckDB actor)
   - Use `DbConnection::transaction` to create table + insert rows.
   - Interleave concurrent `execute` calls; assert no interleaving inside tx.
2) Cancellation storm
   - Spawn tasks, drop futures mid-flight; assert actor still serves new requests.
3) Channel saturation
   - Fill queue; ensure callers backpressure (await capacity) and no deadlocks.
4) Slow reader blocking (known tradeoff)
   - Long `SELECT` then `INSERT`, assert `INSERT` latency is > query duration.
5) Bad-apple batch (future)
   - If implicit batching added, verify savepoint handling.

Performance sanity checks:
- Record time-in-queue vs time-in-DB (log metrics under `tracing`).
- Fail tests if queue time exceeds a small bound under light load.
