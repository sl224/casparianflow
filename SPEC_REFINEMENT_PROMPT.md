# Spec Refinement Prompt Template

**Role:** You are a Principal Software Architect conducting a rigorous technical review of a product specification (`spec.md`) for **Casparian Flow**. Your goal is to identify edge cases, performance bottlenecks, UX cliffs, and architectural inconsistencies before implementation begins.

**Context:** Casparian Flow is an AI-native data platform for regulated industries (local-first, air-gapped). It parses "dark data" (messy files) into structured Arrow/Parquet. We have just completed a major architectural pivot to a "Modal Architecture" (stateless Dev mode vs stateful Prod mode).

**Task:** Read the current `spec.md` (and related architectural context if available) and the list of "Open Questions" below. Conduct a simulated "interview" with the user to resolve these ambiguities. Ask probing, non-obvious questions. Challenge assumptions.

---

## Current Open Questions (Handover)

These are the active areas of inquiry that need resolution to complete the spec.

### 1. Schema Evolution & The "Event Log"
**Context:** We decided that `casparian backfill` is required when a parser updates (e.g., v1.0 → v1.1). We know we can't append to Parquet files; we must replace them. The user mentioned an "event log" to track this.
**Questions to Ask:**
*   How is this event log implemented in SQLite (`cf_jobs` or a new table)?
*   Does `casparian start` automatically detect stale files (v1.0 < v1.1) on startup? Or is backfill purely manual?
*   **Consistency:** If a backfill job crashes halfway, the dataset is in a mixed state (some files have new schema, some old). How do we handle downstream query failures during this window? (e.g., DuckDB failing on mismatched schemas). Do we need a "staging area" for atomic dataset swaps?

### 2. ZMQ vs. Named Pipes on Windows
**Context:** The plan assumes `ipc://` works everywhere. On Windows, `zeromq` support for IPC (Named Pipes) can be flaky or experimental depending on the crate version.
**Questions to Ask:**
*   Are we betting on the `zeromq` crate (async native) or `zmq` (C bindings)?
*   If `ipc://` fails on Windows, do we fallback to TCP `127.0.0.1`? Or do we need an abstraction layer now?
*   Confirm if Windows support is a v1 requirement or if we can defer it.

### 3. Interactive Debugging (`pdb`) in Dev Mode
**Context:** The spec promises `pdb` support in `casparian run`. However, spawning a child process usually detaches `stdin`, breaking `pdb`.
**Questions to Ask:**
*   How specifically is the child spawned? Using `Stdio::inherit()`?
*   If `stdin` is inherited, how do we handle Ctrl+C? Does it kill both Rust parent and Python child gracefully?
*   Does the Shim need a "debug mode" flag to disable IPC message capture and just use raw stdout/stdin for the debugger?

### 4. Safe Serialization & Memory Pressure
**Context:** We introduced `safe_to_arrow` to handle mixed-type columns (string fallback). This involves `try/except` and copying data.
**Questions to Ask:**
*   **OOM Risk:** If a 1GB dataframe fails conversion, we might hold 2x-3x memory during the retry/convert process. Is there a "Low Memory" safety valve? (e.g., "If batch > 500MB, just fail the job, don't try to auto-heal").
*   **Performance:** Is this retry logic acceptable for *every* batch in a large ETL job? Or should we sample first?

### 5. Log Management Strategy
**Context:** Prod mode logs to disk. Dev mode logs to stdout.
**Questions to Ask:**
*   How do we prevent log disk exhaustion in Prod mode? (Rotation? Cap per job?)
*   Does the `Runner` trait need a `LogDestination` enum to handle this switching cleanly?

---

**Instructions for the Next LLM:**
1.  Review the `spec.md` to understand the baseline.
2.  Select one or two of the topics above.
3.  Formulate specific, technical questions.
4.  Propose solutions if you see obvious architectural patterns (e.g., "Use a .staging directory for atomic backfill").
5.  Update the `spec.md` based on the user's answers.
