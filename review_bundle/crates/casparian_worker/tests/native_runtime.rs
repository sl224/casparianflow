use casparian_protocol::JobId;
use casparian_worker::native_runtime::NativeSubprocessRuntime;
use casparian_worker::runtime::{PluginRuntime, RunContext};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn write_script(dir: &TempDir, name: &str, body: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, body).expect("failed to write script");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("set perms");
    path
}

#[cfg(unix)]
fn base_context(entrypoint: String) -> RunContext {
    let mut schema_hashes = HashMap::new();
    schema_hashes.insert("events".to_string(), "expected_hash".to_string());
    RunContext {
        job_id: JobId::new(1),
        file_id: 42,
        entrypoint,
        env_hash: None,
        source_code: None,
        schema_hashes,
    }
}

#[cfg(unix)]
#[test]
fn test_native_missing_hello_errors() {
    let temp = TempDir::new().unwrap();
    let script = write_script(
        &temp,
        "missing_hello.sh",
        r#"#!/bin/sh
echo '{"type":"output_begin","output":"events","schema_hash":"expected_hash","stream_index":0}' 1>&2
exit 0
"#,
    );

    let runtime = NativeSubprocessRuntime::new();
    let ctx = base_context(script.to_string_lossy().to_string());
    let input_path = temp.path().join("input.txt");
    fs::write(&input_path, "data").unwrap();

    let err = match runtime.run_file(&ctx, &input_path) {
        Ok(_) => panic!("expected error"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("hello frame"),
        "unexpected error: {}",
        err
    );
}

#[cfg(unix)]
#[test]
fn test_native_non_arrow_stdout_errors() {
    let temp = TempDir::new().unwrap();
    let script = write_script(
        &temp,
        "non_arrow.sh",
        r#"#!/bin/sh
echo '{"type":"hello","protocol":"0.1","parser_id":"test","parser_version":"0.1.0","capabilities":{}}' 1>&2
echo '{"type":"output_begin","output":"events","schema_hash":"expected_hash","stream_index":0}' 1>&2
echo "not arrow" 1>&1
echo '{"type":"output_end","output":"events","rows_emitted":0,"stream_index":0}' 1>&2
exit 0
"#,
    );

    let runtime = NativeSubprocessRuntime::new();
    let ctx = base_context(script.to_string_lossy().to_string());
    let input_path = temp.path().join("input.txt");
    fs::write(&input_path, "data").unwrap();

    let err = match runtime.run_file(&ctx, &input_path) {
        Ok(_) => panic!("expected error"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("Arrow stream") || err.to_string().contains("Arrow IPC"),
        "unexpected error: {}",
        err
    );
}

#[cfg(unix)]
#[test]
fn test_native_schema_hash_mismatch_errors() {
    let temp = TempDir::new().unwrap();
    let script = write_script(
        &temp,
        "schema_mismatch.sh",
        r#"#!/bin/sh
echo '{"type":"hello","protocol":"0.1","parser_id":"test","parser_version":"0.1.0","capabilities":{}}' 1>&2
echo '{"type":"output_begin","output":"events","schema_hash":"wrong_hash","stream_index":0}' 1>&2
exit 0
"#,
    );

    let runtime = NativeSubprocessRuntime::new();
    let ctx = base_context(script.to_string_lossy().to_string());
    let input_path = temp.path().join("input.txt");
    fs::write(&input_path, "data").unwrap();

    let err = match runtime.run_file(&ctx, &input_path) {
        Ok(_) => panic!("expected error"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("Schema hash mismatch"),
        "unexpected error: {}",
        err
    );
}
