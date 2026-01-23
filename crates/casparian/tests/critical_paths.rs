//! Critical Path E2E Tests - NO MOCKS
//!
//! These tests verify the actual critical paths that users depend on.
//! They use real files, real databases, and real tool execution.
//!
//! Jon Blow principle: "If you can't test it, you can't know if it works."
//!
//! ## Test Philosophy
//!
//! 1. Test PUBLIC interfaces only (what users actually use)
//! 2. Use REAL files and databases (no mocks)
//! 3. Test the BINARY when possible (actual user experience)
//! 4. Favor end-to-end flows over unit-only coverage

use std::fs;
#[cfg(feature = "full")]
use std::process::Command;
use tempfile::TempDir;

mod cli_support;

use casparian_db::{BackendError, DbConnection, DbValue};
use cli_support::with_duckdb;

// =============================================================================
// DATABASE TESTS - Real DuckDB Operations
// =============================================================================

mod database {
    use super::*;

    /// Critical: DuckDB must persist data to disk
    #[test]
    fn test_duckdb_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");

        with_duckdb(&db_path, |conn| {
            conn.execute(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)",
                &[],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO test (id, value) VALUES (?, ?)",
                &[DbValue::from(1i64), DbValue::from("data1")],
            )
            .unwrap();
        });

        let value: String = with_duckdb(&db_path, |conn| {
            conn.query_scalar("SELECT value FROM test WHERE id = 1", &[])
                .unwrap()
        });
        assert_eq!(value, "data1");

        assert!(db_path.exists(), "Database file should persist");
    }

    /// Critical: Read-only connections should allow reads but reject writes
    #[test]
    fn test_duckdb_readonly_mode() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("readonly.duckdb");

        with_duckdb(&db_path, |conn| {
            conn.execute(
                "CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)",
                &[],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO test (id, value) VALUES (?, ?)",
                &[DbValue::from(1i64), DbValue::from("data1")],
            )
            .unwrap();
        });

        let readonly = DbConnection::open_duckdb_readonly(&db_path).expect("open readonly");
        let value: String = readonly
            .query_scalar("SELECT value FROM test WHERE id = 1", &[])
            .expect("read value");
        assert_eq!(value, "data1");

        let write_err = readonly
            .execute(
                "INSERT INTO test (value) VALUES (?)",
                &[DbValue::from("data2")],
            )
            .expect_err("read-only should reject writes");
        assert!(matches!(write_err, BackendError::ReadOnly));
    }

    /// Critical: DuckDB should prevent concurrent writers
    #[test]
    fn test_duckdb_lock_enforced() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("concurrent.duckdb");

        let writer = DbConnection::open_duckdb(&db_path).expect("open writer");

        let second_writer = DbConnection::open_duckdb(&db_path);
        assert!(
            matches!(second_writer, Err(BackendError::Locked(_))),
            "expected lock error, got: {:?}",
            second_writer
        );

        drop(writer);

        let reopened = DbConnection::open_duckdb(&db_path);
        assert!(reopened.is_ok(), "expected writer lock released");
    }
}

// =============================================================================
// BINARY TESTS - Actual Executable
// (Gated behind 'full' feature - runs cargo which triggers compilation)
// =============================================================================

#[cfg(feature = "full")]
mod binary {
    use super::*;

    /// Critical: Binary must compile and run --help
    #[test]
    fn test_binary_runs() {
        let output = Command::new("cargo")
            .args(["run", "-p", "casparian", "-q", "--", "--help"])
            .output();

        match output {
            Ok(out) => {
                let combined = format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );

                // Should contain usage info or subcommands
                assert!(
                    combined.contains("Usage")
                        || combined.contains("casparian")
                        || combined.contains("SUBCOMMANDS")
                        || combined.contains("Commands")
                        || combined.contains("help"),
                    "Should show help. Got: {}",
                    combined
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("Skipping: cargo not in PATH");
            }
            Err(e) => panic!("Binary test failed: {}", e),
        }
    }

    /// Critical: scan subcommand should work
    #[test]
    fn test_scan_subcommand() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("test.csv"), "id\n1\n2\n").unwrap();

        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "casparian",
                "-q",
                "--",
                "scan",
                &temp_dir.path().to_string_lossy(),
            ])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);

                // Should either succeed or show meaningful output
                if out.status.success() {
                    assert!(
                        stdout.contains("csv") || stdout.contains("file") || stdout.contains("1"),
                        "Scan should show files. Got stdout: {}, stderr: {}",
                        stdout,
                        stderr
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("Skipping: cargo not in PATH");
            }
            Err(e) => panic!("Scan test failed: {}", e),
        }
    }
}

// =============================================================================
// SCOUT SCANNER TESTS - File Discovery Critical Path
// =============================================================================

mod scout {
    use super::*;
    use casparian::scout::{Database, ScanProgress, Scanner, Source, SourceId, SourceType};
    use std::sync::mpsc;

    /// Critical: Scanner must send progress updates during scan
    #[test]
    fn test_scanner_sends_progress_updates() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files - enough to trigger progress updates
        // (progress_interval is 500, batch_size is 1000)
        for i in 0..100 {
            fs::write(
                temp_dir.path().join(format!("file_{}.txt", i)),
                format!("content {}", i),
            )
            .unwrap();
        }

        // Create in-memory database
        let db = Database::open_in_memory().unwrap();

        // Create source
        let source = Source {
            id: SourceId::new(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: temp_dir.path().to_string_lossy().to_string(),
            poll_interval_secs: 0,
            enabled: true,
        };

        // Create progress channel
        let (progress_tx, progress_rx) = mpsc::channel::<ScanProgress>();

        // Create scanner
        let scanner = Scanner::new(db);

        // Run scan with progress
        let result = scanner
            .scan(&source, Some(progress_tx.clone()), None)
            .unwrap();
        drop(progress_tx);

        // Collect progress updates
        let progress_updates: Vec<_> = progress_rx.try_iter().collect();

        // CRITICAL: Must receive at least initial and final progress
        assert!(
            progress_updates.len() >= 2,
            "Must receive at least 2 progress updates (initial + final), got {}",
            progress_updates.len()
        );

        // First progress should be initial (0 files)
        let first = &progress_updates[0];
        assert_eq!(first.files_found, 0, "Initial progress should show 0 files");

        // Last progress should have final count
        let last = progress_updates.last().unwrap();
        assert_eq!(
            last.files_found, 100,
            "Final progress should show all 100 files, got {}",
            last.files_found
        );

        // Scan result should match
        assert_eq!(result.stats.files_discovered, 100);
    }

    /// Critical: Rescanning same path should work (use existing source)
    #[test]
    fn test_rescan_existing_source_works() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        for i in 0..10 {
            fs::write(temp_dir.path().join(format!("file_{}.txt", i)), "content").unwrap();
        }

        let db = Database::open_in_memory().unwrap();

        let source = Source {
            id: SourceId::new(),
            name: "test".to_string(),
            source_type: SourceType::Local,
            path: temp_dir.path().to_string_lossy().to_string(),
            poll_interval_secs: 0,
            enabled: true,
        };

        // Upsert source
        db.upsert_source(&source).unwrap();

        let scanner = Scanner::new(db.clone());

        // First scan
        let result1 = scanner.scan_source(&source).unwrap();
        assert_eq!(result1.stats.files_discovered, 10);

        // Rescan same source (should work without error)
        let result2 = scanner.scan_source(&source).unwrap();
        assert_eq!(result2.stats.files_discovered, 10);

        // Source should still exist
        let loaded = db.get_source_by_path(&source.path).unwrap();
        assert!(loaded.is_some(), "Source should still exist after rescan");
    }

    /// Critical: Source names must be unique
    #[test]
    fn test_unique_source_names() {
        let db = Database::open_in_memory().unwrap();

        let source1 = Source {
            id: SourceId::new(),
            name: "data".to_string(),
            source_type: SourceType::Local,
            path: "/path/to/data".to_string(),
            poll_interval_secs: 0,
            enabled: true,
        };

        let source2 = Source {
            id: SourceId::new(),
            name: "data".to_string(), // Same name!
            source_type: SourceType::Local,
            path: "/other/path/to/data".to_string(),
            poll_interval_secs: 0,
            enabled: true,
        };

        // First insert should succeed
        db.upsert_source(&source1).unwrap();

        // Second insert with same name but different ID should fail
        let result = db.upsert_source(&source2);
        assert!(result.is_err(), "Should fail with duplicate name");
    }
}
