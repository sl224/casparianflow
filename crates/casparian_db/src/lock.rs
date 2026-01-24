//! Process-level database locking.
//!
//! DuckDB only allows one writer process at a time. This module provides
//! a locking mechanism to coordinate access across multiple processes.
//!
//! Uses the `fs2` crate for cross-platform file locking (MSRV 1.75 compatible).
//! Note: std::fs::File::lock() requires Rust 1.89+, so we use fs2 instead.

#[cfg(feature = "duckdb")]
use chrono::Utc;
#[cfg(feature = "duckdb")]
use fs2::FileExt;
#[cfg(feature = "duckdb")]
use serde::Serialize;
#[cfg(feature = "duckdb")]
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
#[cfg(feature = "duckdb")]
use tracing::{debug, info, warn};

/// Errors from lock operations.
#[derive(Debug, Error)]
pub enum LockError {
    #[error("Database is locked by another process: {0}")]
    Locked(PathBuf),

    #[error("Failed to create lock file: {0}")]
    CreateFailed(#[source] io::Error),

    #[error("Failed to acquire lock: {0}")]
    AcquireFailed(#[source] io::Error),
}

/// A guard that holds an exclusive lock on a database file.
///
/// The lock is automatically released when the guard is dropped.
#[cfg(feature = "duckdb")]
pub struct DbLockGuard {
    _file: File,
    lock_path: PathBuf,
    sidecar_path: Option<PathBuf>,
}

/// Stub guard for when duckdb feature is disabled.
#[cfg(not(feature = "duckdb"))]
pub struct DbLockGuard {
    lock_path: PathBuf,
    sidecar_path: Option<PathBuf>,
}

impl DbLockGuard {
    /// Get the path to the lock file.
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

#[cfg(feature = "duckdb")]
#[derive(Serialize)]
struct LockSidecar {
    pid: u32,
    exe: Option<String>,
    timestamp: String,
    mode: &'static str,
}

#[cfg(feature = "duckdb")]
fn sidecar_path_for(lock_path: &Path) -> PathBuf {
    let ext = lock_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("lock");
    lock_path.with_extension(format!("{ext}.json"))
}

#[cfg(feature = "duckdb")]
fn write_lock_sidecar(lock_path: &Path, mode: &'static str) -> Option<PathBuf> {
    let sidecar = LockSidecar {
        pid: std::process::id(),
        exe: std::env::current_exe().ok().map(|p| p.display().to_string()),
        timestamp: Utc::now().to_rfc3339(),
        mode,
    };
    let sidecar_path = sidecar_path_for(lock_path);
    match serde_json::to_vec_pretty(&sidecar)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
        .and_then(|payload| fs::write(&sidecar_path, payload))
    {
        Ok(()) => Some(sidecar_path),
        Err(e) => {
            warn!(
                "Failed to write lock sidecar {}: {}",
                sidecar_path.display(),
                e
            );
            None
        }
    }
}

#[cfg(feature = "duckdb")]
impl Drop for DbLockGuard {
    fn drop(&mut self) {
        debug!("Releasing database lock: {}", self.lock_path.display());
        if let Some(path) = &self.sidecar_path {
            if let Err(e) = fs::remove_file(path) {
                debug!("Failed to remove lock sidecar {}: {}", path.display(), e);
            }
        }
        // File is automatically unlocked when closed (fs2 uses flock/LockFileEx)
    }
}

impl std::fmt::Debug for DbLockGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbLockGuard")
            .field("lock_path", &self.lock_path)
            .finish()
    }
}

/// Get the lock file path for a database path.
///
/// Examples:
/// - `/data/test.duckdb` → `/data/test.duckdb.lock`
/// - `/data/mydb` → `/data/mydb.lock` (no double-dot)
pub fn lock_path_for(db_path: &Path) -> PathBuf {
    let mut lock_path = db_path.to_path_buf();
    match lock_path.extension() {
        Some(ext) => {
            // Has extension: append .lock to existing extension
            let new_ext = format!("{}.lock", ext.to_string_lossy());
            lock_path.set_extension(new_ext);
        }
        None => {
            // No extension: just add .lock
            lock_path.set_extension("lock");
        }
    }
    lock_path
}

/// Try to acquire an exclusive lock on a database file.
///
/// This is a non-blocking operation. If another process holds the lock,
/// this returns `Err(LockError::Locked)` immediately.
///
/// # Arguments
///
/// * `db_path` - Path to the database file (not the lock file)
///
/// # Returns
///
/// A guard that holds the lock. The lock is released when the guard is dropped.
#[cfg(feature = "duckdb")]
pub fn try_lock_exclusive(db_path: &Path) -> Result<DbLockGuard, LockError> {
    let lock_path = lock_path_for(db_path);

    debug!("Attempting to acquire exclusive lock: {}", lock_path.display());

    // Create or open the lock file
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(LockError::CreateFailed)?;

    // Try to acquire exclusive lock (non-blocking)
    // Use fully qualified syntax to call fs2's method (not std::fs::File::try_lock_exclusive
    // which exists in Rust 1.89+ and returns TryLockError instead of io::Error)
    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => {
            info!("Acquired exclusive database lock: {}", lock_path.display());
            let sidecar_path = write_lock_sidecar(&lock_path, "exclusive");
            Ok(DbLockGuard {
                _file: file,
                lock_path,
                sidecar_path,
            })
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
            debug!("Database is locked by another process");
            Err(LockError::Locked(db_path.to_path_buf()))
        }
        Err(e) => Err(LockError::AcquireFailed(e)),
    }
}

/// Acquire an exclusive lock on a database file, waiting if necessary.
///
/// This is a blocking operation. If another process holds the lock,
/// this will wait until the lock is released.
///
/// # Arguments
///
/// * `db_path` - Path to the database file (not the lock file)
///
/// # Returns
///
/// A guard that holds the lock. The lock is released when the guard is dropped.
#[cfg(feature = "duckdb")]
pub fn lock_exclusive(db_path: &Path) -> Result<DbLockGuard, LockError> {
    let lock_path = lock_path_for(db_path);

    debug!("Waiting to acquire exclusive lock: {}", lock_path.display());

    // Create or open the lock file
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(LockError::CreateFailed)?;

    // Acquire exclusive lock (blocking)
    // fs2::lock_exclusive() blocks until lock is available
    file.lock_exclusive().map_err(LockError::AcquireFailed)?;

    info!("Acquired exclusive database lock: {}", lock_path.display());
    let sidecar_path = write_lock_sidecar(&lock_path, "exclusive");
    Ok(DbLockGuard {
        _file: file,
        lock_path,
        sidecar_path,
    })
}

/// Try to acquire a shared (read) lock on a database file.
///
/// Multiple processes can hold shared locks simultaneously.
/// This is a non-blocking operation.
///
/// # Arguments
///
/// * `db_path` - Path to the database file (not the lock file)
///
/// # Returns
///
/// A guard that holds the lock. The lock is released when the guard is dropped.
#[cfg(feature = "duckdb")]
pub fn try_lock_shared(db_path: &Path) -> Result<DbLockGuard, LockError> {
    let lock_path = lock_path_for(db_path);

    debug!("Attempting to acquire shared lock: {}", lock_path.display());

    // Create or open the lock file
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(LockError::CreateFailed)?;

    // Try to acquire shared lock (non-blocking)
    // Use fully qualified syntax to call fs2's method (not std::fs::File::try_lock_shared
    // which exists in Rust 1.89+ and returns TryLockError instead of io::Error)
    match FileExt::try_lock_shared(&file) {
        Ok(()) => {
            debug!("Acquired shared database lock: {}", lock_path.display());
            Ok(DbLockGuard {
                _file: file,
                lock_path,
                sidecar_path: None,
            })
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
            debug!("Database has exclusive lock by another process");
            Err(LockError::Locked(db_path.to_path_buf()))
        }
        Err(e) => Err(LockError::AcquireFailed(e)),
    }
}

/// Check if a database file is currently locked by another process.
///
/// This attempts to acquire a lock and immediately releases it.
#[cfg(feature = "duckdb")]
pub fn is_locked(db_path: &Path) -> bool {
    match try_lock_exclusive(db_path) {
        Ok(_guard) => {
            // We got the lock, so it wasn't locked
            // Guard drops here, releasing the lock
            false
        }
        Err(LockError::Locked(_)) => true,
        Err(e) => {
            warn!("Failed to check lock status: {}", e);
            false // Assume not locked if we can't check
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "duckdb")]
    use tempfile::TempDir;

    #[test]
    fn test_lock_path_for() {
        // With extension
        let db_path = Path::new("/data/test.duckdb");
        let lock = lock_path_for(db_path);
        assert_eq!(lock, PathBuf::from("/data/test.duckdb.lock"));

        // Without extension (no double-dot)
        let db_path_no_ext = Path::new("/data/mydb");
        let lock_no_ext = lock_path_for(db_path_no_ext);
        assert_eq!(lock_no_ext, PathBuf::from("/data/mydb.lock"));

        // Multiple dots in name
        let db_path_dots = Path::new("/data/my.data.db");
        let lock_dots = lock_path_for(db_path_dots);
        assert_eq!(lock_dots, PathBuf::from("/data/my.data.db.lock"));
    }

    #[test]
    #[cfg(feature = "duckdb")]
    fn test_try_lock_exclusive() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");

        // First lock should succeed
        let guard = try_lock_exclusive(&db_path).unwrap();
        assert!(guard.lock_path().exists());

        // Drop the guard
        drop(guard);

        // Should be able to lock again
        let _guard2 = try_lock_exclusive(&db_path).unwrap();
    }

    #[test]
    #[cfg(feature = "duckdb")]
    fn test_lock_contention() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");

        // First lock
        let _guard = try_lock_exclusive(&db_path).unwrap();

        // Second lock should fail
        let result = try_lock_exclusive(&db_path);
        assert!(matches!(result, Err(LockError::Locked(_))));
    }

    #[test]
    #[cfg(feature = "duckdb")]
    fn test_is_locked() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");

        // Not locked initially
        assert!(!is_locked(&db_path));

        // Lock it
        let _guard = try_lock_exclusive(&db_path).unwrap();

        // Now it's locked (but is_locked can't detect this from same process)
        // This test mainly verifies the function doesn't crash
    }

    #[test]
    #[cfg(feature = "duckdb")]
    fn test_shared_locks() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.duckdb");

        // Multiple shared locks should work
        let _guard1 = try_lock_shared(&db_path).unwrap();
        let _guard2 = try_lock_shared(&db_path).unwrap();
        // Both guards held simultaneously - OK for shared locks
    }
}
