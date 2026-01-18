# DuckDB Migration Plan

**Status Update (2026-01-18):** async-duckdb is no longer used. Casparian now
owns the async boundary with a dedicated DuckDB actor thread and a synchronous
`duckdb::Connection` on that thread. References to async-duckdb below are
historical and should be treated as deprecated guidance.

**Status:** Draft
**Author:** AI Assistant
**Date:** January 16, 2026
**Version:** 1.0

---

## Executive Summary

This document outlines the migration from SQLite (via sqlx) to DuckDB (via async-duckdb) for Casparian Flow. The migration is motivated by DuckDB's columnar storage, native glob/Parquet support, and 20-50x analytical query performance improvements.

**Key Constraints:**
- DuckDB has a **single-writer model** at the process level (multiple readers OK)
- async-duckdb pools are **read-only** by design
- No sqlx support for DuckDB (different API patterns)
- Must maintain all existing functionality and pass all tests

**Strategy:** Hybrid architecture - DuckDB for analytical workloads, retain SQLite option for edge cases requiring multi-process writes.

---

## Table of Contents

1. [Current Architecture Analysis](#1-current-architecture-analysis)
2. [Target Architecture](#2-target-architecture)
3. [Critical Concerns & Edge Cases](#3-critical-concerns--edge-cases)
4. [Migration Phases](#4-migration-phases)
5. [API Translation Guide](#5-api-translation-guide)
6. [Concurrency Model Changes](#6-concurrency-model-changes)
7. [Test Strategy](#7-test-strategy)
8. [Rollback Plan](#8-rollback-plan)
9. [Performance Validation](#9-performance-validation)
10. [Implementation Checklist](#10-implementation-checklist)

---

## 1. Current Architecture Analysis

### 1.1 Database Usage Summary

| Component | Tables | Query Patterns | Transaction Usage |
|-----------|--------|----------------|-------------------|
| **Scout** | 23 tables, 23 indexes | Bulk INSERT ON CONFLICT, SELECT with filters | Yes (batch_upsert_files) |
| **Schema** | 2 tables | CRUD, ORDER BY DESC LIMIT 1 | No |
| **Sentinel** | 4 tables | Atomic UPDATE WHERE for job claiming | Yes (job claiming, dead letter) |
| **Backtest** | 1 table | SELECT aggregates | No |

### 1.2 Current Database Layer

```
casparian_db/
├── lib.rs          # DatabaseType enum, exports
├── license.rs      # License validation
└── pool.rs         # DbConfig, create_pool(), DbPool type alias
```

**Type Aliases (compile-time):**
```rust
#[cfg(feature = "sqlite")]
pub type DbPool = sqlx::SqlitePool;

#[cfg(feature = "postgres")]
pub type DbPool = sqlx::PgPool;
```

### 1.3 Query Patterns Inventory

**Pattern 1: Simple SELECT**
```rust
// Current (sqlx)
sqlx::query_as::<_, ContractRow>("SELECT * FROM schema_contracts WHERE id = ?")
    .bind(id)
    .fetch_optional(&pool)
    .await?
```

**Pattern 2: INSERT ON CONFLICT (Upsert)**
```rust
// Current (sqlx) - 50+ occurrences
sqlx::query("INSERT INTO table (...) VALUES (...) ON CONFLICT DO UPDATE SET ...")
    .bind(...)
    .execute(&pool)
    .await?
```

**Pattern 3: Transactions**
```rust
// Current (sqlx) - 5 locations
let mut tx = pool.begin().await?;
sqlx::query(...).execute(&mut *tx).await?;
tx.commit().await?;
```

**Pattern 4: Bulk INSERT with Dynamic SQL**
```rust
// Current (sqlx) - Scout batch_upsert_files
let values = (0..files.len()).map(|_| "(?, ?, ...)").collect::<Vec<_>>().join(", ");
let sql = format!("INSERT INTO scout_files (...) VALUES {}", values);
let mut query = sqlx::query(&sql);
for file in files { query = query.bind(...); }
query.execute(&mut *tx).await?;
```

---

## 2. Target Architecture

### 2.1 Dual-Mode Design

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           casparian_db (v2)                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        DbBackend (enum)                              │   │
│  │  ┌─────────────────────┐      ┌─────────────────────────────────┐   │   │
│  │  │  Sqlite(SqlitePool) │      │  DuckDb(async_duckdb::Client)   │   │   │
│  │  │  - Multi-process OK │      │  - Single-writer process        │   │   │
│  │  │  - Row-oriented     │      │  - Columnar OLAP                │   │   │
│  │  │  - Legacy fallback  │      │  - Native glob/Parquet          │   │   │
│  │  └─────────────────────┘      └─────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                      Unified Query Trait                             │   │
│  │  async fn execute(&self, sql: &str, params: &[Value]) -> Result<()> │   │
│  │  async fn query_one<T>(&self, sql: &str, params: &[Value]) -> T     │   │
│  │  async fn query_all<T>(&self, sql: &str, params: &[Value]) -> Vec<T>│   │
│  │  async fn transaction<F, T>(&self, f: F) -> Result<T>               │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Feature Flags

```toml
# Cargo.toml
[features]
default = ["duckdb"]                    # DuckDB is now default
sqlite = ["sqlx/sqlite"]                # Legacy, multi-process scenarios
duckdb = ["async-duckdb"]               # Primary analytical backend
postgres = ["sqlx/postgres"]            # Enterprise (unchanged)
```

### 2.3 New Type System

```rust
/// Database backend selection
pub enum DbBackend {
    #[cfg(feature = "sqlite")]
    Sqlite(sqlx::SqlitePool),

    #[cfg(feature = "duckdb")]
    DuckDb(async_duckdb::Client),

    #[cfg(feature = "postgres")]
    Postgres(sqlx::PgPool),
}

/// Unified connection wrapper
pub struct DbConnection {
    backend: DbBackend,
}

/// Transaction wrapper
pub struct DbTransaction<'a> {
    backend: TransactionBackend<'a>,
}

enum TransactionBackend<'a> {
    #[cfg(feature = "sqlite")]
    Sqlite(sqlx::Transaction<'a, sqlx::Sqlite>),

    #[cfg(feature = "duckdb")]
    DuckDb(DuckDbTransaction),  // Custom wrapper

    #[cfg(feature = "postgres")]
    Postgres(sqlx::Transaction<'a, sqlx::Postgres>),
}
```

---

## 3. Critical Concerns & Edge Cases

### 3.1 DuckDB Concurrency Model

**CRITICAL: Single-Writer Constraint**

DuckDB allows only ONE writer process to a database file at a time.

| Scenario | SQLite Behavior | DuckDB Behavior | Impact |
|----------|-----------------|-----------------|--------|
| TUI + background scan | Works (WAL mode) | Works (single process) | OK |
| CLI + TUI on same DB | Works | **FAILS** - lock error | HIGH |
| Multiple `casparian run` | Works | **FAILS** | HIGH |
| Read during write | Works | Works (MVCC within process) | OK |

**Mitigations:**

1. **Process Lock File**: Check for existing writer before opening
   ```rust
   fn try_open_exclusive(path: &Path) -> Result<LockGuard> {
       let lock_path = path.with_extension("duckdb.lock");
       let lock = FileLock::try_lock(&lock_path)?;
       // If lock acquired, we're the writer
       Ok(lock)
   }
   ```

2. **Read-Only Fallback**: If lock fails, open read-only
   ```rust
   fn open_db(path: &Path) -> Result<DbConnection> {
       match try_open_exclusive(path) {
           Ok(lock) => Ok(DbConnection::writer(path, lock)),
           Err(_) => Ok(DbConnection::reader(path)),
       }
   }
   ```

3. **Clear Error Messages**:
   ```
   Error: Database is locked by another process

   Another casparian process is writing to ~/.casparian_flow/casparian_flow.duckdb

   Options:
     - Wait for the other process to finish
     - Use --read-only flag for queries
     - Kill the other process: ps aux | grep casparian
   ```

### 3.2 Transaction Semantics Differences

**SQLite (sqlx):**
```rust
let mut tx = pool.begin().await?;  // Returns Transaction
// tx is mutable, passed to queries via &mut *tx
tx.commit().await?;  // Explicit commit
// Drop without commit = rollback
```

**DuckDB (duckdb-rs, wrapped by async-duckdb):**
```rust
conn.execute_batch("BEGIN TRANSACTION")?;
// All subsequent queries are in transaction
conn.execute_batch("COMMIT")?;
// Or ROLLBACK
```

**async-duckdb pattern:**
```rust
client.conn(|conn| {
    conn.execute_batch("BEGIN TRANSACTION")?;
    // ... operations ...
    conn.execute_batch("COMMIT")?;
    Ok(())
}).await?
```

**Translation Wrapper:**
```rust
impl DbConnection {
    pub async fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut DbTransactionContext) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        match &self.backend {
            #[cfg(feature = "duckdb")]
            DbBackend::DuckDb(client) => {
                client.conn(move |conn| {
                    conn.execute_batch("BEGIN TRANSACTION")?;
                    let mut ctx = DbTransactionContext::new(conn);
                    match f(&mut ctx) {
                        Ok(result) => {
                            conn.execute_batch("COMMIT")?;
                            Ok(result)
                        }
                        Err(e) => {
                            let _ = conn.execute_batch("ROLLBACK");
                            Err(e)
                        }
                    }
                }).await
            }
            // ... other backends
        }
    }
}
```

### 3.3 Parameter Binding Differences

**SQLite (sqlx):** Positional `?` or named `$name`
```rust
sqlx::query("SELECT * FROM t WHERE a = ? AND b = ?")
    .bind(a)
    .bind(b)
```

**DuckDB:** Positional `$1, $2, ...` or `?`
```rust
conn.execute("SELECT * FROM t WHERE a = $1 AND b = $2", params![a, b])
```

**Edge Case: SQLite's ON CONFLICT syntax**
```sql
-- SQLite
INSERT INTO t (id, val) VALUES (?, ?)
ON CONFLICT(id) DO UPDATE SET val = excluded.val

-- DuckDB (same syntax works!)
INSERT INTO t (id, val) VALUES ($1, $2)
ON CONFLICT(id) DO UPDATE SET val = excluded.val
```

DuckDB supports the same ON CONFLICT syntax.

### 3.4 Type Mapping Differences

| Rust Type | SQLite (sqlx) | DuckDB |
|-----------|---------------|--------|
| `i64` | INTEGER | BIGINT |
| `String` | TEXT | VARCHAR |
| `bool` | INTEGER (0/1) | BOOLEAN |
| `DateTime<Utc>` | TEXT (RFC3339) | TIMESTAMP |
| `Vec<u8>` | BLOB | BLOB |
| `serde_json::Value` | TEXT | VARCHAR (JSON extension) |

**Potential Issue:** DateTime storage
- Current: RFC3339 strings (`2024-01-15T10:30:00Z`)
- DuckDB: Native TIMESTAMP preferred, but VARCHAR works

**Decision:** Keep VARCHAR for backward compatibility; can optimize later.

### 3.5 SQLite-Specific PRAGMAs

Current code uses:
```sql
PRAGMA journal_mode=WAL
PRAGMA synchronous=NORMAL
```

DuckDB equivalents:
```sql
-- WAL mode is always on in DuckDB, no setting needed
-- Synchronous equivalent (for durability):
PRAGMA enable_fsync_on_commit=true  -- Default is true
```

### 3.6 AUTOINCREMENT Differences

**SQLite:**
```sql
CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT)
```

**DuckDB:**
```sql
CREATE TABLE t (id INTEGER PRIMARY KEY DEFAULT nextval('t_id_seq'))
-- Or simpler:
CREATE TABLE t (id INTEGER PRIMARY KEY)  -- Auto-generates on INSERT
```

**Files Affected:**
- `scout/db.rs`: `scout_files.id`, `scout_folders.id`
- `sentinel/db/queue.rs`: `cf_processing_queue.id`, `cf_dead_letter.id`

**Migration SQL:**
```sql
-- DuckDB equivalent (uses IDENTITY)
CREATE TABLE scout_files (
    id INTEGER PRIMARY KEY,  -- DuckDB auto-generates
    ...
)

-- Or explicit sequence
CREATE SEQUENCE scout_files_id_seq;
CREATE TABLE scout_files (
    id INTEGER PRIMARY KEY DEFAULT nextval('scout_files_id_seq'),
    ...
)
```

### 3.7 SQL Syntax Differences

| Feature | SQLite | DuckDB | Action |
|---------|--------|--------|--------|
| `datetime('now')` | Yes | No | Use `current_timestamp` |
| `PRAGMA table_info()` | Yes | Different | Use `DESCRIBE table` |
| `GLOB` operator | Yes | No | Use `SIMILAR TO` or `~~` |
| String concat `||` | Yes | Yes | OK |
| `LIMIT/OFFSET` | Yes | Yes | OK |
| `GROUP BY 1, 2` | Yes | Yes | OK |
| Window functions | Limited | Full | OK (better) |

**datetime('now') occurrences:**
```bash
grep -r "datetime('now')" crates/
# Found in: sentinel/db/queue.rs, schema/storage.rs
```

**Fix:**
```sql
-- Before
created_at TEXT NOT NULL DEFAULT (datetime('now'))

-- After (DuckDB)
created_at TIMESTAMP NOT NULL DEFAULT current_timestamp
```

### 3.8 async-duckdb Pool Limitation

**CRITICAL:** async-duckdb pools only work with `access_mode='read_only'`

```rust
// This WORKS (read-only pool)
let pool = PoolBuilder::new()
    .path("/path/to/db.duckdb")
    .access_mode(AccessMode::ReadOnly)  // Required for pool!
    .build()
    .await?;

// This FAILS (read-write pool)
let pool = PoolBuilder::new()
    .path("/path/to/db.duckdb")
    .build()  // Default is ReadWrite
    .await?;  // ERROR: cannot have multiple writers
```

**Architectural Implication:**
- Use `Client` (single connection) for write operations
- Use `Pool` only for concurrent read queries (if needed)

**Our Pattern:**
```rust
pub struct DbConnection {
    /// Single write-capable client
    writer: async_duckdb::Client,

    /// Optional read pool for parallel queries (analytical workloads)
    #[cfg(feature = "duckdb-read-pool")]
    read_pool: Option<async_duckdb::Pool>,
}
```

---

## 4. Migration Phases

### Phase 1: Foundation (Week 1)

**Goal:** Add DuckDB to `casparian_db` alongside SQLite without breaking existing code.

**Tasks:**

1. **Add Cargo dependencies**
   ```toml
   [dependencies]
   async-duckdb = { version = "0.6", optional = true }
   duckdb = { version = "1.0", optional = true, features = ["bundled"] }

   [features]
   default = ["sqlite"]  # Keep SQLite default initially
   duckdb = ["dep:async-duckdb", "dep:duckdb"]
   ```

2. **Create `DbBackend` enum**
   - New file: `crates/casparian_db/src/backend.rs`
   - Define `DbBackend` enum with feature-gated variants
   - Implement `Clone`, `Debug`

3. **Create unified query trait**
   - New file: `crates/casparian_db/src/query.rs`
   - Define `DbExecutor` trait
   - Implement for each backend

4. **Update `DbConfig`**
   ```rust
   pub fn duckdb(path: impl AsRef<str>) -> Self {
       Self {
           url: format!("duckdb:{}", path.as_ref()),
           db_type: DatabaseType::DuckDb,
           max_connections: 1,  // Single writer
           license: License::community(),
       }
   }
   ```

5. **Add DuckDB pool creation**
   ```rust
   #[cfg(feature = "duckdb")]
   async fn create_duckdb_client(config: &DbConfig) -> Result<async_duckdb::Client, DbError> {
       let path = config.url.strip_prefix("duckdb:").unwrap();
       async_duckdb::ClientBuilder::new()
           .path(path)
           .open()
           .await
           .map_err(|e| DbError::Database(e.to_string()))
   }
   ```

6. **Tests:**
   - `test_duckdb_open_memory`
   - `test_duckdb_open_file`
   - `test_duckdb_basic_query`

**Validation:**
```bash
cargo check --features sqlite  # Existing (must pass)
cargo check --features duckdb  # New feature
cargo test -p casparian_db --features sqlite
cargo test -p casparian_db --features duckdb
```

---

### Phase 2: Schema Layer Migration (Week 2)

**Goal:** Migrate `casparian_schema` to use unified DB interface.

**Rationale:** Schema is the simplest component (2 tables, no transactions).

**Tasks:**

1. **Update `SchemaStorage` struct**
   ```rust
   // Before
   pub struct SchemaStorage {
       pool: DbPool,
   }

   // After
   pub struct SchemaStorage {
       conn: DbConnection,
   }
   ```

2. **Convert queries using wrapper**
   ```rust
   // Before
   sqlx::query_as::<_, ContractRow>("SELECT ... WHERE id = ?")
       .bind(id)
       .fetch_optional(&self.pool)
       .await

   // After
   self.conn.query_optional::<ContractRow>(
       "SELECT ... WHERE id = $1",
       &[id.into()]
   ).await
   ```

3. **Update schema initialization SQL**
   - Replace `datetime('now')` with `current_timestamp`
   - Replace `INTEGER PRIMARY KEY AUTOINCREMENT` with `INTEGER PRIMARY KEY`
   - Test both SQLite and DuckDB paths

4. **Add integration tests**
   ```rust
   #[tokio::test]
   #[cfg(feature = "duckdb")]
   async fn test_schema_storage_duckdb() {
       let temp = tempfile::NamedTempFile::new().unwrap();
       let storage = SchemaStorage::open_duckdb(temp.path()).await.unwrap();
       // ... same tests as SQLite
   }
   ```

**Validation:**
```bash
cargo test -p casparian_schema --features sqlite
cargo test -p casparian_schema --features duckdb
```

---

### Phase 3: Scout Database Migration (Week 3-4)

**Goal:** Migrate Scout's 23 tables while preserving batch insert performance.

**This is the largest and most critical phase.**

**3.1 Schema Translation**

Create `scout_schema_duckdb.sql`:
```sql
-- DuckDB version of Scout schema

CREATE TABLE IF NOT EXISTS scout_sources (
    id VARCHAR PRIMARY KEY,
    name VARCHAR NOT NULL UNIQUE,
    source_type VARCHAR NOT NULL,
    path VARCHAR NOT NULL,
    poll_interval_secs INTEGER NOT NULL DEFAULT 30,
    enabled BOOLEAN NOT NULL DEFAULT true,
    file_count INTEGER NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS scout_files (
    id INTEGER PRIMARY KEY,  -- DuckDB auto-generates
    source_id VARCHAR NOT NULL REFERENCES scout_sources(id),
    path VARCHAR NOT NULL,
    rel_path VARCHAR NOT NULL,
    parent_path VARCHAR NOT NULL DEFAULT '',
    name VARCHAR NOT NULL DEFAULT '',
    extension VARCHAR,
    size BIGINT NOT NULL,
    mtime BIGINT NOT NULL,
    content_hash VARCHAR,
    status VARCHAR NOT NULL DEFAULT 'pending',
    tag VARCHAR,
    tag_source VARCHAR,
    rule_id VARCHAR,
    manual_plugin VARCHAR,
    error VARCHAR,
    first_seen_at BIGINT NOT NULL,
    last_seen_at BIGINT NOT NULL,
    processed_at BIGINT,
    sentinel_job_id INTEGER,
    metadata_raw VARCHAR,
    extraction_status VARCHAR DEFAULT 'pending',
    extracted_at BIGINT,
    UNIQUE(source_id, path)
);

-- Indexes (DuckDB syntax is identical)
CREATE INDEX IF NOT EXISTS idx_files_source ON scout_files(source_id);
CREATE INDEX IF NOT EXISTS idx_files_status ON scout_files(status);
-- ... rest of indexes
```

**3.2 Batch Insert Optimization**

DuckDB has a different optimal pattern for bulk inserts:

```rust
// Before (SQLite bulk INSERT with dynamic VALUES)
let values = files.iter().map(|_| "(?, ?, ...)").collect();
let sql = format!("INSERT INTO scout_files VALUES {}", values);

// After (DuckDB Appender - MUCH faster)
impl Database {
    pub async fn batch_upsert_files_duckdb(
        &self,
        files: &[ScannedFile],
        tag: Option<&str>,
    ) -> Result<BatchUpsertResult> {
        self.conn.conn(|conn| {
            // Use DuckDB's Appender for bulk insert
            let mut appender = conn.appender("scout_files")?;

            for file in files {
                appender.append_row(params![
                    file.source_id,
                    file.path,
                    file.rel_path,
                    // ... rest of fields
                ])?;
            }

            appender.flush()?;

            // Handle conflicts separately (UPDATE for changed files)
            // DuckDB doesn't have ON CONFLICT in Appender

            Ok(BatchUpsertResult { ... })
        }).await
    }
}
```

**3.3 Alternative: INSERT ON CONFLICT works too**

DuckDB supports INSERT ON CONFLICT, just not through Appender:
```sql
INSERT INTO scout_files (source_id, path, ...)
VALUES ($1, $2, ...)
ON CONFLICT (source_id, path) DO UPDATE SET
    size = excluded.size,
    mtime = excluded.mtime,
    ...
```

**Decision:** Use INSERT ON CONFLICT for compatibility, benchmark Appender for future optimization.

**3.4 Migration Detection (PRAGMA table_info)**

```rust
// Before (SQLite)
sqlx::query_as::<_, (String,)>(
    "SELECT name FROM pragma_table_info('scout_files') WHERE name = 'metadata_raw'"
)

// After (DuckDB)
self.conn.query_scalar::<String>(
    "SELECT column_name FROM information_schema.columns
     WHERE table_name = 'scout_files' AND column_name = 'metadata_raw'"
)
```

**3.5 Tests**
- `test_batch_upsert_10k_files_duckdb`
- `test_scan_source_duckdb`
- `test_list_files_with_filters_duckdb`
- `test_folder_hierarchy_duckdb`

---

### Phase 4: Sentinel Queue Migration (Week 5)

**Goal:** Migrate job queue with atomic claiming.

**Critical Pattern:** Atomic job claiming must work identically.

```rust
// Before (SQLite)
let mut tx = pool.begin().await?;
let job_id: Option<i64> = sqlx::query_scalar(
    "SELECT id FROM cf_processing_queue WHERE status = 'QUEUED' ORDER BY priority DESC LIMIT 1"
).fetch_optional(&mut *tx).await?;

if let Some(id) = job_id {
    sqlx::query("UPDATE cf_processing_queue SET status = 'RUNNING' WHERE id = ? AND status = 'QUEUED'")
        .bind(id)
        .execute(&mut *tx)
        .await?;
}
tx.commit().await?;

// After (DuckDB)
self.conn.transaction(|ctx| {
    let job_id: Option<i64> = ctx.query_scalar(
        "SELECT id FROM cf_processing_queue WHERE status = 'QUEUED' ORDER BY priority DESC LIMIT 1"
    )?;

    if let Some(id) = job_id {
        let rows = ctx.execute(
            "UPDATE cf_processing_queue SET status = 'RUNNING' WHERE id = $1 AND status = 'QUEUED'",
            &[id]
        )?;
        if rows == 0 {
            return Ok(None);  // Another worker claimed it
        }
        // Fetch and return
    }
    Ok(Some(job))
}).await
```

**Tests:**
- `test_concurrent_job_claiming` - Critical race condition test
- `test_dead_letter_queue_duckdb`
- `test_job_requeue_duckdb`

---

### Phase 5: Glob & Parquet Integration (Week 6)

**Goal:** Leverage DuckDB's killer features for file pattern queries.

**5.1 Native Glob Queries**

Add new APIs that exploit DuckDB's glob:

```rust
impl Database {
    /// Query files directly from filesystem using DuckDB glob
    /// No DB roundtrip needed!
    #[cfg(feature = "duckdb")]
    pub async fn glob_files(&self, pattern: &str) -> Result<Vec<GlobResult>> {
        self.conn.conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT file, size, last_modified FROM glob($1)"
            )?;

            let results = stmt.query_map([pattern], |row| {
                Ok(GlobResult {
                    path: row.get(0)?,
                    size: row.get(1)?,
                    mtime: row.get(2)?,
                })
            })?;

            results.collect()
        }).await
    }

    /// Query Parquet files directly
    #[cfg(feature = "duckdb")]
    pub async fn query_parquet(
        &self,
        pattern: &str,
        sql: &str,
    ) -> Result<Vec<serde_json::Value>> {
        self.conn.conn(move |conn| {
            // Replace FROM placeholder with read_parquet
            let full_sql = format!(
                "SELECT * FROM read_parquet('{}') {}",
                pattern,
                sql.replace("FROM __parquet__", "")
            );

            let mut stmt = conn.prepare(&full_sql)?;
            // Convert to JSON values
            // ...
        }).await
    }
}
```

**5.2 TUI Pattern Iteration Mode**

New workflow enabled by DuckDB:

```rust
// In TUI Discover mode, user can test patterns instantly
async fn preview_pattern(&self, pattern: &str) -> Result<PatternPreview> {
    // No database write needed!
    let files = self.db.glob_files(pattern).await?;

    Ok(PatternPreview {
        count: files.len(),
        sample: files.into_iter().take(20).collect(),
        extensions: count_extensions(&files),
    })
}
```

**5.3 Direct Parser Output Queries**

```rust
// Query parser output files directly
let sql = "SELECT * FROM read_parquet('./output/orders_*.parquet') WHERE amount > 1000";
let results = db.query_parquet_raw(sql).await?;
```

---

### Phase 6: CLI & TUI Integration (Week 7)

**Goal:** Update all entry points to use DuckDB by default.

**6.1 Database Initialization**

```rust
// main.rs
async fn get_database() -> Result<Database> {
    let db_path = get_db_path();

    #[cfg(feature = "duckdb")]
    {
        // Try to acquire write lock
        match Database::open_exclusive(&db_path).await {
            Ok(db) => return Ok(db),
            Err(DbError::Locked) => {
                // Another process has the lock
                if std::env::var("CASPARIAN_READONLY").is_ok() {
                    return Database::open_readonly(&db_path).await;
                }
                return Err(anyhow!("Database locked by another process. Set CASPARIAN_READONLY=1 for read-only access."));
            }
            Err(e) => return Err(e.into()),
        }
    }

    #[cfg(feature = "sqlite")]
    {
        Database::open(&db_path).await
    }
}
```

**6.2 TUI Updates**

- Add process lock indicator in status bar
- Add read-only mode indicator
- Update Settings to show database backend

**6.3 CLI Updates**

```bash
# New flags
casparian scan /path --read-only  # Force read-only mode
casparian tui --backend duckdb    # Explicit backend selection
casparian files --glob "*.csv"    # Use DuckDB glob directly
```

---

### Phase 7: Testing & Validation (Week 8)

**Goal:** Comprehensive testing using tmux for TUI workflows.

**7.1 Unit Test Updates**

All existing tests must pass with both backends:

```rust
#[tokio::test]
async fn test_example() {
    test_with_backends(|backend| async move {
        let db = create_test_db(backend).await;
        // ... test logic
    }).await;
}

async fn test_with_backends<F, Fut>(test_fn: F)
where
    F: Fn(DbBackendType) -> Fut,
    Fut: Future<Output = ()>,
{
    #[cfg(feature = "sqlite")]
    test_fn(DbBackendType::Sqlite).await;

    #[cfg(feature = "duckdb")]
    test_fn(DbBackendType::DuckDb).await;
}
```

**7.2 TMux TUI Workflow Tests**

```bash
#!/bin/bash
# scripts/test-duckdb-tui.sh

# Kill any existing session
tmux kill-session -t duckdb_test 2>/dev/null

# Build with DuckDB
cargo build --release --features duckdb

# Start TUI in tmux
tmux new-session -d -s duckdb_test -x 120 -y 40 \
    "./target/release/casparian tui"

# Wait for startup
sleep 2

# Test 1: Navigate to Discover mode
tmux send-keys -t duckdb_test "1"
sleep 1
./scripts/tui-capture.sh "Discover mode"

# Test 2: Add a source
tmux send-keys -t duckdb_test "s"
sleep 0.5
tmux send-keys -t duckdb_test "/tmp/test_data"
tmux send-keys -t duckdb_test Enter
sleep 2
./scripts/tui-capture.sh "After add source"

# Test 3: Verify files appear
./scripts/tui-capture.sh "File list" | grep -q "files found"
if [ $? -eq 0 ]; then
    echo "PASS: Files discovered"
else
    echo "FAIL: Files not discovered"
    exit 1
fi

# Cleanup
tmux kill-session -t duckdb_test
```

**7.3 Workflow Test Matrix**

| Workflow | SQLite | DuckDB | Status |
|----------|--------|--------|--------|
| TUI: Add source | | | |
| TUI: Scan directory | | | |
| TUI: Create tagging rule | | | |
| TUI: Apply tag to files | | | |
| CLI: `casparian scan` | | | |
| CLI: `casparian files --tag X` | | | |
| CLI: `casparian run parser.py` | | | |
| CLI: Concurrent scans | | | |
| Parser output query | | | |
| Schema contract create | | | |
| Sentinel job queue | | | |

---

## 5. API Translation Guide

### 5.1 Query Patterns

| SQLite (sqlx) | DuckDB (async-duckdb) |
|---------------|----------------------|
| `sqlx::query("...").execute(&pool)` | `client.conn(\|c\| c.execute("..."))` |
| `sqlx::query_as::<_, T>("...").fetch_all(&pool)` | `client.conn(\|c\| query_rows::<T>(c, "..."))` |
| `sqlx::query_scalar::<_, T>("...").fetch_one(&pool)` | `client.conn(\|c\| c.query_row("...", \|r\| r.get(0)))` |
| `.bind(value)` | `params![value]` |
| `pool.begin().await?` | `c.execute_batch("BEGIN")` |
| `tx.commit().await?` | `c.execute_batch("COMMIT")` |

### 5.2 Row Mapping

```rust
// SQLite (sqlx::FromRow derive)
#[derive(sqlx::FromRow)]
struct MyRow {
    id: i64,
    name: String,
}

// DuckDB (manual mapping)
struct MyRow {
    id: i64,
    name: String,
}

impl MyRow {
    fn from_row(row: &duckdb::Row) -> duckdb::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    }
}

// Or use a macro to generate this
macro_rules! impl_from_duckdb_row {
    ($type:ty, $($field:ident => $idx:expr),*) => {
        impl $type {
            fn from_duckdb_row(row: &duckdb::Row) -> duckdb::Result<Self> {
                Ok(Self {
                    $($field: row.get($idx)?),*
                })
            }
        }
    };
}
```

### 5.3 SQL Syntax Mapping

| SQLite | DuckDB | Notes |
|--------|--------|-------|
| `?` | `$1, $2, ...` | Positional params |
| `datetime('now')` | `current_timestamp` | Current time |
| `AUTOINCREMENT` | (implicit) | Remove keyword |
| `INTEGER PRIMARY KEY` | `INTEGER PRIMARY KEY` | Same |
| `TEXT` | `VARCHAR` | Either works |
| `PRAGMA table_info(t)` | `DESCRIBE t` | Schema introspection |

---

## 6. Concurrency Model Changes

### 6.1 Current Model (SQLite)

```
Process A (TUI)          Process B (CLI scan)
     │                         │
     ▼                         ▼
┌─────────┐              ┌─────────┐
│ SqlitePool │           │ SqlitePool │
│ (5 conns)  │           │ (5 conns)  │
└─────────┘              └─────────┘
     │                         │
     └───────────┬─────────────┘
                 ▼
         ┌─────────────┐
         │ SQLite file │  WAL mode allows concurrent access
         │ (WAL mode)  │
         └─────────────┘
```

### 6.2 New Model (DuckDB)

```
                    ┌──────────────────────┐
                    │ File Lock            │
                    │ casparian_flow.lock  │
                    └──────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    ▼                   ▼
             Lock acquired        Lock failed
                    │                   │
                    ▼                   ▼
            ┌─────────────┐      ┌─────────────┐
            │ Writer Mode │      │ Reader Mode │
            │ (exclusive) │      │ (read-only) │
            └─────────────┘      └─────────────┘
                    │                   │
                    ▼                   ▼
            ┌─────────────┐      ┌─────────────┐
            │ DuckDB      │      │ DuckDB      │
            │ Client      │      │ Pool        │
            │ (write)     │      │ (read-only) │
            └─────────────┘      └─────────────┘
                    │                   │
                    └─────────┬─────────┘
                              ▼
                      ┌─────────────┐
                      │ DuckDB file │
                      └─────────────┘
```

### 6.3 Multi-Process Coordination

```rust
/// Process-level database coordinator
pub struct DbCoordinator {
    /// Path to the database file
    db_path: PathBuf,

    /// Lock file handle (holds exclusive lock if writer)
    lock: Option<FileLock>,

    /// Connection (writer or reader)
    conn: DbConnection,
}

impl DbCoordinator {
    pub async fn acquire(db_path: PathBuf) -> Result<Self> {
        let lock_path = db_path.with_extension("duckdb.lock");

        // Try to acquire exclusive lock (non-blocking)
        match FileLock::try_exclusive(&lock_path) {
            Ok(lock) => {
                // We're the writer
                let conn = DbConnection::open_writer(&db_path).await?;
                Ok(Self { db_path, lock: Some(lock), conn })
            }
            Err(LockError::WouldBlock) => {
                // Another process is writing; open read-only
                let conn = DbConnection::open_reader(&db_path).await?;
                Ok(Self { db_path, lock: None, conn })
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn is_writer(&self) -> bool {
        self.lock.is_some()
    }

    pub async fn upgrade_to_writer(&mut self) -> Result<()> {
        if self.is_writer() {
            return Ok(());
        }

        // Close reader connection
        self.conn.close().await?;

        // Acquire lock (blocking)
        let lock_path = self.db_path.with_extension("duckdb.lock");
        let lock = FileLock::exclusive(&lock_path).await?;

        // Reopen as writer
        self.conn = DbConnection::open_writer(&self.db_path).await?;
        self.lock = Some(lock);

        Ok(())
    }
}
```

---

## 7. Test Strategy

### 7.1 Test Categories

| Category | Count | Backend Testing |
|----------|-------|-----------------|
| Unit tests | ~150 | Both |
| Integration tests | ~30 | Both |
| E2E tests | ~15 | Both |
| TUI tests (tmux) | ~10 | Both |
| Performance tests | ~5 | Both + comparison |

### 7.2 Test Infrastructure Changes

```rust
// crates/casparian_test_utils/src/lib.rs

/// Create a test database with the appropriate backend
pub async fn create_test_db() -> TestDb {
    #[cfg(feature = "duckdb")]
    {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let db = Database::open_duckdb(temp.path()).await.unwrap();
        TestDb { db, _temp: temp }
    }

    #[cfg(all(feature = "sqlite", not(feature = "duckdb")))]
    {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let db = Database::open_sqlite(temp.path()).await.unwrap();
        TestDb { db, _temp: temp }
    }
}

/// Run test with all available backends
pub async fn test_all_backends<F, Fut>(test_fn: F)
where
    F: Fn(Database) -> Fut + Clone,
    Fut: Future<Output = ()>,
{
    #[cfg(feature = "sqlite")]
    {
        let db = create_test_db_sqlite().await;
        test_fn.clone()(db.db).await;
    }

    #[cfg(feature = "duckdb")]
    {
        let db = create_test_db_duckdb().await;
        test_fn(db.db).await;
    }
}
```

### 7.3 Performance Comparison Tests

```rust
#[tokio::test]
async fn bench_batch_insert_sqlite_vs_duckdb() {
    let files: Vec<ScannedFile> = generate_test_files(100_000);

    // SQLite benchmark
    #[cfg(feature = "sqlite")]
    {
        let db = create_test_db_sqlite().await;
        let start = Instant::now();
        db.batch_upsert_files(&files, None).await.unwrap();
        println!("SQLite: {:?}", start.elapsed());
    }

    // DuckDB benchmark
    #[cfg(feature = "duckdb")]
    {
        let db = create_test_db_duckdb().await;
        let start = Instant::now();
        db.batch_upsert_files(&files, None).await.unwrap();
        println!("DuckDB: {:?}", start.elapsed());
    }
}

#[tokio::test]
async fn bench_analytical_query_sqlite_vs_duckdb() {
    // Pre-populate with 1M files
    let db_sqlite = setup_large_db_sqlite().await;
    let db_duckdb = setup_large_db_duckdb().await;

    let query = r#"
        SELECT extension, COUNT(*) as count, SUM(size) as total_size
        FROM scout_files
        WHERE mtime > $1
        GROUP BY extension
        ORDER BY count DESC
        LIMIT 20
    "#;

    // SQLite
    let start = Instant::now();
    db_sqlite.query(query, &[yesterday]).await.unwrap();
    println!("SQLite analytical: {:?}", start.elapsed());

    // DuckDB
    let start = Instant::now();
    db_duckdb.query(query, &[yesterday]).await.unwrap();
    println!("DuckDB analytical: {:?}", start.elapsed());

    // DuckDB should be 10-50x faster for this query
}
```

### 7.4 TMux Workflow Tests

```bash
#!/bin/bash
# tests/tmux/test_duckdb_workflows.sh

set -e

# Test matrix
BACKENDS=("sqlite" "duckdb")

for backend in "${BACKENDS[@]}"; do
    echo "Testing with $backend backend..."

    # Build with specific backend
    cargo build --release --no-default-features --features "$backend"

    # Run workflow tests
    ./scripts/tui-test.sh discover_add_source
    ./scripts/tui-test.sh discover_scan
    ./scripts/tui-test.sh discover_create_rule
    ./scripts/tui-test.sh parser_bench_create
    ./scripts/tui-test.sh jobs_view

    echo "$backend: ALL TESTS PASSED"
done
```

---

## 8. Rollback Plan

### 8.1 Rollback Triggers

| Trigger | Severity | Action |
|---------|----------|--------|
| Data corruption | Critical | Immediate rollback |
| Performance regression > 20% | High | Investigate, rollback if unresolved |
| Concurrency bugs | High | Rollback, fix, re-deploy |
| Minor query failures | Medium | Fix forward |
| Feature parity gaps | Low | Fix forward |

### 8.2 Rollback Procedure

1. **Immediate:** Revert feature flag default
   ```toml
   # Cargo.toml
   default = ["sqlite"]  # Revert from "duckdb"
   ```

2. **Build and deploy:**
   ```bash
   cargo build --release --features sqlite
   ```

3. **Data preservation:**
   - DuckDB files remain on disk
   - SQLite continues to work with existing `.sqlite` files
   - No data migration needed for rollback

### 8.3 Data Migration (If Needed)

```rust
/// Export DuckDB to SQLite for rollback
async fn migrate_duckdb_to_sqlite(
    duckdb_path: &Path,
    sqlite_path: &Path,
) -> Result<()> {
    let duck = async_duckdb::ClientBuilder::new()
        .path(duckdb_path)
        .read_only()
        .open()
        .await?;

    let sqlite = SqlitePool::connect(&format!("sqlite:{}", sqlite_path.display())).await?;

    // Export each table
    for table in ["scout_sources", "scout_files", "scout_tagging_rules", ...] {
        let rows = duck.conn(|c| {
            let mut stmt = c.prepare(&format!("SELECT * FROM {}", table))?;
            // ... fetch all rows
        }).await?;

        // Insert into SQLite
        for row in rows {
            // ... insert
        }
    }

    Ok(())
}
```

---

## 9. Performance Validation

### 9.1 Benchmarks to Run

| Benchmark | Target | SQLite Baseline | DuckDB Expected |
|-----------|--------|-----------------|-----------------|
| Batch insert 100K files | < 10s | 8s | 3s |
| Analytical query (1M files) | < 100ms | 2s | 50ms |
| Glob pattern (10K files) | < 50ms | 200ms (filesystem) | 30ms |
| Parquet query (1GB) | < 1s | N/A | 500ms |
| TUI refresh (10K files) | < 100ms | 150ms | 80ms |

### 9.2 Benchmark Suite

```bash
# Run full benchmark suite
cargo bench --features "sqlite duckdb" -- --save-baseline migration

# Compare results
cargo bench --features duckdb -- --baseline migration
```

### 9.3 Memory Profiling

DuckDB uses more memory than SQLite. Monitor:

```bash
# Memory usage during large scan
/usr/bin/time -v ./target/release/casparian scan /large/dir --features duckdb
```

Expected: 2-3x memory usage vs SQLite, but much faster queries.

---

## 10. Implementation Checklist

### Phase 1: Foundation
- [ ] Add async-duckdb, duckdb dependencies to Cargo.toml
- [ ] Create `DbBackend` enum in `casparian_db/src/backend.rs`
- [ ] Create `DbConnection` wrapper struct
- [ ] Create `DbExecutor` trait
- [ ] Implement `DbExecutor` for DuckDB
- [ ] Add `DbConfig::duckdb()` constructor
- [ ] Add process lock file mechanism
- [ ] Unit tests: open, query, close
- [ ] CI: Add DuckDB feature to test matrix

### Phase 2: Schema Layer
- [ ] Update `SchemaStorage` to use `DbConnection`
- [ ] Convert all queries to unified interface
- [ ] Update schema initialization SQL for DuckDB
- [ ] Integration tests
- [ ] Verify contracts work correctly

### Phase 3: Scout Database
- [ ] Create DuckDB schema file
- [ ] Update `Database` struct
- [ ] Convert source operations
- [ ] Convert file operations (including batch)
- [ ] Convert folder operations
- [ ] Convert tagging rule operations
- [ ] Update migration detection
- [ ] Integration tests
- [ ] Performance benchmarks

### Phase 4: Sentinel Queue
- [ ] Update queue schema for DuckDB
- [ ] Convert job claiming with transactions
- [ ] Convert dead letter queue operations
- [ ] Test concurrent job claiming
- [ ] Integration tests

### Phase 5: Glob & Parquet
- [ ] Add `glob_files()` API
- [ ] Add `query_parquet()` API
- [ ] Update TUI pattern preview
- [ ] Add CLI `--glob` flag
- [ ] Integration tests
- [ ] Documentation

### Phase 6: CLI & TUI Integration
- [ ] Update database initialization in main.rs
- [ ] Add read-only mode support
- [ ] Add lock status to TUI
- [ ] Update Settings view
- [ ] Update CLI help text
- [ ] E2E tests

### Phase 7: Testing & Validation
- [ ] Update test infrastructure
- [ ] Run all unit tests with both backends
- [ ] Run all integration tests
- [ ] Run TUI tests via tmux
- [ ] Performance benchmarks
- [ ] Memory profiling
- [ ] Documentation review

### Phase 8: Deployment
- [ ] Update default feature to `duckdb`
- [ ] Update README
- [ ] Update CLAUDE.md
- [ ] Release notes
- [ ] Monitor production usage

---

## Appendix A: File Changes Summary

| File | Change Type | Complexity |
|------|-------------|------------|
| `casparian_db/Cargo.toml` | Add deps | Low |
| `casparian_db/src/lib.rs` | Add exports | Low |
| `casparian_db/src/backend.rs` | New file | Medium |
| `casparian_db/src/query.rs` | New file | Medium |
| `casparian_db/src/pool.rs` | Major update | Medium |
| `casparian_db/src/lock.rs` | New file | Medium |
| `casparian_schema/src/storage.rs` | Query updates | Medium |
| `casparian/src/scout/db.rs` | Major rewrite | High |
| `casparian/src/scout/scanner.rs` | Minor updates | Low |
| `casparian_sentinel/src/db/queue.rs` | Major update | High |
| `casparian/src/main.rs` | Init updates | Medium |
| `casparian/src/cli/tui/app.rs` | Lock indicator | Low |

---

## Appendix B: SQL Compatibility Cheat Sheet

```sql
-- =============================================
-- SQLite → DuckDB Conversion Examples
-- =============================================

-- 1. Auto-increment
-- SQLite:
CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT);
-- DuckDB:
CREATE TABLE t (id INTEGER PRIMARY KEY);

-- 2. Current timestamp
-- SQLite:
DEFAULT (datetime('now'))
-- DuckDB:
DEFAULT current_timestamp

-- 3. Boolean
-- SQLite:
enabled INTEGER NOT NULL DEFAULT 1
-- DuckDB:
enabled BOOLEAN NOT NULL DEFAULT true

-- 4. Check for table existence
-- SQLite:
SELECT name FROM sqlite_master WHERE type='table' AND name='t';
-- DuckDB:
SELECT table_name FROM information_schema.tables WHERE table_name = 't';

-- 5. Table info
-- SQLite:
PRAGMA table_info('t');
-- DuckDB:
DESCRIBE t;
-- Or:
SELECT * FROM information_schema.columns WHERE table_name = 't';

-- 6. ON CONFLICT (works in both!)
INSERT INTO t (id, val) VALUES (1, 'a')
ON CONFLICT (id) DO UPDATE SET val = excluded.val;

-- 7. GLOB → SIMILAR TO
-- SQLite:
WHERE path GLOB '*.csv'
-- DuckDB:
WHERE path SIMILAR TO '%.csv'
-- Or using LIKE:
WHERE path LIKE '%.csv'
```

---

## Appendix C: async-duckdb Patterns

```rust
// =============================================
// async-duckdb Usage Patterns
// =============================================

use async_duckdb::{Client, ClientBuilder, Error};

// 1. Open database
let client = ClientBuilder::new()
    .path("/path/to/db.duckdb")
    .open()
    .await?;

// 2. Execute without result
client.conn(|conn| {
    conn.execute("CREATE TABLE t (id INTEGER, name VARCHAR)", [])?;
    Ok(())
}).await?;

// 3. Query single value
let count: i64 = client.conn(|conn| {
    conn.query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))
}).await?;

// 4. Query multiple rows
let names: Vec<String> = client.conn(|conn| {
    let mut stmt = conn.prepare("SELECT name FROM t WHERE id > ?")?;
    let rows = stmt.query_map([10], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>()
}).await?;

// 5. Transaction
client.conn(|conn| {
    conn.execute_batch("BEGIN TRANSACTION")?;

    match do_work(conn) {
        Ok(result) => {
            conn.execute_batch("COMMIT")?;
            Ok(result)
        }
        Err(e) => {
            conn.execute_batch("ROLLBACK")?;
            Err(e)
        }
    }
}).await?;

// 6. Bulk insert with Appender
client.conn(|conn| {
    let mut appender = conn.appender("t")?;
    for i in 0..10000 {
        appender.append_row(params![i, format!("name_{}", i)])?;
    }
    appender.flush()?;
    Ok(())
}).await?;

// 7. Read Parquet files
let results: Vec<Row> = client.conn(|conn| {
    let mut stmt = conn.prepare("SELECT * FROM read_parquet('data/*.parquet')")?;
    // ...
}).await?;

// 8. Use glob
let files: Vec<String> = client.conn(|conn| {
    let mut stmt = conn.prepare("SELECT file FROM glob('/data/**/*.csv')")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.collect::<Result<Vec<_>, _>>()
}).await?;
```

---

## Appendix D: Risk Matrix

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Multi-process lock conflicts | Medium | High | Clear error messages, --read-only flag |
| Query syntax incompatibility | Low | Medium | Comprehensive test suite |
| Performance regression | Low | Medium | Benchmarks, rollback plan |
| Memory usage spikes | Medium | Low | Monitor, document limits |
| async-duckdb bugs | Low | High | Pin version, contribute fixes |
| Transaction semantics differ | Low | High | Thorough testing |
| Data corruption | Very Low | Critical | Backups, rollback plan |

---

## Revision History

| Date | Version | Author | Changes |
|------|---------|--------|---------|
| 2026-01-16 | 1.0 | AI | Initial draft |
