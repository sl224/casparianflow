#![allow(dead_code)]

use casparian::scout::Database;
use casparian_db::DbConnection;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn casparian_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_casparian"))
}

pub fn run_cli(args: &[String], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(casparian_bin());
    cmd.args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().expect("failed to execute casparian CLI")
}

pub fn assert_cli_success(output: &Output, args: &[String]) {
    assert!(
        output.status.success(),
        "command failed: {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn run_cli_json_value(args: &[String], envs: &[(&str, &str)]) -> serde_json::Value {
    let output = run_cli(args, envs);
    assert_cli_success(&output, args);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout.find(|c| c == '{' || c == '[').unwrap_or_else(|| {
        panic!(
            "no JSON payload found in output\nstdout:\n{}\nstderr:\n{}",
            stdout,
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let json_text = &stdout[json_start..];
    let mut deserializer = serde_json::Deserializer::from_str(json_text);
    serde_json::Value::deserialize(&mut deserializer).unwrap_or_else(|err| {
        panic!(
            "failed to parse JSON output: {}\nstdout:\n{}\nstderr:\n{}",
            err,
            stdout,
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

pub fn run_cli_json<T: DeserializeOwned>(args: &[String], envs: &[(&str, &str)]) -> T {
    let value = run_cli_json_value(args, envs);
    serde_json::from_value(value).expect("failed to deserialize JSON output")
}

pub fn init_scout_schema(db_path: &Path) {
    let _db = Database::open(db_path).expect("initialize scout schema");
}

pub fn with_duckdb<F, T>(db_path: &Path, f: F) -> T
where
    F: FnOnce(DbConnection) -> T,
{
    let url = format!("duckdb:{}", db_path.display());
    let conn = DbConnection::open_from_url(&url).expect("open duckdb connection");
    f(conn)
}
