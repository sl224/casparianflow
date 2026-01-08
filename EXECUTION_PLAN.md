# Casparian Flow: Unified Architecture - Parallel Execution Plan

**Goal:** Implement the "Modal Architecture" defined in `spec.md` and `UNIFIED_ARCHITECTURE_PLAN.md` (Revision 8).
**Focus:** Convergence of Dev/Prod modes, Protocol Unification, and Safety.

---

## 1. ORCHESTRATION STRATEGY

To execute this safely in parallel, we will use **Git Worktrees** to isolate file system changes and **Feature Branches** to manage merge conflicts.

### Setup Command
```bash
# 1. Initialize Checkpoint
echo '{"phase": "setup", "status": "pending"}' > .claude_checkpoint.json

# 2. Create isolated worktrees for parallel agents
git worktree add ../cf-shim -b feat/universal-shim
git worktree add ../cf-storage -b feat/storage-abstraction
git worktree add ../cf-bundle -b feat/parser-bundling
```

---

## 2. PARALLEL WORKSTREAMS (Phase 1)

Spawn 3 Agent instances simultaneously. Each must work *only* in its assigned directory/branch.

### 🤖 Worker A: Universal Shim & Safety (Python)
**Context:** The current `bridge_shim.py` is unsafe (crashes on mixed types) and lacks Dev mode loading logic.
**Directory:** `../cf-shim`
**Target Files:** `crates/casparian_worker/shim/bridge_shim.py`, `crates/casparian_worker/shim/casparian_types.py`

**Tasks:**
1.  **Implement `safe_to_arrow`:**
    *   Replace direct `pa.Table.from_pandas(df)` calls.
    *   Wrap in `try/except pa.ArrowInvalid`.
    *   On failure, convert object columns to string and retry.
2.  **Implement Loader Modes:**
    *   Add argument parsing for `--parser-path` (Dev mode: `sys.path.insert`, `importlib`).
    *   Add argument parsing for `--parser-archive` (Prod mode: `base64` decode, `zipfile` extract).
3.  **Standardize Output:**
    *   Ensure `output_info` structure matches `UNIFIED_ARCHITECTURE_PLAN.md`.
    *   Implement sideband logging routing `[LOG_SIGNAL]`.

**Validation:**
```bash
# In ../cf-shim
python3 -m py_compile crates/casparian_worker/shim/bridge_shim.py
# Create a small test script to import the shim and verify safe_to_arrow logic
```

### 🤖 Worker B: Storage Abstraction (Rust)
**Context:** `casparian run` and `casparian worker` duplicate DB logic. We need a unified Trait.
**Directory:** `../cf-storage`
**Target Files:** `crates/casparian/src/storage/mod.rs` (new), `crates/casparian/src/storage/sqlite.rs` (new)

**Tasks:**
1.  **Define Traits:**
    *   Create `JobStore` trait: `claim_next`, `heartbeat`, `requeue_stale`, `complete`, `fail`.
    *   Create `ParserStore` trait: `get`, `insert`.
    *   Create `QuarantineStore` trait.
2.  **Implement SQLite:**
    *   Implement `SqliteJobStore` using `sqlx`.
    *   Implement **Atomic Claim**: `UPDATE ... RETURNING` pattern.
    *   Implement **Heartbeat**: Update `last_heartbeat_at`.
3.  **Schema Migration:**
    *   Write the SQL migration string to create/update tables (`cf_jobs`, `cf_parsers`, `cf_quarantine`).

**Validation:**
```bash
# In ../cf-storage
cargo check -p casparian
```

### 🤖 Worker C: Parser Bundling (Rust)
**Context:** Production parsers need to be immutable ZIP artifacts with `uv.lock`.
**Directory:** `../cf-bundle`
**Target Files:** `crates/casparian/src/bundler.rs` (new), `crates/casparian/src/cli/parser.rs`

**Tasks:**
1.  **Implement `bundle_parser`:**
    *   Input: Directory path.
    *   Output: `ParserBundle` struct (archive blob, hashes).
    *   **Logic:**
        *   Check `uv.lock` exists (fail if not).
        *   Walk directory, filtering allowed extensions (`.py`, `.json`, `.yaml`).
        *   **Exclude** `.venv`, `__pycache__`, `*.so`.
        *   Create ZIP with **Canonical Timestamps** (1980-01-01) for deterministic hashing.
2.  **Update Register Command:**
    *   Modify `cli/parser.rs` to use this bundler.
    *   Compute `lockfile_hash` and `source_hash`.

**Validation:**
```bash
# In ../cf-bundle
cargo check -p casparian
# Unit test the bundler against a dummy directory
```

---

## 3. INTEGRATION (Phase 2 - Sequential)

Once Phase 1 workers report success, the Orchestrator (You) performs merges and spawns Phase 2.

**Merge Order:**
1.  `feat/storage-abstraction` (Base layer)
2.  `feat/parser-bundling` (Depends on storage traits for `ParserStore`)
3.  `feat/universal-shim` (Independent file, logical dependency for execution)

### 🤖 Worker D: Executor Refactor (Rust)
**Context:** `casparian run` uses ZMQ (deprecated). `bridge.rs` uses Unix Sockets. Need to unify.
**Branch:** `feat/executor-refactor`

**Tasks:**
1.  **Refactor `bridge.rs`:**
    *   Expose `execute_bridge` that accepts `BridgeConfig`.
    *   Ensure it supports the new Binary Protocol `[LENGTH][BYTES]`.
2.  **Implement Runners:**
    *   `DevRunner`:
        *   Resolves Python from `$VIRTUAL_ENV`.
        *   Sets `Stdio::inherit()` for debugging.
        *   Calls `execute_bridge`.
    *   `QueuedRunner`:
        *   Resolves Python from Managed Venv.
        *   Sets `Stdio::piped()` for log capture.
        *   Wraps `execute_bridge` with `JobStore` updates.
3.  **Update `cli/run.rs`:**
    *   Delete all ZMQ logic.
    *   Instantiate `DevRunner`.
    *   Execute.

---

## 4. CLEANUP (Phase 3)

**Tasks:**
1.  Delete `crates/casparian_worker/shim/run_shim.py`.
2.  Remove ad-hoc dependency hashing from `main.rs`.
3.  Delete `ProcessingHistory` struct from `cli/run.rs` (replaced by stateless execution).

---

## 5. VALIDATION COMMANDS

**Unit Tests:**
```bash
cargo test -p casparian
```

**Dev Mode Verification:**
```bash
# Should work, show output in terminal, allow pdb
cargo run -p casparian -- run ./demo/parsers/mcdata_parser.py ./demo/data/sales_2024_01.csv --whatif
```

**Prod Mode Verification:**
```bash
# Register parser (bundles zip)
cargo run -p casparian -- parser register ./demo/parsers/
# Start worker
cargo run -p casparian -- start
# Queue job (in separate terminal)
cargo run -p casparian -- process-job ...
```
