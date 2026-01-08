# Unified Architecture Plan

> **Status:** Planning
> **Created:** 2025-01-07
> **Last Updated:** 2025-01-07
> **Revision:** 8 (LogDestination, uv error reporting, venv race handling)

---

## Table of Contents

1. [North Star](#north-star)
2. [Current State: The Problem](#current-state-the-problem)
3. [Critical Code Fixes (Verified Issues)](#critical-code-fixes-verified-issues)
4. [Proposed State: Modal Architecture](#proposed-state-modal-architecture)
5. [Critical Engineering Gaps](#critical-engineering-gaps)
6. [Database Abstraction Strategy](#database-abstraction-strategy)
7. [Parser Bundling Strategy](#parser-bundling-strategy)
8. [Environment Strategy](#environment-strategy)
9. [Validation Strategy](#validation-strategy)
10. [Quarantine Pattern](#quarantine-pattern)
11. [Updated Schema](#updated-schema)
12. [Execution Plan](#execution-plan)
13. [User Workflows](#user-workflows)
14. [Risk Mitigation](#risk-mitigation)
15. [Success Criteria](#success-criteria)

---

## North Star

> **Transform "dark data" into queryable datasets with zero friction.**

Users have files scattered across systems. They want to:
1. **Discover** files automatically
2. **Parse** them into structured data
3. **Query** the results

The system should feel like: "Point at files, get clean data."

---

## Current State: The Problem

The codebase has **TWO parallel systems** with incompatible protocols:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    CURRENT STATE (BROKEN)                               │
│                                                                         │
│   SYSTEM A: Direct Execution          SYSTEM B: Daemon Execution        │
│   ─────────────────────────           ──────────────────────────        │
│   CLI: casparian run                  CLI: casparian start/process-job  │
│   Shim: run_shim.py                   Shim: bridge_shim.py              │
│   Protocol: ZMQ PUSH/PULL             Protocol: Unix socket + binary    │
│   Format: JSON + Base64 Arrow         Format: [LENGTH:4][RAW_ARROW_IPC] │
│   Tables:                             Tables:                           │
│     - cf_parsers                        - cf_plugin_manifest            │
│     - cf_parser_topics                  - cf_plugin_environment         │
│     - cf_job_status                     - cf_processing_queue           │
│     - cf_processing_history                                             │
│                                                                         │
│   DIFFERENT PROTOCOLS. DIFFERENT SHIMS. DIFFERENT DATABASES.            │
└─────────────────────────────────────────────────────────────────────────┘
```

### Problems with Current State

| Problem | Impact |
|---------|--------|
| **Protocol divergence** | Two IPC implementations to maintain, different failure modes |
| **Duplicate database code** | ProcessingHistory in run.rs shadows Sentinel's JobQueue |
| Parser registered in System A doesn't exist in System B | Must re-publish parsers |
| Job history in System A invisible to System B | No unified view of work done |
| Different shims may have different behavior | Inconsistent execution |
| Version conflict detection only works within each system | Silent conflicts across systems |
| No path from "dev" to "prod" without re-registration | Friction in deployment |
| **Ad-hoc dependency resolution** | main.rs hashes dep names, not lockfile content |
| **Serialization crashes** | Mixed-type columns crash shim before Rust can quarantine |

### Core Insight

**Direct execution should NOT be a separate system.** But it also shouldn't be forced through distributed-system machinery.

The key realization: **Dev and Prod have different needs.**

| Aspect | Dev Mode (`casparian run`) | Prod Mode (`casparian worker`) |
|--------|---------------------------|-------------------------------|
| Primary goal | Fast iteration | Reliable processing |
| Parser source | File on disk | Bundled in DB |
| Python env | Current shell's `python` | Managed venv or `--python` |
| DB write timing | **Never** (stateless) | **Before** execution (required) |
| Debugging | Full stdio, `pdb` works | Logs only |

---

## Critical Code Fixes (Verified Issues)

> **Status:** These issues were verified against the actual codebase on 2025-01-07.
> All four must be addressed before this plan can be implemented.

### Issue 1: Protocol Divergence (HIGH SEVERITY)

**Location:** `cli/run.rs` vs `crates/casparian_worker/src/bridge.rs`

**Current State:**
| Component | Transport | Message Format | File |
|-----------|-----------|----------------|------|
| `run.rs` + `run_shim.py` | ZMQ PUSH/PULL | JSON with Base64 Arrow | run.rs:50, run_shim.py:47 |
| `bridge.rs` + `bridge_shim.py` | Unix socket | Binary `[LENGTH:4][ARROW_IPC]` | bridge.rs:20-41 |

**Problem:** Two completely different IPC protocols for the same conceptual operation (execute parser, stream results).

**Resolution:** Converge on Unix sockets with binary protocol.

Rationale:
1. Plan says "macOS-first" - Unix sockets are fine
2. `casparian run` is local-only - no need for ZMQ network transport
3. Binary Arrow IPC is more efficient than JSON+Base64 (~33% overhead)
4. `bridge.rs` already has a working implementation

**Implementation:**
```rust
// cli/run.rs - REMOVE ZMQ, use bridge::execute_bridge()
pub async fn run(args: RunArgs) -> Result<RunResult> {
    // ... resolve parser, input, python ...

    // Use the SAME bridge execution as prod mode
    let config = BridgeConfig {
        interpreter_path: python_path,
        source_code: read_parser_source(&args.parser_path)?,
        file_path: args.input_path.to_string_lossy().to_string(),
        job_id: 0,  // Dev mode: no job ID
        file_version_id: 0,
        shim_path: bridge::materialize_bridge_shim()?,
    };

    let result = bridge::execute_bridge(config).await?;
    // ... write output, print results ...
}
```

**Files to Delete:**
- `crates/casparian_worker/shim/run_shim.py`

**Files to Keep:**
- `crates/casparian_worker/shim/bridge_shim.py` (becomes the only shim)

---

### Issue 2: Duplicate Database (MEDIUM SEVERITY)

**Location:** `cli/run.rs:262-567`

**Current State:** `ProcessingHistory` struct creates its own tables:
- `cf_parsers`
- `cf_parser_topics`
- `cf_job_status`
- `cf_processing_history`

These shadow Sentinel's JobQueue tables.

**Problem:** User confirmed: "`casparian run` should NEVER touch database."

**Resolution:** Remove ProcessingHistory entirely. Make `casparian run` stateless.

```
Dev mode (casparian run):
  parser.py + input.csv → execute → output.parquet → exit
                                    ↑
                              NO DATABASE
```

**Implementation:**
```rust
// cli/run.rs - DELETE ProcessingHistory struct entirely
// DELETE: lines 262-567 (ProcessingHistory, all SQL)

pub async fn run(args: RunArgs) -> Result<RunResult> {
    // Execute parser via bridge
    let result = bridge::execute_bridge(config).await?;

    // Write output directly (no DB)
    write_output(&args.sink, &result.batches)?;

    // Print summary to stdout
    println!("✓ {} rows processed", result.batches.iter().map(|b| b.num_rows()).sum::<usize>());

    Ok(RunResult { ... })
}
```

**Migration:** Existing `cf_job_status` etc. tables from dev usage become orphaned. Document as deprecated.

---

### Issue 3: Ad-Hoc Dependency Bug (HIGH SEVERITY)

**Location:** `main.rs:1253-1309`

**Current State:**
```rust
// ADHOC PATH: Generate minimal lockfile with plugin deps + bridge deps
let deps = parse_plugin_dependencies(&plugin_source);  // Parse imports!
let deps_str = all_deps.join(",");  // "pandas,pyarrow,requests"
let hash = sha256(deps_str.as_bytes());  // NOT a real lockfile hash
```

**Problem:** This creates a fake "env_hash" from dependency NAMES without versions.
- `pandas` could be 1.5.0 on machine A, 2.0.0 on machine B
- Transitive dependencies not captured
- Completely non-reproducible

**Resolution:** Remove adhoc path entirely. Require uv.lock. Fail loudly if missing.

```rust
// main.rs - REPLACE adhoc path with error
let (env_hash, lockfile) = match (env_hash_opt, lockfile_content) {
    (Some(h), Some(l)) => (h, l),
    _ => {
        anyhow::bail!(
            "Plugin '{}' was deployed without a lockfile.\n\
             This is no longer supported.\n\n\
             Re-deploy with: casparian publish {} --version <new>\n\
             Ensure uv.lock exists in the plugin directory.",
            plugin_name, plugin_name
        );
    }
};
```

**Files to Modify:**
- `main.rs`: Delete lines 1253-1309 (adhoc path), add error message
- `main.rs`: Delete `parse_plugin_dependencies()` function (lines 1419-1452)
- `main.rs`: Delete `is_stdlib_module()` function (lines 1454-1465)

---

### Issue 4: Serialization Robustness (HIGH SEVERITY)

**Location:** `bridge_shim.py:269-270`

**Current State:**
```python
elif isinstance(data, pd.DataFrame):
    table = pa.Table.from_pandas(data)  # Can throw ArrowInvalid!
```

**Scenario:**
1. CSV has "Age" column: rows 1-999 are integers, row 1000 is "Unknown"
2. pandas reads as `object` dtype (mixed int/string)
3. `pa.Table.from_pandas(df)` throws `ArrowInvalid`
4. Python crashes, Rust sees "subprocess exited unexpectedly"
5. Bad row **never reaches Rust** for quarantine

**Problem:** Quarantine architecture assumes bad data reaches Rust. If shim crashes during Arrow conversion, quarantine is useless.

**Resolution:** Add `safe_to_arrow()` that falls back to strings for problematic columns.

```python
# bridge_shim.py - ADD this function

def safe_to_arrow(df: pd.DataFrame) -> pa.Table:
    """
    Convert DataFrame to Arrow with fallback for mixed-type columns.

    Ensures data always reaches Rust for quarantine processing,
    rather than crashing in Python due to mixed types.
    """
    try:
        return pa.Table.from_pandas(df)
    except (pa.ArrowInvalid, pa.ArrowTypeError) as e:
        # Log which columns are problematic
        for col in df.columns:
            if df[col].dtype == 'object':
                try:
                    pa.array(df[col])
                except:
                    # Force to string - let Rust-side validation handle it
                    logger.warning(f"Column '{col}' has mixed types, converting to string")
                    df[col] = df[col].astype(str)

        # Retry with sanitized DataFrame
        return pa.Table.from_pandas(df)
```

Then update `publish()` method:
```python
# bridge_shim.py line 269-270 - CHANGE to:
elif isinstance(data, pd.DataFrame):
    table = safe_to_arrow(data)  # Was: pa.Table.from_pandas(data)
```

**Why string fallback?**
- Strings always serialize cleanly to Arrow
- The data reaches Rust intact
- Rust-side schema validation catches the type mismatch
- Quarantine can isolate the offending rows
- User gets actionable error: "Row 1000: 'Unknown' is not Int64"

---

## Proposed State: Modal Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      MODAL ARCHITECTURE                                  │
│                                                                         │
│   DEV MODE (casparian run)              PROD MODE (casparian worker)    │
│   ────────────────────────              ────────────────────────────    │
│                                                                         │
│   ┌─────────────┐                       ┌─────────────┐                 │
│   │ Parser.py   │ ← File on disk        │  cf_parsers │ ← ZIP in DB    │
│   │ (editable)  │                       │  (bundled)  │                 │
│   └──────┬──────┘                       └──────┬──────┘                 │
│          │                                     │                        │
│          ▼                                     ▼                        │
│   ┌─────────────┐                       ┌─────────────┐                 │
│   │ Current     │ ← $VIRTUAL_ENV        │ Managed     │ ← ~/.casparian │
│   │ Python      │    or system          │ Venv        │    /venvs/     │
│   └──────┬──────┘                       └──────┬──────┘                 │
│          │                                     │                        │
│          ▼                                     ▼                        │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                     SHARED EXECUTOR                              │   │
│   │  - Single shim: bridge_shim.py                                   │   │
│   │  - Single protocol: Unix socket + binary [LENGTH:4][ARROW_IPC]   │   │
│   │  - Safe serialization: safe_to_arrow() with string fallback      │   │
│   │  - Same result handling                                          │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│          │                                     │                        │
│          ▼                                     ▼                        │
│   ┌─────────────┐                       ┌─────────────┐                 │
│   │ Write       │ ← STATELESS           │ Record job  │ ← BEFORE exec  │
│   │ output.parq │    (no DB)            │ (required)  │    (for queue) │
│   └─────────────┘                       └─────────────┘                 │
└─────────────────────────────────────────────────────────────────────────┘
```

### The Runner Trait

```rust
/// Core abstraction: execution is separate from orchestration
trait Runner {
    fn execute(&self, parser: &ParserRef, input: &Path, log_dest: LogDestination) -> Result<Output>;
}

/// Where parser output (stdout/stderr) goes
pub enum LogDestination {
    /// Dev mode: pipe to terminal for debugging (pdb, print statements work)
    Terminal,

    /// Prod mode: write to per-job log file
    File(PathBuf),

    /// Future: stream to callback (UI, websocket, etc.)
    Callback(Arc<dyn Fn(&str) + Send + Sync>),
}

struct DevRunner;      // File on disk, current env, logs to Terminal
struct QueuedRunner;   // ZIP from DB, managed env, logs to File
```

**Why LogDestination matters:**
- Dev mode MUST pass stdout/stderr to terminal for `pdb.set_trace()` to work
- Prod mode MUST capture logs for debugging failed jobs
- Without explicit handling, dev mode loses its primary advantage (debugging)

Both runners share:
- Same shim: `bridge_shim.py`
- Same protocol: Unix socket with binary framing `[LENGTH:4][ARROW_IPC]`
- Same serialization: `safe_to_arrow()` with string fallback
- Same output format (Arrow IPC batches)
- Same result type (`Output` with rows, errors, warnings)

They differ on:
- Where parser source comes from (disk vs database)
- Where Python interpreter comes from (current env vs managed venv)
- Database interaction (none vs queue-based)

### Execution Modes

| Mode | Command | Runner | DB Timing |
|------|---------|--------|-----------|
| **Dev** | `casparian run parser.py input.csv` | `DevRunner` | **None** (stateless) |
| **Batch** | `casparian process` | `QueuedRunner` | Before |
| **Daemon** | `casparian start --watch /data` | `QueuedRunner` | Before |

### What's Shared, What's Not

| Component | Shared? | Notes |
|-----------|---------|-------|
| Shim | ✅ Yes | `bridge_shim.py` (single shim) |
| Protocol | ✅ Yes | Unix socket: `[LENGTH:4][ARROW_IPC]` |
| Serialization | ✅ Yes | `safe_to_arrow()` with string fallback |
| Output format | ✅ Yes | Arrow IPC batches |
| Result handling | ✅ Yes | Success, failure, warnings |
| Parser source | ❌ No | Dev: disk, Prod: DB |
| Python env | ❌ No | Dev: current, Prod: managed |
| Database | ❌ No | Dev: **none** (stateless), Prod: queue-based |

### Progressive Parser API

To lower the barrier to entry while maintaining structure for production, support two API levels:

| Level | Supported In | Use Case | Example |
|-------|--------------|----------|---------|
| **Level 1: Function** | Dev only | Quick iteration, exploration | `def parse(path): return pd.read_csv(path)` |
| **Level 2: Class** | Dev + Prod | Production registration | `class Parser` with metadata |

```python
# Level 1: Dev-only (ad-hoc mode)
# Works with: casparian run script.py input.csv
# Cannot register for production
def parse(file_path):
    return pd.read_csv(file_path)

# Level 2: Prod-ready (registerable)
# Works with: casparian run, casparian parser register
class Parser:
    name = "invoice_parser"
    version = "1.0.0"
    outputs = {"amount": "float", "date": "date"}

    def parse(self, ctx):
        return pd.read_csv(ctx.input_path)
```

**The "Validation Cliff" Mitigation:**

When running Level 1 code, emit a warning:

```bash
$ casparian run script.py input.csv
⚠ Running in ad-hoc mode. This parser cannot be registered for production.
  To register, wrap in a Parser class: casparian scaffold script.py

Processing input.csv...
✓ 1000 rows processed
```

**Scaffold Command:**

```bash
$ casparian scaffold script.py
Generated parser_scaffold.py:

class Parser:
    name = "script"  # TODO: Update
    version = "1.0.0"
    outputs = {}     # TODO: Define output schema

    def parse(self, ctx):
        # Your original code:
        return pd.read_csv(ctx.input_path)
```

---

## Critical Engineering Gaps

Six critical gaps were identified that would cause runtime failures:

### Gap 1: Dependency Hell (Environment Persistence)

**Problem:** The plan registers parsers but ignores Python environments. If a user runs `casparian run` with a local venv, and later the daemon picks up a job, **the daemon doesn't know which Python to use**.

**Failure Scenario:**
```
Day 1: User runs `casparian run parser.py input.csv` with local .venv
       Parser registered with source_code only

Day 2: Daemon picks up job for this parser
       Daemon: "What Python interpreter should I use?" → CRASH
```

**Fix:** Store `lockfile_content` and `lockfile_hash` with each parser. Workers use content-addressed venv cache (`~/.casparian_flow/venvs/{hash}/`) and rebuild from lockfile if needed.

### Gap 2: Double Claim Race Condition (SQLite Concurrency)

**Problem:** SQLite doesn't support `SELECT ... FOR UPDATE`. Two concurrent processes can claim the same pending job.

**Failure Scenario:**
```
Process A                              Process B
─────────                              ─────────
SELECT * FROM cf_jobs
WHERE status='pending' LIMIT 1;
  → Returns job_123                    SELECT * FROM cf_jobs
                                       WHERE status='pending' LIMIT 1;
                                         → Returns job_123 (SAME JOB!)

UPDATE cf_jobs SET status='running'... UPDATE cf_jobs SET status='running'...

Both think they own job_123 → DISASTER
```

**Fix:** Use atomic `UPDATE ... RETURNING` pattern:

```sql
UPDATE cf_jobs
SET status = 'running', worker_pid = ?, started_at = ?
WHERE job_id = (
    SELECT job_id FROM cf_jobs
    WHERE status = 'pending'
    ORDER BY created_at LIMIT 1
)
RETURNING *;
```

### Gap 3: Shim Protocol Mismatch

**Problem:** `run_shim.py` and `bridge_shim.py` are different files with incompatible protocols:
- `run_shim.py`: ZMQ PUSH/PULL with JSON messages containing Base64-encoded Arrow
- `bridge_shim.py`: Unix socket with binary `[LENGTH:4][ARROW_IPC]`

**Fix:** Converge on `bridge_shim.py` with Unix socket protocol. Delete `run_shim.py`.

Rationale:
- Unix sockets are simpler (no ZMQ dependency for shim)
- Binary Arrow IPC is more efficient (no Base64 overhead)
- `bridge.rs` already has working implementation
- macOS-first means Unix sockets are fine

See [Critical Code Fixes: Issue 1](#issue-1-protocol-divergence-high-severity) for implementation details.

### Gap 4: Database Coupling (The "SQL Trap")

**Problem:** The original plan embeds SQLite-specific constructs directly into business logic:
- `PRAGMA busy_timeout` - SQLite only, errors on Postgres
- `PRAGMA journal_mode = WAL` - SQLite only
- `UPDATE ... RETURNING` with `LIMIT` subquery - different behavior across DBs

**Failure Scenario:**
```
Day 1: Deploy with SQLite (works)
Day 2: Need to scale, migrate to PostgreSQL
Day 3: PRAGMA statements error, job claiming breaks
Day 4: Rewrite entire JobQueue struct
```

**Fix:** Repository Pattern (trait-based storage). Define abstract `JobStore` and `ParserStore` traits. SQLite-specific SQL lives in `SqliteJobStore` implementation. Future Postgres support implements same trait with `FOR UPDATE SKIP LOCKED`.

### Gap 5: Local Import Failure (Parser Bundling)

**Problem:** Plan stores `source_code` as a single file's content. Real parsers often have multiple files:

```
my_parser/
├── parser.py      # Main entry point: from .utils import clean_date
├── utils.py       # Helper functions
├── models.py      # Data classes
└── __init__.py
```

If only `parser.py` is sent to worker: `ModuleNotFoundError: No module named 'utils'`

**Risk:** Users rarely write complex parsers in a single file. This breaks real-world usage.

**Fix:** Store ZIP archive of parser directory, not just single file text. Shim unzips to temp dir before execution.

### Gap 6: Zombie Detection (Lightweight Heartbeats)

**Problem:** How do we detect zombie jobs (worker died mid-execution)?

**Why Not PID-Based Detection:**

PID-based detection (checking if `worker_pid` process exists) has a fatal flaw: **PID reuse**.

```
1. Worker (PID 12345) claims job, starts processing
2. Orchestrator crashes
3. Worker exits, PID 12345 becomes available
4. Some OTHER process (Chrome, cron) spawns with PID 12345
5. Orchestrator restarts, checks: "Is PID 12345 alive?" → YES
6. Job stuck as "running" forever (silent failure)
```

On Linux with default `pid_max=32768`, PID reuse can happen within minutes on busy systems.

**Solution: Lightweight Heartbeats**

Use infrequent heartbeats that create negligible DB pressure:

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Heartbeat interval | 60 seconds | 1 write/minute/worker = trivial for SQLite |
| Stale threshold | 5 minutes | Generous buffer for slow operations |
| Cleanup frequency | On startup + every 5 minutes | Catches crashes without polling constantly |

```rust
impl QueuedRunner {
    /// Called every 60 seconds by worker during job execution
    async fn heartbeat(&self, job_id: &str) -> Result<()> {
        self.job_store.heartbeat(job_id).await
    }

    /// Called on startup and every 5 minutes
    async fn cleanup_stale_jobs(&self) -> Result<Vec<String>> {
        let threshold = Duration::from_secs(300); // 5 minutes
        self.job_store.requeue_stale(threshold).await
    }
}
```

**DB Overhead Analysis:**

With 4 concurrent workers:
- Heartbeat writes: 4 workers × 1 write/min = 240 writes/hour
- SQLite can handle 50,000+ writes/second
- Overhead: **0.001%** of capacity

This is not "distributed cosplay" - it's pragmatic engineering that:
1. Has no edge cases (unlike PID reuse)
2. Works if we ever go multi-node
3. Requires no platform-specific code
4. Is simple to understand and debug

### Optional Enhancement: Worker ID Persistence

The heartbeat approach handles correctness (stale jobs get requeued). However, persisting worker_id provides additional benefits:

**Use cases:**
1. **Duplicate daemon prevention**: Lock file with PID + worker_id prevents starting two daemons
2. **Log correlation**: Know which daemon processed which jobs across restarts
3. **Graceful resume**: Optionally resume own jobs instead of requeuing on restart

**Implementation (optional):**

```rust
// ~/.casparian_flow/daemon.lock
struct DaemonLock {
    worker_id: String,  // UUID, persisted across restarts
    pid: u32,
    started_at: i64,
}

fn acquire_daemon_lock() -> Result<DaemonLock> {
    let lock_path = data_dir().join("daemon.lock");

    if lock_path.exists() {
        let existing: DaemonLock = read_json(&lock_path)?;
        if process_exists(existing.pid) {
            bail!("Daemon already running (PID {})", existing.pid);
        }
        // Previous daemon crashed, reuse worker_id for log continuity
        return Ok(DaemonLock {
            worker_id: existing.worker_id,
            pid: std::process::id(),
            started_at: timestamp_ms(),
        });
    }

    // Fresh start
    Ok(DaemonLock {
        worker_id: Uuid::new_v4().to_string(),
        pid: std::process::id(),
        started_at: timestamp_ms(),
    })
}
```

**Priority:** Low. Heartbeat-based zombie detection is sufficient for correctness. This is a polish feature for multi-daemon deployments.

---

## Database Abstraction Strategy

### The Problem: SQL Lock-in

Embedding raw SQL in business logic creates tight coupling:

```rust
// BAD: SQLite-specific SQL in business logic
impl JobQueue {
    async fn claim_job(&self) -> Result<Option<Job>> {
        sqlx::query("PRAGMA busy_timeout = 5000").execute(&self.pool).await?;  // SQLite only!
        sqlx::query_as("UPDATE cf_jobs SET ... WHERE job_id = (SELECT ... LIMIT 1) RETURNING *")
            .fetch_optional(&self.pool).await  // Different syntax on Postgres
    }
}
```

### The Solution: Repository Pattern

Define abstract traits, implement per-database:

```rust
/// Abstract job storage - database agnostic
#[async_trait]
pub trait JobStore: Send + Sync {
    /// Insert a new job
    async fn insert(&self, job: &Job) -> Result<()>;

    /// Atomically claim the next pending job
    /// Implementation handles DB-specific locking
    async fn claim_next(&self, worker_id: &str) -> Result<Option<Job>>;

    /// Mark job as completed with result
    async fn complete(&self, job_id: &str, result: &JobResult) -> Result<()>;

    /// Mark job as failed with error message
    async fn fail(&self, job_id: &str, error: &str) -> Result<()>;

    /// Requeue a specific job
    async fn requeue(&self, job_id: &str) -> Result<()>;

    /// Update heartbeat timestamp for a running job
    async fn heartbeat(&self, job_id: &str) -> Result<()>;

    /// Requeue jobs with stale heartbeats (returns requeued job IDs)
    async fn requeue_stale(&self, threshold: Duration) -> Result<Vec<String>>;

    /// Get job by ID
    async fn get(&self, job_id: &str) -> Result<Option<Job>>;

    /// List jobs matching filter
    async fn list(&self, filter: &JobFilter) -> Result<Vec<Job>>;

    /// Check if job exists for input hash + parser (dedup)
    async fn exists_for_hash(&self, input_hash: &str, parser_id: &str) -> Result<bool>;
}

/// Abstract parser storage - database agnostic
#[async_trait]
pub trait ParserStore: Send + Sync {
    async fn insert(&self, parser: &Parser) -> Result<()>;
    async fn get(&self, parser_id: &str) -> Result<Option<Parser>>;
    async fn get_by_name_version(&self, name: &str, version: &str) -> Result<Option<Parser>>;
    async fn find_by_topic(&self, topic: &str) -> Result<Vec<Parser>>;
    async fn list(&self, filter: &ParserFilter) -> Result<Vec<ParserSummary>>;
}
```

Note: No `heartbeat()` or `cleanup_zombies()` methods. Zombie detection is handled at the OS level via `waitpid()`, not via database polling.

### SQLite Implementation

```rust
pub struct SqliteJobStore {
    pool: SqlitePool,
}

impl SqliteJobStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .connect(database_url).await?;

        // SQLite-specific initialization
        sqlx::query("PRAGMA journal_mode = WAL").execute(&pool).await?;
        sqlx::query("PRAGMA busy_timeout = 5000").execute(&pool).await?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl JobStore for SqliteJobStore {
    async fn claim_next(&self, worker_id: &str) -> Result<Option<Job>> {
        let now = timestamp_ms();

        // SQLite-specific: UPDATE...RETURNING with subquery
        let job = sqlx::query_as::<_, Job>(r#"
            UPDATE cf_jobs
            SET
                status = 'running',
                worker_id = ?,
                claimed_at = ?
            WHERE job_id = (
                SELECT job_id
                FROM cf_jobs
                WHERE status = 'pending'
                ORDER BY created_at
                LIMIT 1
            )
            RETURNING *
        "#)
        .bind(worker_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        Ok(job)
    }

    async fn requeue(&self, job_id: &str) -> Result<()> {
        sqlx::query(r#"
            UPDATE cf_jobs
            SET status = 'pending', worker_id = NULL, claimed_at = NULL
            WHERE job_id = ?
        "#)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ... other methods
}
```

### Future PostgreSQL Implementation

```rust
pub struct PostgresJobStore {
    pool: PgPool,
}

#[async_trait]
impl JobStore for PostgresJobStore {
    async fn claim_next(&self, worker_id: &str) -> Result<Option<Job>> {
        let now = timestamp_ms();

        // Postgres-specific: FOR UPDATE SKIP LOCKED
        let job = sqlx::query_as::<_, Job>(r#"
            UPDATE cf_jobs
            SET
                status = 'running',
                worker_id = $1,
                claimed_at = $2
            WHERE job_id = (
                SELECT job_id
                FROM cf_jobs
                WHERE status = 'pending'
                ORDER BY created_at
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING *
        "#)
        .bind(worker_id)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        Ok(job)
    }

    // Same interface, different SQL
}
```

### Worker Loop (Heartbeat-Based)

```rust
/// Tracks a running worker subprocess
struct WorkerProcess {
    job_id: String,
    child: Child,  // std::process::Child
    last_heartbeat: Instant,
}

async fn worker_loop(
    job_store: Arc<dyn JobStore>,
    parser_store: Arc<dyn ParserStore>,
    env_manager: Arc<EnvManager>,
) -> Result<()> {
    let worker_id = Uuid::new_v4().to_string();
    let mut active_workers: Vec<WorkerProcess> = Vec::new();
    let mut last_stale_check = Instant::now();

    // On startup, clean up any stale jobs from previous crash
    let requeued = job_store.requeue_stale(Duration::from_secs(300)).await?;
    if !requeued.is_empty() {
        info!("Startup cleanup: requeued {} stale jobs", requeued.len());
    }

    loop {
        // Periodic stale job cleanup (every 5 minutes)
        if last_stale_check.elapsed() > Duration::from_secs(300) {
            let requeued = job_store.requeue_stale(Duration::from_secs(300)).await?;
            if !requeued.is_empty() {
                warn!("Requeued {} stale jobs", requeued.len());
            }
            last_stale_check = Instant::now();
        }

        // Check for completed workers and send heartbeats
        active_workers.retain_mut(|w| {
            match w.child.try_wait() {
                Ok(Some(_status)) => {
                    // Process exited (success or failure handled by shim protocol)
                    false  // Remove from active list
                }
                Ok(None) => {
                    // Still running - send heartbeat if needed
                    if w.last_heartbeat.elapsed() > Duration::from_secs(60) {
                        let store = job_store.clone();
                        let job_id = w.job_id.clone();
                        tokio::spawn(async move {
                            let _ = store.heartbeat(&job_id).await;
                        });
                        w.last_heartbeat = Instant::now();
                    }
                    true  // Keep in list
                }
                Err(e) => {
                    error!("Failed to check worker status: {}", e);
                    false
                }
            }
        });

        // Try to claim a job (if we have capacity)
        if active_workers.len() < MAX_CONCURRENT_WORKERS {
            match job_store.claim_next(&worker_id).await? {
                Some(job) => {
                    let child = spawn_worker(&job, &parser_store, &env_manager)?;
                    active_workers.push(WorkerProcess {
                        job_id: job.job_id.clone(),
                        child,
                        last_heartbeat: Instant::now(),
                    });
                }
                None => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
```

---

## Parser Bundling Strategy

### The Problem: Single-File Assumption

Storing only `source_code` (single file content) breaks real-world parsers:

```python
# my_parser/parser.py
from .utils import clean_date, parse_amount  # FAILS: utils.py not sent to worker
from .models import Invoice                   # FAILS: models.py not sent to worker

class Handler:
    def parse(self, file_path):
        ...
```

### The Solution: Self-Contained ZIP Archive

The ZIP is a **single artifact** containing both code AND lockfile:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    PARSER BUNDLING FLOW                                 │
│                                                                         │
│  Registration                            Execution                      │
│  ────────────                            ─────────                      │
│                                                                         │
│  my_parser/                              Worker receives:               │
│  ├─ parser.py      ──┐                   ├─ source_archive (ZIP blob)   │
│  ├─ utils.py         │                   ├─ lockfile_hash (venv cache)  │
│  ├─ models.py        │ ZIP               │                              │
│  ├─ mappings.json    ├─────►             ▼                              │
│  └─ uv.lock ◄────────┘                   1. Check venv cache by hash    │
│      ▲                                   2. If miss: extract ZIP        │
│      │                                   3. Build venv from uv.lock     │
│  REQUIRED                                4. Cache venv for reuse        │
│  (enforced)                              5. Execute parser              │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key insight:** Lockfile travels WITH the code. They can't drift apart.

### What Gets Bundled

```
my_parser/
├── parser.py        ──┐
├── utils.py           │
├── models.py          │
├── mappings.json      ├──► INTO ZIP (single artifact)
├── uv.lock ◄──────────┤    └─ lockfile_hash computed for venv caching
│   (REQUIRED)         │
├── .venv/           ──┼──► EXCLUDED (rebuilt from lockfile)
├── __pycache__/     ──┼──► EXCLUDED
└── *.so             ──┘──► EXCLUDED (platform-specific)
```

### Registration Logic

```rust
pub fn bundle_parser(parser_dir: &Path) -> Result<ParserBundle> {
    // 1. Require uv.lock
    let lockfile_path = parser_dir.join("uv.lock");
    if !lockfile_path.exists() {
        bail!("No uv.lock found in {}. Run 'uv lock' first.", parser_dir.display());
    }

    // 2. Compute lockfile hash for venv caching
    let lockfile_content = fs::read_to_string(&lockfile_path)?;
    let lockfile_hash = sha256(lockfile_content.as_bytes());

    // 3. Bundle allowed files (including uv.lock)
    let archive = create_zip_archive(parser_dir, &BUNDLED_EXTENSIONS, &EXCLUDED_PATHS)?;
    let source_hash = sha256(&archive);

    // 4. Find entrypoint
    let entrypoint = find_entrypoint(parser_dir)?; // parser.py or __init__.py

    Ok(ParserBundle {
        source_archive: archive,
        source_hash,
        lockfile_hash,
        entrypoint,
    })
}
```

### Shim Execution Logic

```python
# worker_shim.py

def setup_parser_environment(archive_base64: str, entrypoint: str) -> ModuleType:
    """Unzip parser archive and import entrypoint module."""
    import base64
    import zipfile
    import tempfile
    import sys
    import importlib.util

    # Decode and extract archive
    archive_bytes = base64.b64decode(archive_base64)

    with tempfile.TemporaryDirectory() as temp_dir:
        # Extract ZIP
        with zipfile.ZipFile(io.BytesIO(archive_bytes)) as zf:
            zf.extractall(temp_dir)

        # Add to Python path
        sys.path.insert(0, temp_dir)

        # Import entrypoint module
        module_name = entrypoint.replace('.py', '')
        spec = importlib.util.spec_from_file_location(
            module_name,
            os.path.join(temp_dir, entrypoint)
        )
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        return module
```

### Validation at Registration

```rust
pub fn validate_parser_bundle(bundle: &ParserBundle) -> Result<()> {
    // 1. Verify entrypoint exists in archive
    let archive = ZipArchive::new(Cursor::new(&bundle.source_archive))?;
    if !archive.file_names().any(|n| n == bundle.entrypoint) {
        bail!("Entrypoint '{}' not found in archive", bundle.entrypoint);
    }

    // 2. Extract and verify Python syntax
    let entrypoint_content = archive.by_name(&bundle.entrypoint)?.read_to_string()?;
    verify_python_syntax(&entrypoint_content)?;

    // 3. Verify all local imports are satisfied
    let local_imports = extract_local_imports(&entrypoint_content);
    for import in local_imports {
        let import_file = format!("{}.py", import);
        if !archive.file_names().any(|n| n == import_file || n.ends_with(&format!("/{}", import_file))) {
            bail!("Local import '{}' not found in archive", import);
        }
    }

    Ok(())
}
```

### Source-Only Bundling (No Native Extensions)

**Problem:** If a user has `numpy` installed in their venv, the `site-packages/` contains `.so` files (macOS/Linux) or `.pyd` files (Windows). These are platform-specific binaries that won't work on a different OS or architecture.

**Solution:** Bundle source files only. Dependencies come from the lockfile, not from the archive.

**Allowlist for Bundling:**

```rust
const BUNDLED_EXTENSIONS: &[&str] = &[
    // Python source
    ".py",

    // Data files (often needed for parsers)
    ".json",
    ".yaml", ".yml",
    ".toml",
    ".csv",
    ".txt",
    ".xml",

    // Templates (some parsers use Jinja, etc.)
    ".j2", ".jinja", ".jinja2",

    // Schema files
    ".xsd", ".dtd",
];

const EXCLUDED_PATHS: &[&str] = &[
    // Virtual environments (NEVER bundle)
    "venv/", ".venv/", "env/", ".env/",
    "__pycache__/",
    "*.pyc",

    // Build artifacts
    "*.egg-info/",
    "build/", "dist/",

    // Native extensions (platform-specific, come from lockfile)
    "*.so", "*.dylib", "*.dll", "*.pyd",

    // IDE/editor
    ".git/", ".idea/", ".vscode/",
];

pub fn bundle_parser(parser_path: &Path) -> Result<ParserBundle> {
    let parser_dir = parser_path.parent().unwrap_or(Path::new("."));

    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)?); // Canonical timestamp

    for entry in WalkDir::new(parser_dir) {
        let entry = entry?;
        let path = entry.path();

        // Skip excluded paths
        if is_excluded(path) {
            continue;
        }

        // Only include allowlisted extensions
        if !is_allowed_extension(path) {
            continue;
        }

        // Add to archive
        let relative = path.strip_prefix(parser_dir)?;
        archive.start_file(relative.to_string_lossy(), options)?;
        archive.write_all(&fs::read(path)?)?;
    }

    let archive_bytes = archive.finish()?.into_inner();
    // ... rest of bundling logic
}
```

**Why This Matters:**

| Without Allowlist | With Allowlist |
|-------------------|----------------|
| `my_parser.zip`: 150MB (includes numpy .so files) | `my_parser.zip`: 15KB (just .py + .json) |
| Fails on different platform | Works everywhere (deps from lockfile) |
| Non-deterministic hash (timestamps vary) | Deterministic hash (canonical timestamps) |

---

## Environment Strategy

### The Fundamental Question

**"Which Python runs this parser?"**

### Modal Environment Selection

The environment strategy differs between Dev and Prod modes:

| Mode | Python Source | When Used |
|------|---------------|-----------|
| **Dev** | `$VIRTUAL_ENV` → `.venv/` → system Python | `casparian run` (default) |
| **Prod** | Managed venv or `--python /path` | `casparian worker`, `casparian process` |

### Dev Mode: Current Environment (Default)

When iterating on a parser, use the developer's active environment:

```rust
impl DevRunner {
    fn resolve_python(&self) -> Result<PathBuf> {
        // 1. Check VIRTUAL_ENV (user activated a venv)
        if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
            return Ok(PathBuf::from(venv).join("bin/python"));
        }

        // 2. Check for .venv in parser directory
        let local_venv = self.parser_dir.join(".venv/bin/python");
        if local_venv.exists() {
            return Ok(local_venv);
        }

        // 3. Fall back to system Python
        which::which("python3").or_else(|_| which::which("python"))
    }
}
```

**Benefits:**
- Zero friction for development
- `pdb.set_trace()` works
- Changes to parser take effect immediately
- No hidden venv management

### Prod Mode: Managed or Explicit

For production/deployment, environments must be reproducible:

```rust
impl QueuedRunner {
    fn resolve_python(&self, parser: &Parser, temp_dir: &Path, config: &Config) -> Result<PathBuf> {
        // 1. Explicit override wins
        if let Some(python_path) = &config.python_path {
            return Ok(python_path.clone());
        }

        // 2. Check venv cache by lockfile_hash
        let cache_path = self.venv_cache.join(&parser.lockfile_hash);
        if cache_path.join("bin/python").exists() {
            return Ok(cache_path.join("bin/python"));
        }

        // 3. Build venv from lockfile (extracted from ZIP)
        let lockfile_path = temp_dir.join("uv.lock");  // Already extracted
        self.env_manager.build_venv(&lockfile_path, &cache_path, config.offline)?;
        Ok(cache_path.join("bin/python"))
    }
}
```

### Air-Gapped / Offline Support

Defense and health sectors often operate air-gapped networks:

**Requirements:**

1. **`--offline` flag**: Passes `--offline` to `uv sync`, preventing network access
2. **`--python` flag**: Explicit interpreter path (skip venv management entirely)
3. **`casparian vendor`**: Pre-download dependencies for air-gapped deployment

**CLI Support:**

```bash
# Dev: use current environment (default)
casparian run parser.py input.csv

# Prod: explicit Python path (no venv management)
casparian worker --python /opt/myapp/venv/bin/python

# Prod: managed venv, offline mode
casparian worker --offline

# Pre-download dependencies for air-gapped deployment
casparian vendor parser.py --output ./vendor
```

### When Venv Management Kicks In

Managed venvs are ONLY used in Prod mode when:
1. No `--python` flag is provided
2. Parser has a `lockfile_content` in the database

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    ENVIRONMENT DECISION TREE                            │
│                                                                         │
│   Is this Dev mode (casparian run)?                                     │
│   │                                                                     │
│   ├─ YES → Use current environment ($VIRTUAL_ENV / .venv / system)     │
│   │                                                                     │
│   └─ NO (Prod mode) → Is --python flag set?                            │
│       │                                                                 │
│       ├─ YES → Use specified Python path                               │
│       │                                                                 │
│       └─ NO → Does parser have lockfile?                               │
│           │                                                             │
│           ├─ YES → Ensure venv from lockfile (managed cache)           │
│           │                                                             │
│           └─ NO → Error: "Parser has no lockfile. Use --python or      │
│                          re-register with 'casparian parser register'" │
└─────────────────────────────────────────────────────────────────────────┘
```

### Environment Lifecycle

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    ENVIRONMENT LIFECYCLE                                │
│                                                                         │
│  User's Machine                          Worker (Local or Remote)       │
│  ──────────────────                      ─────────────────────────      │
│                                                                         │
│  parser.py                               cf_parsers table:              │
│  pyproject.toml ──┐                      ├─ source_archive (ZIP)        │
│  (or requirements)│                      ├─ lockfile_content            │
│       │           │                      └─ lockfile_hash               │
│       ▼           │                              │                      │
│  uv lock ─────────┼──► uv.lock ─────────────────►│                      │
│       │           │                              │                      │
│       ▼           │                              ▼                      │
│  .venv/ ◄─────────┘              ~/.casparian_flow/venvs/{hash}/        │
│  (user's local)                  (content-addressed cache)              │
│                                                                         │
│  User develops with                Worker rebuilds identical env        │
│  their local venv                  from stored lockfile                 │
└─────────────────────────────────────────────────────────────────────────┘
```

### Registration Flow (`casparian parser register` only)

**Dev mode (`casparian run`) NEVER touches the database.** Registration is explicit.

```
$ casparian parser register ./my_parser/

1. Validate uv.lock exists:
   └─ parser_dir/
      ├─ parser.py
      ├─ utils.py
      └─ uv.lock   ← REQUIRED (error if missing)

2. If uv.lock missing:
   → Error: "No uv.lock found. Run 'uv lock' first."
   → Exit (no auto-generation)

3. Compute lockfile_hash:
   → lockfile_hash = sha256(read("uv.lock"))

4. Bundle ZIP (code + lockfile + data files):
   → source_archive = zip(allowed_files)
   → source_hash = sha256(source_archive)

5. Store in cf_parsers:
   → source_archive (BLOB)
   → source_hash (integrity check)
   → lockfile_hash (venv cache key)
```

### Worker Execution Flow

```
1. Look up parser in cf_parsers:
   → Get source_archive, lockfile_hash

2. Check venv cache:
   cache_path = ~/.casparian_flow/venvs/{lockfile_hash}/

   If cache_path/bin/python exists:
   → Use cached venv (fast path, skip to step 5)

3. Extract ZIP to temp dir:
   /tmp/casparian_xyz/
   ├─ parser.py
   ├─ utils.py
   └─ uv.lock    ← Lockfile is in the ZIP

4. Build venv from extracted lockfile:
   $ cd /tmp/casparian_xyz && uv sync --locked
   $ mv .venv ~/.casparian_flow/venvs/{lockfile_hash}/

5. Execute parser:
   ~/.casparian_flow/venvs/{lockfile_hash}/bin/python parser.py
```

### Self-Healing

```
If execution fails (interpreter missing, corrupted venv):
1. Delete cache_path
2. Re-extract ZIP to get fresh uv.lock
3. Retry once
4. If still fails → mark job failed with clear error
```

### Edge Cases

| Scenario | Handling |
|----------|----------|
| User deletes their .venv | Worker rebuilds from lockfile in ZIP |
| User updates parser but not lockfile | ImportError at runtime (user must re-lock) |
| Same lockfile for multiple parsers | Shared venv (saves disk space) |
| Worker on different OS | uv sync generates platform-specific venv |
| No uv.lock in parser directory | Error at registration: "Run 'uv lock' first" |
| uv not installed on worker | Clear error with install instructions |
| uv sync fails (network/conflict) | Pristine error with full context (see below) |
| Concurrent venv creation | File lock prevents race condition (see below) |

### Error Reporting for uv sync (CRITICAL)

When `uv sync` fails, users must understand WHY. Generic "Worker failed" is unacceptable.

**Required error format:**

```
Error: Failed to create environment for parser 'invoice_parser'

Command: uv sync --locked
Working dir: /tmp/casparian_abc123/
Exit code: 1

Stderr:
  × No solution found when resolving dependencies:
  ╰─▶ Because pandas>=2.0 depends on numpy>=1.23 and your project
      depends on numpy==1.21, we can conclude that your project's
      requirements are unsatisfiable.

Suggestions:
  1. Verify lockfile is valid: uv lock --check
  2. Re-lock dependencies: cd /path/to/parser && uv lock
  3. Re-deploy parser: casparian parser register ./parser --force
  4. Check network connectivity (if --offline not set)
```

**Implementation:**

```rust
fn build_venv(lockfile_path: &Path, cache_path: &Path) -> Result<()> {
    let output = Command::new("uv")
        .args(["sync", "--locked"])
        .current_dir(lockfile_path.parent().unwrap())
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Failed to create environment for parser\n\n\
             Command: uv sync --locked\n\
             Working dir: {}\n\
             Exit code: {}\n\n\
             Stderr:\n{}\n\n\
             Suggestions:\n\
             1. Verify lockfile is valid: uv lock --check\n\
             2. Re-lock dependencies: uv lock\n\
             3. Re-deploy parser with --force\n\
             4. Check network connectivity",
            lockfile_path.parent().unwrap().display(),
            output.status.code().unwrap_or(-1),
            stderr
        );
    }
    Ok(())
}
```

### Venv Creation Race Condition

**Problem:** Two workers process same parser simultaneously, both try to create venv.

**Solution:** File-based locking during venv creation:

```rust
fn ensure_venv(lockfile_hash: &str, lockfile_path: &Path) -> Result<PathBuf> {
    let cache_path = venv_cache_dir().join(lockfile_hash);
    let lock_path = cache_path.with_extension("building.lock");

    // Fast path: venv already exists
    if cache_path.join("bin/python").exists() {
        return Ok(cache_path);
    }

    // Acquire exclusive lock (blocks if another worker is building)
    let lock_file = File::create(&lock_path)?;
    lock_file.lock_exclusive()?;  // Uses fs2 crate

    // Double-check after acquiring lock (another worker may have finished)
    if cache_path.join("bin/python").exists() {
        return Ok(cache_path);
    }

    // Build venv
    build_venv(lockfile_path, &cache_path)?;

    // Release lock (automatic on drop, but explicit for clarity)
    drop(lock_file);
    fs::remove_file(&lock_path).ok();  // Clean up lock file

    Ok(cache_path)
}
```

**Why this matters:** Without locking, two workers might:
1. Both see venv doesn't exist
2. Both start `uv sync`
3. Race condition corrupts venv OR wastes resources

---

## Validation Strategy

### The Problem: Hidden Dependencies

Local dry-run validation catches Python issues (import errors, lockfile problems) but misses:

- **Binary dependencies**: `subprocess.run(["pdftotext", ...])` works locally, fails on worker
- **System libraries**: `libpq.so`, `libssl.so` required by some packages
- **Environment variables**: `$TESSDATA_PREFIX`, `$DATABASE_URL`
- **Hardcoded paths**: `/Users/john/config.json`
- **Hardware**: CUDA/GPU dependencies

### Layered Validation Approach

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    VALIDATION LAYERS                                     │
│                                                                         │
│  Layer 1: Local Dry-Run (always, during registration)                   │
│  ─────────────────────────────────────────────────────                  │
│  • Build venv from lockfile locally                                     │
│  • Extract ZIP, try to import entrypoint                                │
│  • Catches: lockfile issues, syntax errors, import errors               │
│                                                                         │
│  Layer 2: Good Error Messages (always, at runtime)                      │
│  ─────────────────────────────────────────────────────                  │
│  • Fast failure with file:line information                              │
│  • Actionable hints: "Install poppler-utils on worker"                  │
│  • Catches: everything else, with clear output                          │
│                                                                         │
│  Layer 3: Remote Validation (opt-in, --validate-remote)                 │
│  ─────────────────────────────────────────────────────                  │
│  • Send ZIP to running worker                                           │
│  • Worker builds venv, runs test execution                              │
│  • Catches: system deps, env vars, OS differences                       │
└─────────────────────────────────────────────────────────────────────────┘
```

### Layer 1: Local Dry-Run (Built into Registration)

```bash
$ casparian parser register ./my_parser/

Validating parser...
✓ Found uv.lock
✓ Bundled 5 files (24KB)
✓ Building venv from lockfile...
✓ Venv created (pandas 2.0.0, pyarrow 14.0.0)
✓ Import test: parser.py loads successfully

Registered 'invoice_parser' v1.0.0
```

### Layer 2: Actionable Error Messages

When a parser fails in production, provide clear guidance:

```
Job abc123 FAILED (0.3s)
Parser: invoice_parser v1.0.0
Phase: Execution

Error: FileNotFoundError: [Errno 2] No such file or directory: 'pdftotext'
  File "parser.py", line 47
    subprocess.run(["pdftotext", input_path, "-"])

Hint: Parser requires system binary 'pdftotext'.
      Install on worker: apt install poppler-utils
```

### Layer 3: Remote Validation (Opt-In)

For parsers with system dependencies or cross-platform deployments:

```bash
$ casparian parser register ./my_parser/ --validate-remote

Uploading to worker for validation...
Worker: Building venv from lockfile...
Worker: ✓ Venv created
Worker: Running test execution...
Worker: ✗ FAILED: FileNotFoundError: 'pdftotext' not found
        at parser.py:47

Registration aborted. Fix the issue and retry.
```

**Implementation:**

```rust
enum JobType {
    Parse,      // Normal job - process file, write output
    Validate,   // Test job - run parser, discard output, report status
}

// Validation job payload
struct ValidatePayload {
    source_archive: Vec<u8>,  // ZIP blob (not yet committed)
    lockfile_hash: String,
    test_file: Option<String>, // Optional sample input
}
```

### When to Use Each Layer

| Scenario | Layer 1 | Layer 2 | Layer 3 |
|----------|---------|---------|---------|
| Pure Python parser | ✅ Sufficient | Backup | Overkill |
| Parser with binary deps | Partial | Helpful | ✅ Recommended |
| Dev machine = Prod | ✅ Sufficient | Backup | Overkill |
| macOS dev → Linux prod | Partial | Helpful | ✅ Recommended |
| CI/CD pipeline | ✅ Required | Required | Optional |

### CLI Commands

```bash
# Test parser locally (simulates prod execution)
$ casparian parser test ./my_parser/ sample.csv
Building isolated venv...
Executing parser...
✓ Processed 1000 rows

# Register with local validation only (default)
$ casparian parser register ./my_parser/

# Register with remote validation
$ casparian parser register ./my_parser/ --validate-remote

# Register and skip validation (for CI/CD that validates separately)
$ casparian parser register ./my_parser/ --skip-validation
```

---

## Quarantine Pattern

### The Problem: Binary Success/Failure

The original plan treated jobs as binary: success or failure. But real-world data is messy:

```
10,000 rows in file
9,998 rows parse correctly
2 rows have invalid dates

Original behavior: JOB FAILED (entire file rejected)
Desired behavior: JOB SUCCEEDED with 2 warnings
```

If a critical daily report fails because 2 rows out of 10,000 have bad data, the user doesn't care about "contract purity" - they care that their dashboard is empty.

### The Solution: Rust-Side Validation

**Key Architectural Decision:** Validation happens in Rust, not Python.

| Layer | Role | Responsibility |
|-------|------|----------------|
| **Python Shim** | Ingest & Stream | Parse file, serialize to Arrow, stream batches |
| **Rust Executor** | Enforce & Store | Validate against Schema Contract, split valid/invalid |

This keeps the shim simple and leverages Rust's performance for row-level operations.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    QUARANTINE FLOW (Rust-Side Validation)               │
│                                                                         │
│   Python Shim                          Rust Executor                    │
│   ───────────                          ─────────────                    │
│                                                                         │
│   ┌──────────────┐                     ┌──────────────────────────────┐ │
│   │ Parse file   │                     │ Receive Arrow batch          │ │
│   │ (lenient)    │ ─── ZMQ batch ────► │                              │ │
│   └──────────────┘                     │ Validate each row against    │ │
│                                        │ Schema Contract              │ │
│   Shim is DUMB:                        │                              │ │
│   - Just parse                         │ Split:                       │ │
│   - Stream ALL rows                    │ ├─ Valid → output.parquet    │ │
│   - No validation                      │ └─ Invalid → cf_quarantine   │ │
│                                        └──────────────────────────────┘ │
│                                                                         │
│   Protocol: {batch, done, error}       Job Status: completed_with_warnings│
│   (no quarantine message needed)       Warning: "2 rows quarantined"    │
└─────────────────────────────────────────────────────────────────────────┘
```

### Safe Serialization in Shim (CRITICAL)

**The Serialization Gap:** If Python yields a DataFrame with mixed types (integers and strings in same column), `pyarrow.Table.from_pandas(df)` throws `ArrowInvalid` before data reaches Rust.

**Example Scenario:**
```
CSV "users.csv":
Row 1-999:  Age=25, Age=30, ...  (integers)
Row 1000:   Age="Unknown"        (string - poison pill)

Result: pandas reads Age as object dtype (mixed)
        pa.Table.from_pandas(df) → ArrowInvalid exception
        Python crashes, Rust sees "subprocess exited"
        Bad row NEVER reaches Rust for quarantine
```

**Mitigation:** The shim MUST use `safe_to_arrow()` with fallback to strings:

```python
def safe_to_arrow(df: pd.DataFrame) -> pa.Table:
    """
    Convert DataFrame to Arrow with fallback for mixed-type columns.

    Ensures data always reaches Rust for quarantine processing,
    rather than crashing in Python due to mixed types.
    """
    try:
        return pa.Table.from_pandas(df)
    except (pa.ArrowInvalid, pa.ArrowTypeError) as e:
        # Log which columns are problematic
        for col in df.columns:
            if df[col].dtype == 'object':
                try:
                    pa.array(df[col])
                except:
                    # Force to string - let Rust-side validation handle it
                    logger.warning(f"Column '{col}' has mixed types, converting to string")
                    df[col] = df[col].astype(str)

        # Retry with sanitized DataFrame
        return pa.Table.from_pandas(df)
```

**Why string fallback works:**
1. Strings always serialize cleanly to Arrow
2. Data reaches Rust intact
3. Rust-side schema validation catches type mismatch
4. Quarantine isolates offending rows
5. User gets actionable error: "Row 1000: 'Unknown' is not Int64"

See [Critical Code Fixes: Issue 4](#issue-4-serialization-robustness-high-severity) for implementation details.

### Job Status Values

```rust
pub enum JobStatus {
    Pending,
    Running,
    Completed,              // All rows valid
    CompletedWithWarnings,  // Some rows quarantined
    Failed,                 // Parser error or schema violation
}
```

### Quarantine Table

```sql
CREATE TABLE cf_quarantine (
    quarantine_id     TEXT PRIMARY KEY,
    job_id            TEXT NOT NULL,
    row_number        INTEGER NOT NULL,
    raw_data          TEXT NOT NULL,      -- Original row as JSON
    error_type        TEXT NOT NULL,      -- 'invalid_date', 'null_required', etc.
    error_message     TEXT NOT NULL,
    column_name       TEXT,               -- Which column failed
    created_at        INTEGER NOT NULL,

    FOREIGN KEY (job_id) REFERENCES cf_jobs(job_id)
);

CREATE INDEX idx_quarantine_job ON cf_quarantine(job_id);
```

### Validation During Execution (Rust Side)

The Rust executor receives Arrow batches and validates against the Schema Contract:

```rust
impl Executor {
    fn process_batch(
        &self,
        batch: RecordBatch,
        schema: &SchemaContract,
        job_id: &str,
    ) -> Result<BatchResult> {
        let mut valid_indices = Vec::new();
        let mut quarantine_entries = Vec::new();

        for row_idx in 0..batch.num_rows() {
            match schema.validate_row(&batch, row_idx) {
                Ok(()) => valid_indices.push(row_idx),
                Err(ValidationError { column, error_type, message }) => {
                    quarantine_entries.push(QuarantineEntry {
                        job_id: job_id.to_string(),
                        row_number: row_idx as i64,
                        raw_data: self.row_to_json(&batch, row_idx),
                        error_type: error_type.to_string(),
                        error_message: message,
                        column_name: Some(column),
                    });
                }
            }
        }

        // Filter batch to only valid rows (efficient Arrow operation)
        let valid_batch = batch.slice_by_indices(&valid_indices)?;

        Ok(BatchResult {
            valid_batch,
            quarantine_entries,
        })
    }
}
```

**Performance Note:** Arrow's columnar format makes row-level validation efficient in Rust. The `slice_by_indices` operation is O(1) for each column (just pointer arithmetic).

### User Workflow: Replay Quarantine

```bash
# View quarantined rows
$ casparian quarantine list --job abc123
Job abc123: 2 rows quarantined

Row 47:   {"date": "31/02/2024", "amount": "100"}
          Error: Invalid date (February has no 31st)

Row 1892: {"date": "2024-13-01", "amount": "50"}
          Error: Invalid date (month 13 doesn't exist)

# Fix the source data and replay
$ casparian quarantine replay --job abc123
Re-processing 2 quarantined rows...
✓ Row 47: Now valid (date corrected in source)
✗ Row 1892: Still invalid
1 row recovered, 1 row still quarantined
```

### When to Quarantine vs. Fail

| Scenario | Action | Reason |
|----------|--------|--------|
| Parser throws exception | **Fail** | Parser is broken |
| Output has wrong columns | **Fail** | Schema contract violated |
| Row has null in required field | **Quarantine** | Data quality issue |
| Row has unparseable date | **Quarantine** | Data quality issue |
| All rows fail validation | **Fail** | Something is very wrong |
| >50% rows fail validation | **Fail** (configurable) | Likely schema mismatch |

### Configuration

```toml
[validation]
quarantine_threshold = 0.5    # Fail if >50% of rows are invalid
max_quarantine_rows = 10000   # Fail if >10k rows would be quarantined
```

---

## Updated Schema

### cf_parsers (Unified Parser Registry)

```sql
CREATE TABLE cf_parsers (
    parser_id         TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    version           TEXT NOT NULL,

    -- Source (single artifact containing everything)
    source_archive    BLOB NOT NULL,         -- ZIP: *.py + uv.lock + data files
    source_hash       TEXT NOT NULL,         -- sha256(ZIP) for integrity
    entrypoint        TEXT NOT NULL DEFAULT 'parser.py',

    -- Environment (extracted from ZIP at registration for venv caching)
    lockfile_hash     TEXT NOT NULL,         -- sha256(uv.lock) for venv cache lookup
    python_version    TEXT DEFAULT '3.11',

    -- Metadata
    topics            TEXT DEFAULT '[]',     -- JSON array
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,

    UNIQUE(name, version)
);
```

### cf_jobs (Unified Job Queue)

```sql
CREATE TABLE cf_jobs (
    job_id            TEXT PRIMARY KEY,
    job_type          TEXT NOT NULL,         -- 'parse', 'scan', 'backtest'
    status            TEXT NOT NULL CHECK(status IN (
        'pending',
        'running',
        'completed',
        'completed_with_warnings',           -- Some rows quarantined
        'failed'
    )),

    -- Payload (job-type specific, JSON)
    payload           TEXT NOT NULL,

    -- Execution tracking
    worker_id         TEXT,                  -- UUID for logging/tracing
    claimed_at        INTEGER,
    last_heartbeat_at INTEGER,               -- Updated every 60s during execution

    -- Results
    completed_at      INTEGER,
    result            TEXT,                  -- JSON
    error_message     TEXT,
    rows_processed    INTEGER,               -- Total rows in output
    rows_quarantined  INTEGER DEFAULT 0,     -- Rows sent to quarantine

    -- Lineage
    parent_job_id     TEXT,
    created_at        INTEGER NOT NULL,

    FOREIGN KEY (parent_job_id) REFERENCES cf_jobs(job_id)
);

-- Index for atomic claiming
CREATE INDEX idx_jobs_claimable ON cf_jobs(status, created_at)
    WHERE status = 'pending';
```

Note: `last_heartbeat_at` is updated every 60 seconds during job execution. Jobs with stale heartbeats (>5 minutes) are requeued on startup and periodically. This is lightweight (240 writes/hour with 4 workers) and avoids PID reuse bugs.

### cf_files (Discovered Files)

```sql
CREATE TABLE cf_files (
    file_id           TEXT PRIMARY KEY,
    path              TEXT NOT NULL UNIQUE,
    hash              TEXT NOT NULL,     -- blake3 hash of content
    size_bytes        INTEGER NOT NULL,
    topic             TEXT,              -- Assigned topic (nullable)
    discovered_at     INTEGER NOT NULL,
    modified_at       INTEGER NOT NULL,

    -- Source tracking
    source_id         TEXT,              -- Which scan discovered this

    FOREIGN KEY (source_id) REFERENCES cf_jobs(job_id)
);

CREATE INDEX idx_files_topic ON cf_files(topic) WHERE topic IS NOT NULL;
CREATE INDEX idx_files_hash ON cf_files(hash);
```

### Database Initialization

```sql
-- Enable WAL mode for concurrent access
PRAGMA journal_mode = WAL;

-- Busy timeout (wait instead of immediate failure)
PRAGMA busy_timeout = 5000;
```

---

## Execution Plan

### Phase 0: Modal Runner Architecture

**Goal:** Implement the DevRunner/QueuedRunner abstraction with modal environment resolution.

**Tasks:**

- [ ] **0.1** Define `Runner` trait
  ```rust
  trait Runner {
      fn execute(&self, parser: &ParserRef, input: &Path) -> Result<Output>;
  }
  ```

- [ ] **0.2** Implement `DevRunner`
  - Uses parser file from disk (not bundled)
  - Resolves Python: `$VIRTUAL_ENV` → `.venv/` → system
  - No database interaction during execution
  - Optional: record job after completion (for history)

- [ ] **0.3** Implement `QueuedRunner`
  - Uses bundled parser from database (ZIP archive)
  - Resolves Python: `--python` flag → managed venv
  - Claims job before execution, updates after

- [ ] **0.4** Add `--python` flag to CLI
  - `casparian worker --python /path/to/python`
  - Bypasses venv management entirely

### Phase 1: Storage Abstraction (Database Agnostic)

**Goal:** Trait-based storage to support multiple databases.

**Tasks:**

- [ ] **1.1** Define `JobStore` trait
  - `insert(&Job) -> Result<()>`
  - `claim_next(worker_id: &str) -> Result<Option<Job>>`
  - `complete(job_id: &str, result: &JobResult) -> Result<()>`
  - `fail(job_id: &str, error: &str) -> Result<()>`
  - `requeue(job_id: &str) -> Result<()>`
  - `heartbeat(job_id: &str) -> Result<()>`
  - `requeue_stale(threshold: Duration) -> Result<Vec<String>>`
  - `get(job_id: &str) -> Result<Option<Job>>`
  - `list(filter: &JobFilter) -> Result<Vec<Job>>`
  - `exists_for_hash(input_hash: &str, parser_id: &str) -> Result<bool>`

- [ ] **1.2** Define `ParserStore` trait
  - `insert(&Parser) -> Result<()>`
  - `get(parser_id: &str) -> Result<Option<Parser>>`
  - `get_by_name_version(name: &str, version: &str) -> Result<Option<Parser>>`
  - `find_by_topic(topic: &str) -> Result<Vec<Parser>>`
  - `list(filter: &ParserFilter) -> Result<Vec<ParserSummary>>`

- [ ] **1.3** Define `QuarantineStore` trait
  - `insert(&QuarantineEntry) -> Result<()>`
  - `list_for_job(job_id: &str) -> Result<Vec<QuarantineEntry>>`
  - `delete_for_job(job_id: &str) -> Result<()>`

- [ ] **1.4** Implement SQLite stores
  - `SqliteJobStore` with atomic `UPDATE...RETURNING`
  - `SqliteParserStore` with ZIP archive BLOB storage
  - `SqliteQuarantineStore`
  - WAL mode and busy_timeout in constructors

- [ ] **1.5** Wire up dependency injection
  - `Arc<dyn JobStore>` / `Arc<dyn ParserStore>` passed to runners
  - Factory function to create stores based on config

### Phase 2: Unified Schema & Parser Bundling

**Goal:** Single database schema with self-contained parser artifacts.

**Tasks:**

- [ ] **2.1** Create unified tables
  - `cf_jobs` with `status` including `completed_with_warnings`
  - `cf_parsers` with `source_archive` (BLOB), `source_hash`, `lockfile_hash`
  - `cf_quarantine` for row-level validation failures
  - `cf_files` (discovered files with tags)

- [ ] **2.2** Implement parser bundling
  - `bundle_parser(dir) -> ParserBundle` (creates ZIP archive)
  - **REQUIRE uv.lock** - Error if missing: "Run 'uv lock' first"
  - Bundle: *.py, *.json, *.yaml, *.csv, *.txt, *.xml, **uv.lock**
  - Exclude: .venv/, __pycache__/, *.so, *.dll, *.pyd
  - Compute `lockfile_hash` from uv.lock (for venv caching)
  - Compute `source_hash` from ZIP (for integrity)
  - **CRITICAL: Canonical ZIPs** - Zero out timestamps (1980-01-01)

- [ ] **2.3** Implement local validation (Layer 1)
  - Build venv from lockfile locally
  - Extract ZIP to temp, try to import entrypoint
  - Report clear errors on failure

- [ ] **2.4** Create migration script
  - Migrate `cf_plugin_manifest` → `cf_parsers`
  - Migrate `cf_processing_queue` → `cf_jobs`
  - Convert single-file source_code → ZIP archive
  - Preserve existing data

### Phase 3: Protocol Convergence & Safe Serialization

**Goal:** Single shim, single protocol, robust serialization.

**Tasks:**

- [ ] **3.1** Add `safe_to_arrow()` to `bridge_shim.py`
  - Catches `ArrowInvalid` / `ArrowTypeError`
  - Falls back to string for mixed-type columns
  - Logs which columns were converted
  - Ensures data always reaches Rust for quarantine
  - See [Critical Code Fixes: Issue 4](#issue-4-serialization-robustness-high-severity)

- [ ] **3.2** Update `bridge_shim.py` for Progressive API
  - Detect function vs class parser
  - Warn if Level 1 (function) in Prod mode
  - Support `--parser-path /path/to/parser.py` (Dev mode)
  - Support `--parser-archive base64...` (Prod mode, ZIP)

- [ ] **3.3** Update `cli/run.rs` to use bridge protocol
  - Remove ZMQ code entirely
  - Use `bridge::execute_bridge()` function
  - No database interaction (stateless)
  - See [Critical Code Fixes: Issue 1](#issue-1-protocol-divergence-high-severity)

- [ ] **3.4** Remove ProcessingHistory from `cli/run.rs`
  - Delete lines 262-567 (ProcessingHistory struct and SQL)
  - Dev mode writes output directly, prints summary, exits
  - See [Critical Code Fixes: Issue 2](#issue-2-duplicate-database-medium-severity)

- [ ] **3.5** Delete `run_shim.py`
  - `crates/casparian_worker/shim/run_shim.py` - DELETE
  - `bridge_shim.py` becomes the single shim

- [ ] **3.6** Remove adhoc dependency path from `main.rs`
  - Delete lines 1253-1309 (adhoc environment creation)
  - Delete `parse_plugin_dependencies()` function (lines 1419-1452)
  - Delete `is_stdlib_module()` function (lines 1454-1465)
  - Replace with clear error if lockfile missing
  - See [Critical Code Fixes: Issue 3](#issue-3-ad-hoc-dependency-bug-high-severity)

### Phase 4: Worker Loop (Heartbeat-Based)

**Goal:** Reliable job processing with lightweight heartbeat-based zombie detection.

**Tasks:**

- [ ] **4.1** Implement heartbeat mechanism
  - Worker sends heartbeat every 60 seconds during job execution
  - `JobStore.heartbeat(job_id)` updates `last_heartbeat_at`
  - Simple, no platform-specific code

- [ ] **4.2** Implement stale job cleanup
  - On startup: `job_store.requeue_stale(Duration::from_secs(300))`
  - Periodic: Run cleanup every 5 minutes
  - Requeues jobs with heartbeat older than 5 minutes
  - Log requeued job IDs for visibility

- [ ] **4.3** Implement `QueuedRunner` worker loop
  - Claim job via `JobStore.claim_next(worker_id)`
  - Spawn subprocess, track `Child` handle
  - Send heartbeat every 60s while child is running
  - Update status on completion/failure

- [ ] **4.4** Add concurrent worker support
  - `MAX_CONCURRENT_WORKERS` config
  - Track multiple `WorkerProcess` handles
  - Fair scheduling across workers

### Phase 5: Quarantine & Row Validation

**Goal:** Partial success with row-level validation.

**Tasks:**

- [ ] **5.1** Implement row validation in executor
  - Validate each row against schema contract
  - Separate valid rows from quarantined rows
  - Track quarantine entries

- [ ] **5.2** Add quarantine persistence
  - Write quarantine entries via `QuarantineStore`
  - Update job with `rows_processed` and `rows_quarantined`
  - Set status to `completed_with_warnings` when quarantine > 0

- [ ] **5.3** Add quarantine threshold config
  - Fail job if >50% rows invalid (configurable)
  - Fail job if >10k rows quarantined (configurable)

- [ ] **5.4** Implement quarantine CLI
  - `casparian quarantine list --job <id>`
  - `casparian quarantine replay --job <id>`

### Phase 6: Folder Watching (Daemon Mode)

**Goal:** Background process that watches folders and processes queue.

**Tasks:**

- [ ] **6.1** Implement periodic scan
  - Scan watch paths every 10 seconds
  - Create jobs for new files matching patterns
  - Match topics to parser subscriptions

- [ ] **6.2** Implement daemon command
  - `casparian start --watch /path`
  - Spawn watch task + worker tasks
  - Graceful shutdown on SIGTERM

- [ ] **6.3** Implement batch process command
  - `casparian process [--limit N]`
  - Drain queue and exit

### Phase 7: CLI Polish & Cleanup

**Goal:** User-friendly commands, remove deprecated code.

**Tasks:**

- [ ] **7.1** Parser commands
  - `casparian parser list`
  - `casparian parser show <name>`
  - `casparian parser register <path>` - Bundle, validate, store
  - `casparian parser register <path> --validate-remote` - Also validate on worker
  - `casparian parser register <path> --skip-validation` - For CI/CD
  - `casparian parser register <path> --force` - Overwrite existing
  - `casparian parser test <path> [sample.csv]` - Test in isolated venv locally
  - `casparian parser topics <name> add/remove <topic>`
  - `casparian scaffold <script.py>` - Generate Parser class from Level 1 function

- [ ] **7.2** Job commands
  - `casparian jobs [--status X] [--parser Y]`
  - `casparian job show <id>`
  - `casparian job retry <id>`
  - `casparian job cancel <id>`
  - `casparian jobs prune --older-than 30d`

- [ ] **7.3** Environment commands
  - `casparian vendor <parser_dir> --output ./vendor` (air-gapped prep)
  - `casparian env list` (show cached venvs)
  - `casparian env prune` (clean old venvs)

- [ ] **7.4** Validation job type
  - Add `JobType::Validate` for remote validation
  - Worker handles validation jobs: build venv, import test, report status
  - CLI waits for validation result before committing registration

- [ ] **7.5** Delete deprecated code
  - `run_shim.py`, `bridge_shim.py`
  - `cf_plugin_manifest`, `cf_plugin_environment` tables
  - `cf_processing_queue`, `cf_job_status`, `cf_processing_history` tables
  - Remove `lockfile_content` column (lockfile now in ZIP)

- [ ] **7.6** Update documentation
  - Update `CLAUDE.md` files
  - Update `README.md`
  - Update CLI help text

---

## User Workflows

### Workflow 1: Dev Mode (Fast Iteration)

```bash
$ casparian run my_parser.py sales.csv

# What happens (DevRunner):
# 1. Uses parser.py directly from disk (no bundling)
# 2. Uses current Python ($VIRTUAL_ENV or system)
# 3. Executes immediately, stdout/stderr to terminal
# 4. Output written to sink
# 5. Optional: job recorded in cf_jobs (for history)
# 6. Process exits

# Debugging works!
$ casparian run my_parser.py sales.csv
> Breakpoint hit at parser.py:47
> (pdb) print(row)
```

### Workflow 2: Register Parser for Prod

```bash
# Explicitly register parser with lockfile for production use
$ casparian parser register my_parser.py
Parser 'my_parser' v1.0.0 registered
  - Source: bundled (ZIP archive)
  - Environment: lockfile captured from ./uv.lock
  - Topics: []

# Add topic subscription
$ casparian parser topics my_parser add invoices
Parser 'my_parser' now subscribed to topic 'invoices'
```

### Workflow 3: Batch Processing (Prod Mode)

```bash
# Scan and tag files
$ casparian scan /data/csv --tag invoices
Discovered 1000 files, tagged with 'invoices'
Created 1000 jobs (pending)

# Process queue (QueuedRunner)
$ casparian process
Processing 1000 jobs...
[========================================] 100% (1000/1000)
✓ 998 completed
⚠ 2 completed with warnings (rows quarantined)
Done.
```

### Workflow 4: Continuous Processing (Daemon)

```bash
# Start daemon with folder watching
$ casparian start --watch /data/incoming
Watching /data/incoming for new files...
Parser 'invoice_parser' subscribed to topic 'invoices'

# New files are auto-processed
[10:30:15] New file: invoice_001.csv → job created
[10:30:16] Job a1b2c3d4 completed (1000 rows, 2 quarantined)
```

### Workflow 5: Handle Quarantined Rows

```bash
# View quarantined rows
$ casparian quarantine list --job a1b2c3d4
Job a1b2c3d4: 2 rows quarantined

Row 47:   {"date": "31/02/2024", "amount": "100"}
          Error: Invalid date (February has no 31st)

# Fix source data, replay
$ casparian quarantine replay --job a1b2c3d4
✓ 1 row recovered, 1 still quarantined
```

### Workflow 6: Air-Gapped Deployment

```bash
# On connected machine: vendor dependencies
$ casparian vendor my_parser.py --output ./vendor
Dependencies vendored to ./vendor/

# Copy to air-gapped machine, then:
$ casparian worker --python /opt/venv/bin/python --offline
# Uses explicit Python, no network access
```

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Migration corrupts data | Low | High | Backup before migrate, test on copy |
| Shim merge introduces bugs | Medium | Medium | Extensive testing, gradual rollout |
| uv not installed on worker | Medium | High | Check on startup, clear error message. Use `--python` to bypass. |
| Lockfile generation fails | Low | Medium | Fall back to minimal lockfile |
| Venv rebuild fails | Low | Medium | Retry once, then fail with clear error |
| SQLite concurrent access issues | Medium | Medium | WAL mode + busy timeout + atomic claims |
| Worker crash mid-job | Medium | Low | OS-level process tracking, requeue via `waitpid()` |
| Dev mode uses wrong Python | Low | Medium | Clear logging of which Python is used |
| Performance regression | Low | Low | Benchmark before/after |

---

## Success Criteria

1. **Dev mode works:** `casparian run parser.py input.csv` uses current Python, runs from disk, `pdb` works

2. **Prod mode works:** `casparian process` uses bundled parsers and managed venvs

3. **Dedup works:** Running same file twice shows "Already processed"

4. **Environment works:** Worker rebuilds venv from lockfile, or uses `--python` flag

5. **Concurrency works:** Two workers don't claim same job (atomic claim)

6. **Daemon works:** `casparian start --watch /data` processes new files automatically

7. **History unified:** `casparian jobs` shows ALL jobs from ALL execution modes

8. **Quarantine works:** Bad rows go to `cf_quarantine`, good rows to output, job status is `completed_with_warnings`

9. **Debugging works:** Dev mode pipes stdout/stderr to terminal, supports breakpoints

10. **No parallel systems:** Only `cf_jobs`, `cf_parsers`, `cf_quarantine`, `cf_files` tables exist

---

## What Gets Deleted

| Current | Action |
|---------|--------|
| `run_shim.py` | **DELETE** - use `bridge_shim.py` instead |
| `bridge_shim.py` | **KEEP** - update with safe_to_arrow(), becomes single shim |
| `ProcessingHistory` in run.rs | **DELETE** - dev mode is stateless |
| Adhoc dependency path in main.rs | **DELETE** - require uv.lock |
| `parse_plugin_dependencies()` | **DELETE** - no longer needed |
| `cf_plugin_manifest` | **DELETE** - use `cf_parsers` |
| `cf_plugin_environment` | **DELETE** - use `cf_parsers.lockfile_*` |
| `cf_processing_queue` | **DELETE** - use `cf_jobs` |
| `cf_job_status` | **DELETE** - use `cf_jobs` |
| `cf_processing_history` | **DELETE** - derive from `cf_jobs` |
| `cf_parsers` (run.rs version) | **DELETE** - use unified `cf_parsers` |
| `casparian publish` | **DELETE** - use `casparian parser register` |

**Net reduction:**
- 6 old tables → 4 new tables
- 2 shims → 1 shim (`bridge_shim.py`)
- 2 protocols → 1 protocol (Unix socket binary)
- 2 database code paths → 1 (Sentinel only)

---

## Appendix: Atomic Job Claim Query

```sql
-- Atomic claim (SQLite version)
-- Note: PostgreSQL would use "FOR UPDATE SKIP LOCKED" in the subquery
UPDATE cf_jobs
SET
    status = 'running',
    worker_id = :worker_uuid,
    claimed_at = :now,
    last_heartbeat_at = :now
WHERE job_id = (
    SELECT job_id
    FROM cf_jobs
    WHERE status = 'pending'
    ORDER BY created_at
    LIMIT 1
)
RETURNING *;

-- Heartbeat update (called every 60 seconds during job execution)
UPDATE cf_jobs
SET last_heartbeat_at = :now
WHERE job_id = :job_id AND status = 'running';

-- Requeue stale jobs (called on startup + every 5 minutes)
-- Returns requeued job IDs for logging
UPDATE cf_jobs
SET status = 'pending', worker_id = NULL, claimed_at = NULL, last_heartbeat_at = NULL
WHERE status = 'running' AND last_heartbeat_at < :threshold
RETURNING job_id;

-- Requeue a specific job (manual intervention)
UPDATE cf_jobs
SET status = 'pending', worker_id = NULL, claimed_at = NULL, last_heartbeat_at = NULL
WHERE job_id = :job_id AND status = 'running';
```

Note: Lightweight heartbeats (60s interval, 5min threshold) provide zombie detection without PID reuse bugs. With 4 workers, this is ~240 writes/hour - negligible for SQLite.

---

## Appendix: Unified Shim Protocol (Unix Socket)

> **Note:** This replaces the previous ZMQ-based protocol. Unix sockets with binary framing
> are simpler, more efficient, and already implemented in `bridge.rs`.

### Protocol Specification

```
Transport: Unix Domain Socket (AF_UNIX, SOCK_STREAM)
Framing:   [LENGTH:4 bytes, big-endian][PAYLOAD]

Message Types:
  LENGTH > 0 && LENGTH < 0xFFFFFFFE  → Arrow IPC batch (PAYLOAD = raw bytes)
  LENGTH = 0                         → End of stream (no payload)
  LENGTH = 0xFFFFFFFF                → Error signal (followed by error message)
  LENGTH = 0xFFFFFFFE                → Log message (sideband logging)

Error Message Format:
  [0xFFFFFFFF:4][ERROR_LENGTH:4][ERROR_UTF8_BYTES]

Log Message Format:
  [0xFFFFFFFE:4][LEVEL:1][MSG_LENGTH:4][MSG_UTF8_BYTES]
  LEVEL: 0=stdout, 1=stderr, 2=debug, 3=info, 4=warning, 5=error
```

### Updated bridge_shim.py (Key Changes)

```python
#!/usr/bin/env python3
"""
Unified worker shim for Casparian.
Supports BOTH Dev mode (--parser-path) and Prod mode (--parser-archive).
Supports Progressive API: Level 1 (function) and Level 2 (class).

Protocol: Unix socket with binary framing [LENGTH:4][ARROW_IPC]
"""

import argparse
import base64
import io
import os
import sys
import socket
import struct
import tempfile
import zipfile
import importlib.util
import logging
import pyarrow as pa
import pandas as pd

# Protocol constants
HEADER_FORMAT = "!I"  # 4-byte unsigned int (big-endian)
END_OF_STREAM = 0
ERROR_SIGNAL = 0xFFFFFFFF
LOG_SIGNAL = 0xFFFFFFFE

logger = logging.getLogger(__name__)


def safe_to_arrow(df: pd.DataFrame) -> pa.Table:
    """
    Convert DataFrame to Arrow with fallback for mixed-type columns.

    This is CRITICAL for quarantine to work. If Arrow conversion fails
    here, the data never reaches Rust and quarantine is useless.

    Strategy:
    1. Try direct conversion
    2. On ArrowInvalid, identify problematic columns (object dtype)
    3. Convert those columns to string
    4. Retry conversion

    Why strings? They always serialize cleanly, and Rust-side validation
    will catch the type mismatch and quarantine appropriately.
    """
    try:
        return pa.Table.from_pandas(df)
    except (pa.ArrowInvalid, pa.ArrowTypeError) as e:
        # Identify and fix problematic columns
        for col in df.columns:
            if df[col].dtype == 'object':
                try:
                    pa.array(df[col])
                except:
                    logger.warning(f"Column '{col}' has mixed types, converting to string")
                    df[col] = df[col].astype(str)

        # Retry with sanitized DataFrame
        return pa.Table.from_pandas(df)


def send_batch(sock: socket.socket, table: pa.Table):
    """Serialize Arrow table and send via binary protocol."""
    sink = io.BytesIO()
    with pa.ipc.new_stream(sink, table.schema) as writer:
        for batch in table.to_batches():
            writer.write_batch(batch)

    ipc_bytes = sink.getvalue()
    sock.sendall(struct.pack(HEADER_FORMAT, len(ipc_bytes)))
    sock.sendall(ipc_bytes)


def send_end_of_stream(sock: socket.socket):
    """Signal completion."""
    sock.sendall(struct.pack(HEADER_FORMAT, END_OF_STREAM))


def send_error(sock: socket.socket, message: str):
    """Signal error with message."""
    error_bytes = message.encode('utf-8')
    sock.sendall(struct.pack(HEADER_FORMAT, ERROR_SIGNAL))
    sock.sendall(struct.pack(HEADER_FORMAT, len(error_bytes)))
    sock.sendall(error_bytes)


def load_parser_from_path(parser_path: str):
    """Dev mode: load parser directly from disk."""
    parser_dir = os.path.dirname(os.path.abspath(parser_path))
    sys.path.insert(0, parser_dir)

    module_name = os.path.basename(parser_path).replace('.py', '')
    spec = importlib.util.spec_from_file_location(module_name, parser_path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Could not load parser: {parser_path}")

    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_parser_from_archive(archive_b64: str, entrypoint: str, temp_dir: str):
    """Prod mode: unzip archive and load entrypoint."""
    archive_bytes = base64.b64decode(archive_b64)

    with zipfile.ZipFile(io.BytesIO(archive_bytes)) as zf:
        zf.extractall(temp_dir)

    sys.path.insert(0, temp_dir)

    module_name = entrypoint.replace('.py', '')
    spec = importlib.util.spec_from_file_location(
        module_name,
        os.path.join(temp_dir, entrypoint)
    )
    if spec is None or spec.loader is None:
        raise ImportError(f"Could not load entrypoint: {entrypoint}")

    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def execute_parser(module, input_path: str, is_prod_mode: bool) -> pd.DataFrame:
    """Execute parser supporting Progressive API."""

    # Level 2: Class-based parser (production-ready)
    if hasattr(module, 'Parser'):
        parser_class = module.Parser
        ctx = type('Context', (), {'input_path': input_path})()
        return parser_class().parse(ctx)

    # Level 1: Function-based parser (dev-only)
    if hasattr(module, 'parse'):
        if is_prod_mode:
            logger.warning("Using Level 1 (function) parser in production mode.")
            logger.warning("Consider upgrading: casparian scaffold")
        return module.parse(input_path)

    # Legacy: Handler class pattern
    if hasattr(module, 'Handler'):
        return module.Handler().parse(input_path)

    raise ImportError("Parser must have Parser class, parse() function, or Handler class")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--socket-path', required=True, help='Unix socket path')
    parser.add_argument('--input-path', required=True)
    parser.add_argument('--job-id', required=True)

    # Dev mode: parser file on disk
    parser.add_argument('--parser-path', help='Path to parser.py (Dev mode)')

    # Prod mode: bundled archive
    parser.add_argument('--parser-archive', help='Base64 ZIP archive (Prod mode)')
    parser.add_argument('--entrypoint', default='parser.py')

    args = parser.parse_args()

    # Validate: exactly one of --parser-path or --parser-archive
    if bool(args.parser_path) == bool(args.parser_archive):
        print("Error: Provide exactly one of --parser-path or --parser-archive", file=sys.stderr)
        sys.exit(1)

    is_prod_mode = bool(args.parser_archive)

    # Connect to Rust process via Unix socket
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect(args.socket_path)

    try:
        if args.parser_path:
            # Dev mode: load from disk
            module = load_parser_from_path(args.parser_path)
        else:
            # Prod mode: unzip and load
            with tempfile.TemporaryDirectory() as temp_dir:
                module = load_parser_from_archive(args.parser_archive, args.entrypoint, temp_dir)

        # Execute parser (supports Progressive API)
        result = execute_parser(module, args.input_path, is_prod_mode)

        # Convert to Arrow with SAFE serialization
        table = safe_to_arrow(result)

        # Send Arrow data via binary protocol
        send_batch(sock, table)
        send_end_of_stream(sock)

    except Exception as e:
        send_error(sock, str(e))
        sys.exit(1)

    finally:
        sock.close()


if __name__ == '__main__':
    main()
```

### Key Differences from ZMQ Version

| Aspect | Old (ZMQ) | New (Unix Socket) |
|--------|-----------|-------------------|
| Transport | ZMQ PUSH/PULL | AF_UNIX SOCK_STREAM |
| Framing | JSON messages | Binary `[LENGTH:4][DATA]` |
| Arrow encoding | Base64 in JSON | Raw bytes |
| Overhead | ~33% (Base64) | ~0% |
| Dependencies | pyzmq | stdlib only |
| Error handling | JSON `{"type": "error"}` | Binary error signal |
| Completion | JSON `{"type": "done"}` | `LENGTH=0` |
