#[cfg(feature = "duckdb")]
use casparian_db::backend::{BackendError, DbConnection};
#[cfg(feature = "duckdb")]
use std::env;
#[cfg(feature = "duckdb")]
use std::fs;
#[cfg(feature = "duckdb")]
use std::path::{Path, PathBuf};
#[cfg(feature = "duckdb")]
use std::process::{Child, Command};
#[cfg(feature = "duckdb")]
use std::thread;
#[cfg(feature = "duckdb")]
use std::time::{Duration, Instant};

#[cfg(feature = "duckdb")]
const ROLE_ENV: &str = "CASPARIAN_DB_LOCK_ROLE";
#[cfg(feature = "duckdb")]
const DB_ENV: &str = "CASPARIAN_DB_LOCK_DB";
#[cfg(feature = "duckdb")]
const READY_ENV: &str = "CASPARIAN_DB_LOCK_READY";
#[cfg(feature = "duckdb")]
const RELEASE_ENV: &str = "CASPARIAN_DB_LOCK_RELEASE";

#[cfg(feature = "duckdb")]
fn maybe_run_child() {
    let role = match env::var(ROLE_ENV) {
        Ok(role) => role,
        Err(_) => return,
    };

    let db_path = PathBuf::from(env::var(DB_ENV).expect("child missing DB path"));
    match role.as_str() {
        "hold" => {
            let _conn = DbConnection::open_duckdb(&db_path)
                .expect("child failed to open DB with exclusive lock");
            if let Ok(ready_path) = env::var(READY_ENV) {
                fs::write(ready_path, "ready").expect("child failed to write ready file");
            }
            let release_path =
                PathBuf::from(env::var(RELEASE_ENV).expect("child missing release path"));
            let start = Instant::now();
            while !release_path.exists() {
                if start.elapsed() > Duration::from_secs(20) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            std::process::exit(0);
        }
        "rw" => {
            let result = DbConnection::open_duckdb(&db_path);
            exit_with_result(result);
        }
        "ro" => {
            let result = DbConnection::open_duckdb_readonly(&db_path);
            match result {
                Ok(_) => std::process::exit(0),
                Err(err) if is_lockish_error(&err) => std::process::exit(2),
                Err(err) => {
                    eprintln!("Unexpected read-only error: {err}");
                    std::process::exit(3);
                }
            }
        }
        other => {
            eprintln!("Unknown role: {other}");
            std::process::exit(4);
        }
    }
}

#[cfg(feature = "duckdb")]
fn exit_with_result(result: Result<DbConnection, BackendError>) -> ! {
    match result {
        Ok(_) => std::process::exit(0),
        Err(BackendError::Locked(_)) => std::process::exit(2),
        Err(err) => {
            eprintln!("Unexpected error: {err}");
            std::process::exit(3);
        }
    }
}

#[cfg(feature = "duckdb")]
fn is_lockish_error(err: &BackendError) -> bool {
    match err {
        BackendError::Locked(_) => true,
        BackendError::Database(message) => is_lockish_message(message),
        BackendError::DuckDb(inner) => is_lockish_message(&inner.to_string()),
        _ => false,
    }
}

#[cfg(feature = "duckdb")]
fn is_lockish_message(message: &str) -> bool {
    let msg = message.to_lowercase();
    msg.contains("lock") || msg.contains("locked") || msg.contains("busy") || msg.contains("in use")
}

#[cfg(feature = "duckdb")]
fn wait_for_file(path: &Path, timeout: Duration, child: &mut Child) {
    let start = Instant::now();
    while !path.exists() {
        if let Some(status) = child.try_wait().expect("failed to poll child") {
            panic!("lock holder exited early: {status}");
        }
        if start.elapsed() > timeout {
            panic!("timed out waiting for {}", path.display());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(feature = "duckdb")]
fn wait_for_exit(child: &mut Child, timeout: Duration) {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("failed to poll child") {
            if !status.success() {
                panic!("lock holder exited with failure: {status}");
            }
            break;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            panic!("timed out waiting for lock holder to exit");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[cfg(feature = "duckdb")]
fn duckdb_locking_behavior() {
    maybe_run_child();

    let temp_dir = tempfile::TempDir::new().expect("tempdir");
    let db_path = temp_dir.path().join("test.duckdb");
    let ready_path = temp_dir.path().join("ready");
    let release_path = temp_dir.path().join("release");

    let exe = env::current_exe().expect("test executable");

    let mut holder = Command::new(&exe)
        .args(["--exact", "duckdb_locking_behavior", "--nocapture"])
        .env(ROLE_ENV, "hold")
        .env(DB_ENV, &db_path)
        .env(READY_ENV, &ready_path)
        .env(RELEASE_ENV, &release_path)
        .spawn()
        .expect("failed to spawn lock holder");

    wait_for_file(&ready_path, Duration::from_secs(5), &mut holder);

    let rw_output = Command::new(&exe)
        .args(["--exact", "duckdb_locking_behavior", "--nocapture"])
        .env(ROLE_ENV, "rw")
        .env(DB_ENV, &db_path)
        .output()
        .expect("failed to spawn rw checker");
    assert_eq!(
        rw_output.status.code(),
        Some(2),
        "expected RW open to fail with Locked; stdout: {} stderr: {}",
        String::from_utf8_lossy(&rw_output.stdout),
        String::from_utf8_lossy(&rw_output.stderr)
    );

    let ro_output = Command::new(&exe)
        .args(["--exact", "duckdb_locking_behavior", "--nocapture"])
        .env(ROLE_ENV, "ro")
        .env(DB_ENV, &db_path)
        .output()
        .expect("failed to spawn ro checker");
    match ro_output.status.code() {
        Some(0) => {}
        Some(2) => {
            eprintln!(
                "DuckDB read-only open blocked while RW lock held; clients must use Control API for reads."
            );
        }
        other => {
            panic!(
                "unexpected RO result: {other:?}; stdout: {} stderr: {}",
                String::from_utf8_lossy(&ro_output.stdout),
                String::from_utf8_lossy(&ro_output.stderr)
            );
        }
    }

    fs::write(&release_path, "release").expect("failed to release lock holder");
    wait_for_exit(&mut holder, Duration::from_secs(5));
}
