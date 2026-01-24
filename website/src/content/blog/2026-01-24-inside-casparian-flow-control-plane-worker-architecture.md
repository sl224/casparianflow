---
title: "Inside Casparian Flow: A Control Plane + Worker Architecture Built for Trust"
description: "Casparian Flow separates control-plane state (Sentinel) from execution (Worker) so cancellation is real, outputs are atomic, and multi-client UX stays consistent. Here’s the architecture and the invariants it enforces."
pubDate: 2026-01-24
---

If you’ve built ingestion systems before, you know the temptation:

> “It’s a CLI. Let it do everything.”

Scan files, run parsers, write outputs, track state, handle retries… all in one process.

It works—until you need:

- a UI *and* a CLI to control the same runs
- truthful cancellation (abort means no commit)
- deterministic incremental ingestion tracking
- auditability and approvals for schema contracts

At that point, “just a CLI” becomes a distributed system hidden in a single binary.

Casparian Flow avoids that trap with a deliberate split:

- **Sentinel** = the control plane (single mutation authority)
- **Worker** = the execution plane (stateless executor)

This post explains that architecture and why it’s central to Casparian’s robustness.

---

## The Core Idea: Separate Authority From Execution

### Sentinel: Control Plane (Authority)
Sentinel owns **all mutable system state**:

- job queue and job state machine
- approvals and schema contract registry
- materializations for incremental ingestion
- output catalog

It exposes a **Control API**. Frontends (CLI/TUI/UI/MCP) call the API for mutations.

This prevents:
- split-brain state (two clients both “think” they own the queue)
- lock contention dead ends
- inconsistent UI behavior

### Worker: Execution Plane (Stateless)
Worker is a stateless executor that:

- accepts dispatch commands
- runs parser plugins (Python/native) in isolated subprocesses
- validates outputs against schema contracts
- writes outputs via sinks (DuckDB/Parquet/CSV)
- emits receipts on completion

Workers can be restarted, replaced, or scaled without changing control-plane logic.

---

## Why a “Single Mutation Authority” Matters

When multiple clients exist (CLI + TUI + UI + MCP), the system needs one place where truth lives.

If every client writes to the DB directly, you get:

- race conditions
- partial updates
- inconsistent status reporting
- hard-to-debug corruption

Casparian instead enforces a simple rule:

> All mutations go through Sentinel.

Read-only DB connections are fine for queries, but state transitions happen in one place.

This is what makes the UI truthful: job state is not an opinion.

---

## Cancellation That Actually Cancels

A lot of systems implement “cancel” as:

- set a flag
- hope the task checks it

That doesn’t work for ingestion tasks that:

- run external subprocesses
- stream large outputs
- have side effects (writes)

Casparian’s architecture makes cancellation real:

- Sentinel sends an ABORT command
- Worker kills the plugin subprocess
- outputs are staged and never promoted
- job status reflects what actually happened

This is not just a UX feature. It’s a trust guarantee.

---

## Atomic Outputs: Stage → Promote

Casparian writes outputs in two phases:

1. **Stage** outputs in a temporary location
2. **Promote** them on successful conclusion

If the job fails or is aborted, staged artifacts are not promoted.

That prevents the classic “half-written parquet files” problem where downstream queries accidentally read incomplete data.

Atomic commit is part of the architecture, not an afterthought.

---

## Deterministic Incremental Ingestion

Because Sentinel owns the materialization registry, it can plan work deterministically:

- check which (source hash, parser artifact, output target) combinations already exist
- enqueue only what needs processing
- keep decisions conservative and auditable

This is how Casparian turns ingestion into a build system: the control plane decides what must run, the worker executes, and receipts are recorded.

---

## Bridge Mode Execution (Plugin Isolation)

Casparian runs plugins out-of-process.

The worker (host) holds system capabilities; the plugin process (guest) holds only code.

This separation:

- reduces blast radius of plugin crashes
- makes cancellation possible (kill the guest)
- enables trust policies (signed artifacts, entrypoint validation)
- prevents “just attach a debugger” interactivity in production runs

---

## “Bad States Impossible”: Explicit Invariants

A serious runtime doesn’t just “work.” It prevents certain states from existing.

Casparian treats these as invariants:

- **No output collisions** (artifact naming is globally unique)
- **Atomic commits** (stage → promote only on success)
- **Cancel means stop** (aborted jobs cannot commit)
- **Lineage deterministic** (reserved metadata namespace cannot be broken)
- **Sink modes enforced** (replace/append/error semantics consistent)

These invariants aren’t marketing—they’re engineering constraints that guide the entire design.

---

## The Takeaway

The control plane / worker split might feel “heavy” for a local CLI tool.

But it’s what enables the things that make the system trustworthy:

- multi-client UX without split-brain
- cancellation that reflects reality
- atomic outputs
- deterministic incremental ingestion
- auditable schema contracts and approvals

If you care about evidence-grade ingestion, this architecture is not optional.

---

## Next in the Series

Next up: a practical tutorial for DFIR—**EVTX → timeline in DuckDB**, with lineage and quarantine in place.

<!-- TODO: update CTA link to your site -->
Want to evaluate Casparian on a real corpus? Reach out for a pilot: `/contact`
