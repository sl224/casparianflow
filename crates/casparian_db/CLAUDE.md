# Claude Code Instructions for casparian_db

**Status**: canonical
**Last verified against code**: 2026-01-24
**Key code references**: `src/backend.rs`, `src/lib.rs`, `src/lock.rs`

## Quick Reference

```bash
cargo test -p casparian_db                    # All tests
cargo check -p casparian_db                   # Type check
```

---

## Overview

`casparian_db` provides the **Database Abstraction Layer** for Casparian Flow. It enables:
- **DuckDB-only database access** (no SQLite, no PostgreSQL)
- **File-based locking** for single-writer enforcement
- **Unified connection API** via `DbConnection`

### Design Principles

1. **DuckDB only** - Columnar OLAP database for analytics workloads
2. **Single-writer enforcement** - File locking via `fs2` crate
3. **Read-only mode** - Multiple readers allowed simultaneously
4. **Single source of truth** - All crates use `DbConnection` for DB access
5. **Synchronous API** - No async, uses `duckdb::Connection` directly

---

## Key Types

### DatabaseType

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    DuckDb,  // Only variant
}
```

### DbConnection

```rust
pub struct DbConnection {
    conn: Rc<duckdb::Connection>,
    access_mode: AccessMode,
    lock_guard: Option<Rc<DbLockGuard>>,
}

// Constructors
DbConnection::open_duckdb(Path::new("./data.duckdb"))?     // Read-write with exclusive lock
DbConnection::open_duckdb_readonly(Path::new("./data.duckdb"))? // Read-only, no lock
DbConnection::open_duckdb_memory()?                         // In-memory for testing
DbConnection::open_from_url("duckdb:./data.duckdb")?       // URL-based
```

### AccessMode

```rust
pub enum AccessMode {
    ReadWrite,  // Requires exclusive file lock
    ReadOnly,   // No lock required
}
```

### DbValue

```rust
pub enum DbValue {
    Null,
    Integer(i64),
    Float(f64),
    Text(String),
    Blob(Vec<u8>),
    Boolean(bool),
    Timestamp(i64),  // Microseconds since epoch
}
```

---

## Usage

### Standard API

```rust
use casparian_db::DbConnection;

// Open with exclusive lock (read-write)
let conn = DbConnection::open_duckdb(Path::new("./data.duckdb"))?;

// Execute SQL
conn.execute("INSERT INTO t VALUES (?)", &[DbValue::Integer(42)])?;

// Query
let rows = conn.query_all("SELECT * FROM t", &[])?;
for row in rows {
    let id: i64 = row.get_by_index(0)?;
}

// Transaction
conn.transaction(|tx| {
    tx.execute("INSERT INTO t VALUES (1)", &[])?;
    tx.execute("INSERT INTO t VALUES (2)", &[])?;
    Ok(())
})?;
```

### Bulk Insert (High Performance)

```rust
// Uses DuckDB APPENDER for fast row insertion
conn.bulk_insert_rows(
    "my_table",
    &["col1", "col2"],
    &[
        vec![DbValue::Integer(1), DbValue::Text("a".into())],
        vec![DbValue::Integer(2), DbValue::Text("b".into())],
    ],
)?;
```

---

## Integration Guide

### For Crate Authors

To use `casparian_db` in another crate:

1. Add dependency:
```toml
[dependencies]
casparian_db = { path = "../casparian_db" }
```

2. Use DbConnection:
```rust
use casparian_db::DbConnection;

// Read-write access (gets exclusive lock)
let conn = DbConnection::open_duckdb(Path::new(&path))?;

// Read-only access (shared)
let conn = DbConnection::open_duckdb_readonly(Path::new(&path))?;
```

---

## Error Handling

`DbConnection` returns `BackendError`:

```rust
pub enum BackendError {
    Database(String),         // DuckDB errors
    Locked(String),           // File lock acquisition failed
    ReadOnly,                 // Write attempted on read-only connection
    Query(String),            // Query execution error
    Transaction(String),      // Transaction error
    TypeConversion(String),   // Type conversion failed
    NotAvailable(String),     // Feature not available
}
```

---

## File Structure

```
casparian_db/
├── CLAUDE.md           # This file
├── Cargo.toml
└── src/
    ├── lib.rs          # DatabaseType, exports
    ├── backend.rs      # DbConnection, DbValue, query methods
    ├── lock.rs         # File-based locking (fs2)
    └── sql_guard.rs    # SQL safety checks
```

---

## File Locking

DuckDB doesn't provide built-in distributed locking. We use `fs2` for file-based locking:

```rust
// In lock.rs
pub fn try_lock_exclusive(db_path: &Path) -> Result<DbLockGuard, LockError> {
    let lock_path = db_path.with_extension("duckdb.lock");
    let file = File::create(&lock_path)?;
    file.try_lock_exclusive()?;  // Non-blocking
    Ok(DbLockGuard { file, lock_path })
}
```

- **Read-write connections**: Acquire exclusive lock on `.duckdb.lock` file
- **Read-only connections**: No lock required
- **Lock contention**: Returns `BackendError::Locked` if file is already locked

---

## Testing

```rust
#[test]
fn test_duckdb_connection() {
    let conn = DbConnection::open_duckdb_memory().unwrap();

    conn.execute("CREATE TABLE t (id INTEGER)", &[]).unwrap();
    conn.execute("INSERT INTO t VALUES (?)", &[DbValue::Integer(42)]).unwrap();

    let rows = conn.query_all("SELECT * FROM t", &[]).unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn test_lock_contention() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("test.duckdb");

    let conn1 = DbConnection::open_duckdb(&path).unwrap();  // Gets lock
    let result = DbConnection::open_duckdb(&path);           // Should fail

    assert!(matches!(result, Err(BackendError::Locked(_))));
}
```

---

## Key Principles

1. **DuckDB only** - No SQLite, no PostgreSQL, no sqlx
2. **Single-writer** - Exclusive file lock for write access
3. **Synchronous API** - No async, direct duckdb::Connection usage
4. **Bulk operations** - Use APPENDER for high-performance inserts
5. **Read-only mode** - Explicit mode for query-only access
