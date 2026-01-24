use casparian_protocol::types::SinkMode;
use casparian_sinks::DuckDbSink;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

const ROLE_ENV: &str = "CASPARIAN_SINK_LOCK_ROLE";
const DB_ENV: &str = "CASPARIAN_SINK_LOCK_DB";
const READY_ENV: &str = "CASPARIAN_SINK_LOCK_READY";
const RELEASE_ENV: &str = "CASPARIAN_SINK_LOCK_RELEASE";

fn maybe_run_child() {
    let role = match env::var(ROLE_ENV) {
        Ok(role) => role,
        Err(_) => return,
    };

    let db_path = PathBuf::from(env::var(DB_ENV).expect("child missing DB path"));
    match role.as_str() {
        "hold" => {
            let _sink = DuckDbSink::new(db_path, "records", SinkMode::Append, "job-1", "records")
                .expect("child failed to open sink");
            if let Ok(ready_path) = env::var(READY_ENV) {
                fs::write(ready_path, "ready").expect("child failed to write ready file");
            }
            let release_path = PathBuf::from(env::var(RELEASE_ENV).expect("child missing release"));
            let start = Instant::now();
            while !release_path.exists() {
                if start.elapsed() > Duration::from_secs(20) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            std::process::exit(0);
        }
        "try" => match DuckDbSink::new(db_path, "records", SinkMode::Append, "job-2", "records") {
            Ok(_) => std::process::exit(0),
            Err(err) => {
                let msg = err.to_string().to_lowercase();
                if msg.contains("lock") || msg.contains("locked") || msg.contains("in use") {
                    std::process::exit(2);
                }
                eprintln!("unexpected error: {}", err);
                std::process::exit(3);
            }
        },
        other => {
            eprintln!("unknown role: {other}");
            std::process::exit(4);
        }
    }
}

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
fn duckdb_sink_locking() {
    maybe_run_child();

    let temp = tempfile::TempDir::new().expect("tempdir");
    let db_path = temp.path().join("sink.duckdb");
    let ready_path = temp.path().join("ready");
    let release_path = temp.path().join("release");

    let exe = env::current_exe().expect("test executable");
    let mut holder = Command::new(&exe)
        .args(["--exact", "duckdb_sink_locking", "--nocapture"])
        .env(ROLE_ENV, "hold")
        .env(DB_ENV, &db_path)
        .env(READY_ENV, &ready_path)
        .env(RELEASE_ENV, &release_path)
        .spawn()
        .expect("failed to spawn lock holder");

    wait_for_file(&ready_path, Duration::from_secs(5), &mut holder);

    let try_output = Command::new(&exe)
        .args(["--exact", "duckdb_sink_locking", "--nocapture"])
        .env(ROLE_ENV, "try")
        .env(DB_ENV, &db_path)
        .output()
        .expect("failed to spawn lock contender");

    assert_eq!(
        try_output.status.code(),
        Some(2),
        "expected lock contention; stdout: {} stderr: {}",
        String::from_utf8_lossy(&try_output.stdout),
        String::from_utf8_lossy(&try_output.stderr)
    );

    fs::write(&release_path, "release").expect("failed to release lock holder");
    wait_for_exit(&mut holder, Duration::from_secs(5));
}
