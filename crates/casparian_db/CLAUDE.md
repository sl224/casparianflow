# Claude Code Instructions for casparian_db

## Quick Reference

```bash
cargo test -p casparian_db                    # All tests
cargo check -p casparian_db                   # Type check
cargo check -p casparian_db --features postgres  # With PostgreSQL
```

---

## Overview

`casparian_db` provides the **Database Abstraction Layer** for Casparian Flow. It enables:
- **Feature-gated database support** (compile-time)
- **License-controlled enterprise features** (runtime)
- **Unified pool creation** across all crates

### Design Principles

1. **SQLite is always free** - Community tier, no license required
2. **Enterprise DBs require license** - PostgreSQL (Professional), MSSQL (Enterprise)
3. **Single source of truth** - All crates use this crate for database types
4. **Unified connection API** - Use `DbConnection` for backend-agnostic access
5. **Fail at connection time** - License check happens when creating pool, not at query time

---

## Feature Flags

| Feature | Description | License Required |
|---------|-------------|------------------|
| `sqlite` | SQLite support (default) | No |
| `postgres` | PostgreSQL support | Professional |
| `mssql` | MSSQL support (planned) | Enterprise |

Build with specific features:
```bash
cargo build --features sqlite           # Default
cargo build --features sqlite,postgres  # Both
cargo build --no-default-features --features postgres  # Postgres only
```

---

## License Tiers

| Tier | SQLite | PostgreSQL | MSSQL | Price |
|------|--------|------------|-------|-------|
| Community | ✓ | ✗ | ✗ | Free |
| Professional | ✓ | ✓ | ✗ | Paid |
| Enterprise | ✓ | ✓ | ✓ | Paid |

---

## Key Types

### DatabaseType

```rust
#[non_exhaustive]
pub enum DatabaseType {
    #[cfg(feature = "sqlite")]
    Sqlite,
    #[cfg(feature = "postgres")]
    Postgres,
}
```

### DbConnection

```rust
pub struct DbConnection { /* ... */ }

// Constructors
DbConnection::open_sqlite(Path::new("./data.db")).await?
DbConnection::open_sqlite_memory().await?
DbConnection::open_duckdb(Path::new("./data.duckdb")).await?
DbConnection::open_postgres("postgres://localhost/mydb").await?
DbConnection::open_from_url("sqlite:./data.db").await?
```

### Legacy DbConfig (sqlx pool)

Available via `casparian_db::legacy::DbConfig`.

```rust
pub struct DbConfig {
    pub url: String,
    pub db_type: DatabaseType,
    pub max_connections: u32,
    pub license: License,
}

// Constructors
DbConfig::sqlite("./data.db")              // SQLite file
DbConfig::sqlite_memory()                   // In-memory SQLite
DbConfig::postgres(url, license)            // PostgreSQL (requires license)
DbConfig::from_url(url, license)            // Auto-detect from URL
```

### License

```rust
pub struct License {
    pub organization: String,
    pub tier: LicenseTier,
    pub expires_at: Option<i64>,
    pub license_id: String,
}

// Load from file
let license = License::load(Path::new("license.json"))?;

// Default (Community)
let license = License::community();

// Auto-load from standard locations
let license = load_license();
```

### LicenseTier

```rust
pub enum LicenseTier {
    Community,      // SQLite only
    Professional,   // + PostgreSQL
    Enterprise,     // + MSSQL
}
```

---

## Usage

### Unified API (recommended)

```rust
use casparian_db::DbConnection;

let conn = DbConnection::open_sqlite(Path::new("./data.db")).await?;
conn.execute("SELECT 1", &[]).await?;
```

### Legacy sqlx pool (deprecated)

```rust
use casparian_db::legacy::{create_pool, DbConfig};

let config = DbConfig::sqlite("./data.db");
let pool = create_pool(config).await?;
```

### PostgreSQL (License Required, legacy pool)

```rust
use casparian_db::legacy::{create_pool, load_license, DbConfig};

let license = load_license();  // Loads from standard locations
let config = DbConfig::postgres("postgres://localhost/mydb", license);

// This will fail if license doesn't allow PostgreSQL
let pool = create_pool(config).await?;
```

### License File Format

```json
{
  "organization": "Acme Corp",
  "tier": "Professional",
  "expires_at": 1735689600,
  "license_id": "lic_abc123",
  "signature": "base64_ed25519_signature"
}
```

License file locations (checked in order):
1. `CASPARIAN_LICENSE` environment variable
2. `~/.casparian_flow/license.json`
3. `./license.json`

---

## Integration Guide

### For Crate Authors

To use `casparian_db` in another crate:

1. Add dependency:
```toml
[dependencies]
casparian_db = { path = "../casparian_db" }
```

2. Replace direct pool creation:
```rust
// Before
let pool = SqlitePool::connect(&url).await?;

// After (preferred)
use casparian_db::DbConnection;

let conn = DbConnection::open_sqlite(Path::new(&path)).await?;
```

3. If you still need a sqlx pool (legacy):
```rust
use casparian_db::legacy::{create_pool, DbConfig, DbPool};

pub struct MyStorage {
    pool: DbPool,  // Legacy sqlx pool
}
```

---

## Error Handling

`DbConnection` returns `BackendError`. Legacy pool APIs return `legacy::DbError`.

```rust
pub enum BackendError {
    Database(String),
    Locked(String),
    ReadOnly,
    Query(String),
    Transaction(String),
    TypeConversion(String),
    NotAvailable(String),
    // backend-specific variants
}

pub enum DbError {
    Database(sqlx::Error),           // Connection/query errors
    License(LicenseError),           // License validation failed
    InvalidUrl(String),              // Unrecognized database URL
    NotCompiled(String, String),     // Feature not enabled
}

pub enum LicenseError {
    NotFound(String),                // License file missing
    InvalidFormat(String),           // JSON parse error
    Expired,                         // License expired
    FeatureNotLicensed(String),      // Tier doesn't include feature
    InvalidSignature,                // Signature verification failed
}
```

---

## File Structure

```
casparian_db/
├── CLAUDE.md           # This file
├── Cargo.toml          # Feature flags
└── src/
    ├── lib.rs          # DatabaseType, exports
    ├── backend.rs      # DbConnection, DbValue
    ├── license.rs      # License, LicenseTier
    └── pool.rs         # Legacy sqlx pool API
```

---

## Common Tasks

### Add a New Database Backend

1. Add feature flag to `Cargo.toml`:
```toml
[features]
mssql = ["tiberius"]
```

2. Add variant to `DatabaseType`:
```rust
#[cfg(feature = "mssql")]
Mssql,
```

3. Update `requires_license()`:
```rust
#[cfg(feature = "mssql")]
Self::Mssql => true,
```

4. Add `LicenseTier::allows()` check:
```rust
#[cfg(feature = "mssql")]
DatabaseType::Mssql => matches!(self, Self::Enterprise),
```

5. Add pool creation logic in `create_pool()`

6. Add tests

### Validate License Programmatically

```rust
use casparian_db::{License, DatabaseType, LicenseError};

let license = License::load(Path::new("license.json"))?;

// Check if license allows a specific database
match license.allows(DatabaseType::Postgres) {
    Ok(()) => println!("PostgreSQL allowed"),
    Err(LicenseError::FeatureNotLicensed(db)) => {
        println!("Upgrade to Professional for {} support", db);
    }
    Err(e) => return Err(e.into()),
}
```

---

## Testing

### Unit Tests

```rust
#[test]
fn test_community_license_sqlite_only() {
    let license = License::community();

    #[cfg(feature = "sqlite")]
    assert!(license.allows(DatabaseType::Sqlite).is_ok());

    #[cfg(feature = "postgres")]
    assert!(license.allows(DatabaseType::Postgres).is_err());
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_sqlite_connection() {
    let config = DbConfig::sqlite_memory();
    let pool = create_pool(config).await.unwrap();

    let row: (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(row.0, 1);
}
```

---

## Key Principles

1. **License check at connection time** - Not at query time
2. **Feature flags for compile-time gating** - Dead code elimination
3. **Graceful degradation** - Missing license = Community tier
4. **Single DbPool type** - Uses sqlx::AnyPool for flexibility
5. **Standard license locations** - Predictable, documented
