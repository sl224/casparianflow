use casparian_protocol::idempotency::schema_hash;
use casparian_protocol::types::SchemaDefinition;
use casparian_worker::native_runtime::NativeSubprocessRuntime;
use casparian_worker::runtime::{PluginRuntime, RunContext};
use casparian_protocol::JobId;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
#[test]
fn test_evtx_native_subprocess_e2e() {
    let sample_path = match resolve_sample_path() {
        Some(path) => path,
        None => {
            eprintln!(
                "Skipping EVTX native e2e test: set EVTX_SAMPLE_PATH or add tests/fixtures/evtx/sample.evtx"
            );
            return;
        }
    };

    let plugin_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../parsers/evtx_native");
    let schema_dir = plugin_dir.join("schemas");

    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&plugin_dir)
        .status()
        .expect("Failed to build evtx_native plugin");
    assert!(status.success(), "evtx_native build failed");

    let binary_path = plugin_dir.join("target/release/evtx_native");
    assert!(binary_path.exists(), "evtx_native binary missing");

    let target_schema_dir = plugin_dir.join("target/schemas");
    copy_schema_dir(&schema_dir, &target_schema_dir);

    let schema_hashes = load_schema_hashes(&schema_dir);
    let ctx = RunContext {
        job_id: JobId::new(1),
        file_id: 1,
        entrypoint: binary_path.to_string_lossy().to_string(),
        env_hash: None,
        source_code: None,
        schema_hashes,
    };

    let runtime = NativeSubprocessRuntime::new();
    let outputs = runtime
        .run_file(&ctx, &sample_path)
        .expect("native runtime failed");

    let mut output_names = outputs
        .output_info
        .iter()
        .map(|info| info.name.as_str())
        .collect::<Vec<_>>();
    output_names.sort();

    let mut expected = vec![
        "evtx_events",
        "evtx_eventdata_kv",
        "evtx_userdata_kv",
        "evtx_record_annotations",
        "evtx_files",
    ];
    expected.sort();

    assert_eq!(output_names, expected, "unexpected output set");
}

#[cfg(unix)]
fn resolve_sample_path() -> Option<PathBuf> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/evtx/sample.evtx");
    if fixture.exists() {
        return Some(fixture);
    }

    std::env::var("EVTX_SAMPLE_PATH")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.exists())
}

#[cfg(unix)]
fn load_schema_hashes(schema_dir: &Path) -> HashMap<String, String> {
    let mut hashes = HashMap::new();
    let entries = fs::read_dir(schema_dir).expect("schema dir read failed");
    for entry in entries {
        let entry = entry.expect("schema entry read failed");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("schema filename invalid");
        if !file_name.ends_with(".schema.json") {
            continue;
        }
        let output_name = file_name.trim_end_matches(".schema.json");
        let content = fs::read_to_string(&path).expect("schema read failed");
        let schema_def: SchemaDefinition =
            serde_json::from_str(&content).expect("schema json invalid");
        let hash = schema_hash(Some(&schema_def)).expect("schema hash failed");
        hashes.insert(output_name.to_string(), hash);
    }
    hashes
}

#[cfg(unix)]
fn copy_schema_dir(src: &Path, dst: &Path) {
    if dst.exists() {
        return;
    }
    fs::create_dir_all(dst).expect("schema dir create failed");
    for entry in fs::read_dir(src).expect("schema source read failed") {
        let entry = entry.expect("schema entry read failed");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let target = dst.join(file_name);
        fs::copy(&path, &target).expect("schema copy failed");
    }
}
