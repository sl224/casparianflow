# Phase 4: End-to-End Testing - IN PROGRESS

## Overview

Phase 4 validates the complete Rust Worker + Sentinel system working together with real ZMQ communication, database operations, and job processing.

---

## Test Infrastructure

### Created Files

```
tests/e2e/
├── schema.sql              # SQLite test database schema
├── test_plugin.py          # Minimal Python plugin for testing
└── run_e2e_test.sh        # E2E test orchestration script
```

### Test Flow

1. **Build** release binaries (sentinel + worker)
2. **Setup** SQLite database with test schema
3. **Insert** test job (QUEUED status)
4. **Start** Sentinel (ROUTER socket on tcp://127.0.0.1:15557)
5. **Start** Worker (DEALER socket connects to Sentinel)
6. **Verify** job lifecycle:
   - Worker sends IDENTIFY
   - Sentinel assigns job (DISPATCH)
   - Worker executes plugin
   - Worker writes Parquet output
   - Worker sends CONCLUDE
   - Sentinel marks job COMPLETED

---

## Current Blockers

### 1. Missing Database Lookups

The Sentinel's `assign_job()` currently uses placeholders:

```rust
// TODO: Load file path, env_hash, source_code from database
let cmd = DispatchCommand {
    file_path: "/placeholder/input.csv",  // ← Need real path
    env_hash: "placeholder_env_hash",      // ← Need real hash
    source_code: "# placeholder",          // ← Need real code
    // ...
};
```

**Needed**:
- Query `FileLocation` + `FileVersion` for actual file path
- Query `PluginManifest` for `source_code` and `env_hash`

### 2. Environment Provisioning

Worker checks if interpreter exists:

```rust
if !interpreter.exists() {
    bail!("Environment {} not provisioned", env_hash);
}
```

**Options**:
a. Create real venv with `uv` (complex for test)
b. Mock with system Python interpreter (simpler)
c. Skip venv check in test mode

### 3. Bridge Execution

Worker executes Python via bridge:

```rust
let batches = bridge::execute_bridge(config).await?;
```

**Requires**:
- Python interpreter
- Arrow/PyArrow installed
- Unix socket IPC between Rust & Python

---

## Solutions (Option A: Minimal Real Test)

### 1. Extend Database Schema

Add minimal tables to support lookups:

```sql
CREATE TABLE cf_file_location (
    id INTEGER PRIMARY KEY,
    source_root_id INTEGER,
    rel_path TEXT,
    filename TEXT
);

CREATE TABLE cf_file_version (
    id INTEGER PRIMARY KEY,
    location_id INTEGER,
    content_hash TEXT
);

CREATE TABLE cf_plugin_manifest (
    id INTEGER PRIMARY KEY,
    plugin_name TEXT,
    source_code TEXT,
    env_hash TEXT,
    status TEXT
);
```

### 2. Implement Sentinel Database Queries

```rust
async fn assign_job(...) {
    // Load file path
    let file_version: FileVersion = sqlx::query_as(
        "SELECT * FROM cf_file_version WHERE id = ?"
    ).bind(job.file_version_id).fetch_one(&pool).await?;

    let file_location: FileLocation = sqlx::query_as(
        "SELECT * FROM cf_file_location WHERE id = ?"
    ).bind(file_version.location_id).fetch_one(&pool).await?;

    // Load plugin manifest
    let manifest: PluginManifest = sqlx::query_as(
        "SELECT * FROM cf_plugin_manifest
         WHERE plugin_name = ? AND status = 'ACTIVE'"
    ).bind(&job.plugin_name).fetch_one(&pool).await?;

    let cmd = DispatchCommand {
        file_path: format!("{}/{}", source_root.path, file_location.rel_path),
        env_hash: manifest.env_hash,
        source_code: manifest.source_code,
        // ...
    };
}
```

### 3. Mock Environment for Testing

```rust
// In test mode, use system Python
let interpreter = if cfg!(test) {
    PathBuf::from("/usr/bin/python3")
} else {
    mgr.interpreter_path(&cmd.env_hash)
};
```

---

## Solutions (Option B: Mock Test)

Skip real execution, just test message flow:

1. **Sentinel** dispatches DISPATCH message
2. **Worker** receives it, validates format
3. **Worker** sends mock CONCLUDE (success)
4. **Sentinel** marks job complete

**Pros**: Simple, fast, no Python dependency
**Cons**: Doesn't test actual plugin execution

---

## Next Steps

**Recommended**: Option A (Minimal Real Test)

1. ✅ Create E2E test infrastructure (done)
2. ⏳ Extend database schema with FileLocation, FileVersion, PluginManifest
3. ⏳ Implement Sentinel database lookups in `assign_job()`
4. ⏳ Create test data (file, manifest) in schema.sql
5. ⏳ Create minimal venv or use system Python
6. ⏳ Run E2E test and verify full cycle

---

**Status**: Phase 4 infrastructure ready, implementing database queries next.
