//! Schema version management for pre-v1 development.
//!
//! Pre-v1 has no data to preserve, so on version mismatch we simply
//! drop all known tables and let init_*_schema recreate them.

use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbValue};
use tracing::warn;

/// Current schema version. Increment when schema changes.
pub const SCHEMA_VERSION: i32 = 3;

/// Known tables that will be dropped on schema mismatch.
///
/// Order matters: tables with foreign keys should come before
/// the tables they reference.
const KNOWN_TABLES: &[&str] = &[
    // API tables (api_storage.rs)
    "cf_api_events",
    "cf_api_jobs",
    "cf_api_approvals",
    // Pipeline tables (storage/duckdb.rs)
    "cf_selection_specs",
    "cf_selection_snapshots",
    "cf_selection_snapshot_files",
    "cf_pipelines",
    "cf_pipeline_runs",
    // Schema tables (schema_storage)
    "schema_contracts",
    "schema_discovery_results",
    // UI session tables (tauri session_storage)
    "cf_sessions",
    // Queue tables (queue.rs)
    "cf_processing_queue",
    "cf_output_materializations",
    "cf_plugin_manifest",
    "cf_plugin_environment",
    "cf_topic_config",
    // Error handling tables (queue.rs)
    "cf_dead_letter",
    "cf_parser_health",
    "cf_quarantine",
    "cf_job_schema_mismatch",
    // Meta table (last, so version check fails if others exist without it)
    "cf_meta",
];

/// Known sequences that will be dropped on schema mismatch.
const KNOWN_SEQUENCES: &[&str] = &[
    "seq_cf_processing_queue",
    "seq_cf_plugin_manifest",
    "seq_cf_topic_config",
    "seq_cf_dead_letter",
    "seq_cf_quarantine",
    "seq_cf_job_schema_mismatch",
    "seq_cf_api_jobs",
    "seq_cf_api_events",
];

/// Ensure the database schema version matches the expected version.
///
/// If the version mismatches or cf_meta doesn't exist, drops all known
/// tables and recreates the cf_meta table with the current version.
///
/// Returns `true` if a reset occurred, `false` if schema was already current.
pub fn ensure_schema_version(conn: &DbConnection, expected_version: i32) -> Result<bool> {
    // Check if cf_meta exists and has the expected version
    let current_version = get_current_version(conn)?;

    match current_version {
        Some(v) if v == expected_version => {
            // Schema is current, nothing to do
            Ok(false)
        }
        Some(v) => {
            // Version mismatch - reset
            warn!(
                "Database schema reset (dev mode): version {} -> {}",
                v, expected_version
            );
            reset_schema(conn, expected_version)?;
            Ok(true)
        }
        None => {
            // cf_meta doesn't exist - check if other tables exist
            if has_any_known_tables(conn)? {
                // Old schema without versioning - reset
                warn!(
                    "Database schema reset (dev mode): unversioned -> {}",
                    expected_version
                );
                reset_schema(conn, expected_version)?;
                Ok(true)
            } else {
                // Fresh database - just create cf_meta
                create_meta_table(conn, expected_version)?;
                Ok(false)
            }
        }
    }
}

/// Get the current schema version from cf_meta, if it exists.
fn get_current_version(conn: &DbConnection) -> Result<Option<i32>> {
    // Check if cf_meta table exists
    let table_exists = conn
        .query_optional(
            "SELECT 1 FROM information_schema.tables WHERE table_name = 'cf_meta'",
            &[],
        )?
        .is_some();

    if !table_exists {
        return Ok(None);
    }

    // Get the schema version
    let row = conn.query_optional(
        "SELECT schema_version FROM cf_meta WHERE key = 'schema'",
        &[],
    )?;

    match row {
        Some(r) => {
            let version: i32 = r.get_by_name("schema_version")?;
            Ok(Some(version))
        }
        None => Ok(None),
    }
}

/// Check if any known tables exist (besides cf_meta).
fn has_any_known_tables(conn: &DbConnection) -> Result<bool> {
    for table in KNOWN_TABLES.iter().filter(|t| **t != "cf_meta") {
        let exists = conn
            .query_optional(
                "SELECT 1 FROM information_schema.tables WHERE table_name = ?",
                &[DbValue::from(*table)],
            )?
            .is_some();
        if exists {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Drop all known tables and sequences, then create cf_meta with the new version.
fn reset_schema(conn: &DbConnection, version: i32) -> Result<()> {
    // Drop tables in order (respects foreign key dependencies)
    for table in KNOWN_TABLES {
        let drop_sql = format!("DROP TABLE IF EXISTS {}", table);
        conn.execute(&drop_sql, &[])
            .with_context(|| format!("Failed to drop table {}", table))?;
    }

    // Drop sequences
    for seq in KNOWN_SEQUENCES {
        let drop_sql = format!("DROP SEQUENCE IF EXISTS {}", seq);
        conn.execute(&drop_sql, &[])
            .with_context(|| format!("Failed to drop sequence {}", seq))?;
    }

    // Create fresh cf_meta
    create_meta_table(conn, version)?;

    Ok(())
}

/// Create the cf_meta table with the given schema version.
fn create_meta_table(conn: &DbConnection, version: i32) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS cf_meta (
            key TEXT PRIMARY KEY,
            schema_version INTEGER NOT NULL,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )
    .context("Failed to create cf_meta table")?;

    // Use explicit timestamp in ON CONFLICT - DuckDB doesn't support CURRENT_TIMESTAMP there
    let now = casparian_db::DbTimestamp::now();
    conn.execute(
        r#"
        INSERT INTO cf_meta (key, schema_version, updated_at)
        VALUES ('schema', ?, ?)
        ON CONFLICT(key) DO UPDATE SET schema_version = excluded.schema_version, updated_at = excluded.updated_at
        "#,
        &[DbValue::from(version), DbValue::from(now)],
    )
    .context("Failed to set schema version")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_database_creates_meta() {
        let conn = DbConnection::open_duckdb_memory().unwrap();

        // Fresh database should not trigger reset
        let reset = ensure_schema_version(&conn, 1).unwrap();
        assert!(!reset);

        // cf_meta should exist with version 1
        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, Some(1));
    }

    #[test]
    fn test_matching_version_no_reset() {
        let conn = DbConnection::open_duckdb_memory().unwrap();

        // Initialize with version 1
        ensure_schema_version(&conn, 1).unwrap();

        // Create a test table
        conn.execute_batch("CREATE TABLE cf_processing_queue (id INTEGER PRIMARY KEY)")
            .unwrap();

        // Check again with same version - should not reset
        let reset = ensure_schema_version(&conn, 1).unwrap();
        assert!(!reset);

        // Table should still exist
        let exists = conn
            .query_optional(
                "SELECT 1 FROM information_schema.tables WHERE table_name = 'cf_processing_queue'",
                &[],
            )
            .unwrap()
            .is_some();
        assert!(exists);
    }

    #[test]
    fn test_version_mismatch_triggers_reset() {
        let conn = DbConnection::open_duckdb_memory().unwrap();

        // Initialize with version 1
        ensure_schema_version(&conn, 1).unwrap();

        // Create a test table
        conn.execute_batch("CREATE TABLE cf_processing_queue (id INTEGER PRIMARY KEY)")
            .unwrap();

        // Check with version 2 - should trigger reset
        let reset = ensure_schema_version(&conn, 2).unwrap();
        assert!(reset);

        // Table should be dropped
        let exists = conn
            .query_optional(
                "SELECT 1 FROM information_schema.tables WHERE table_name = 'cf_processing_queue'",
                &[],
            )
            .unwrap()
            .is_some();
        assert!(!exists);

        // cf_meta should have version 2
        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, Some(2));
    }

    #[test]
    fn test_unversioned_schema_triggers_reset() {
        let conn = DbConnection::open_duckdb_memory().unwrap();

        // Create tables without cf_meta (simulating old schema)
        conn.execute_batch("CREATE TABLE cf_processing_queue (id INTEGER PRIMARY KEY)")
            .unwrap();
        conn.execute_batch("CREATE TABLE cf_api_jobs (id INTEGER PRIMARY KEY)")
            .unwrap();

        // ensure_schema_version should detect this and reset
        let reset = ensure_schema_version(&conn, 1).unwrap();
        assert!(reset);

        // Old tables should be dropped
        let queue_exists = conn
            .query_optional(
                "SELECT 1 FROM information_schema.tables WHERE table_name = 'cf_processing_queue'",
                &[],
            )
            .unwrap()
            .is_some();
        assert!(!queue_exists);

        // cf_meta should exist with version 1
        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, Some(1));
    }

    #[test]
    fn test_schema_reset_then_init_tables() {
        let conn = DbConnection::open_duckdb_memory().unwrap();

        // Simulate old version
        create_meta_table(&conn, 1).unwrap();
        conn.execute_batch(
            r#"
            CREATE SEQUENCE seq_cf_processing_queue;
            CREATE TABLE cf_processing_queue (
                id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_processing_queue'),
                old_column TEXT
            );
            "#,
        )
        .unwrap();

        // Version mismatch should reset
        let reset = ensure_schema_version(&conn, 2).unwrap();
        assert!(reset);

        // Now we can create the new schema
        conn.execute_batch(
            r#"
            CREATE SEQUENCE seq_cf_processing_queue;
            CREATE TABLE cf_processing_queue (
                id BIGINT PRIMARY KEY DEFAULT nextval('seq_cf_processing_queue'),
                new_column TEXT NOT NULL
            );
            "#,
        )
        .unwrap();

        // New table should exist with new column
        let has_new_col = conn
            .query_optional(
                "SELECT 1 FROM information_schema.columns WHERE table_name = 'cf_processing_queue' AND column_name = 'new_column'",
                &[],
            )
            .unwrap()
            .is_some();
        assert!(has_new_col);
    }
}
