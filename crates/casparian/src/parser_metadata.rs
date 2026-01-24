use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Parser metadata extracted without executing plugin code.
#[derive(Debug, Clone, Default)]
pub struct ParserMetadata {
    pub name: String,
    pub version: Option<String>,
    pub topics: Vec<String>,
    pub has_transform: bool,
    pub has_parse: bool,
}

/// Python script for extracting parser metadata via AST (no execution).
/// Batch mode: reads JSON array of paths from stdin, outputs JSON keyed by path.
const METADATA_EXTRACTOR_SCRIPT: &str = r#"
import ast
import json
import sys
import os

def extract_metadata(path):
    try:
        source = open(path).read()
        tree = ast.parse(source)
    except SyntaxError as e:
        return {"error": f"Syntax error: {e}"}
    except Exception as e:
        return {"error": str(e)}

    result = {
        "name": None,
        "version": None,
        "topics": [],
        "has_transform": False,
        "has_parse": False,
    }

    for node in ast.walk(tree):
        if isinstance(node, ast.ClassDef):
            for item in node.body:
                if isinstance(item, ast.Assign):
                    for target in item.targets:
                        if isinstance(target, ast.Name):
                            try:
                                value = ast.literal_eval(item.value)
                                if target.id == "name":
                                    result["name"] = value
                                elif target.id == "version":
                                    result["version"] = value
                                elif target.id == "topics":
                                    result["topics"] = value if isinstance(value, list) else [value]
                            except Exception:
                                pass
                elif isinstance(item, ast.FunctionDef):
                    if item.name == "transform":
                        result["has_transform"] = True
                    elif item.name == "parse":
                        result["has_parse"] = True

        elif isinstance(node, ast.FunctionDef) and node.name == "parse":
            result["has_parse"] = True

    if result["name"] is None:
        result["name"] = os.path.splitext(os.path.basename(path))[0]

    return result

if __name__ == "__main__":
    paths = json.load(sys.stdin)
    results = {}
    for path in paths:
        results[path] = extract_metadata(path)
    print(json.dumps(results))
"#;

/// Extract metadata for a batch of parser files.
/// Uses Python AST when available, falls back to regex parsing per file.
pub fn extract_metadata_batch(paths: &[PathBuf]) -> HashMap<String, ParserMetadata> {
    let mut results = HashMap::new();
    if paths.is_empty() {
        return results;
    }

    let path_strings: Vec<String> = paths
        .iter()
        .filter_map(|p| p.to_str().map(|s| s.to_string()))
        .collect();

    if path_strings.is_empty() {
        return results;
    }

    let json_input = serde_json::to_string(&path_strings).ok();
    let python_output = json_input.as_deref().and_then(run_python_extractor);
    let parsed = python_output
        .as_deref()
        .and_then(|output| match serde_json::from_str::<Value>(output) {
            Ok(value) => Some(value),
            Err(err) => {
                record_python_extractor_error(format!(
                    "Python metadata extractor returned invalid JSON: {}",
                    err
                ));
                None
            }
        });

    for path_str in &path_strings {
        let path = PathBuf::from(path_str);
        let fallback = fallback_metadata(&path);
        let meta = parsed
            .as_ref()
            .and_then(|value| value.get(path_str))
            .map(|value| merge_metadata(value, &path, &fallback))
            .unwrap_or(fallback);
        results.insert(path_str.clone(), meta);
    }

    results
}

fn run_python_extractor(input: &str) -> Option<String> {
    let mut last_error: Option<String> = None;
    for candidate in ["python3", "python"] {
        let mut child = match Command::new(candidate)
            .arg("-c")
            .arg(METADATA_EXTRACTOR_SCRIPT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                last_error = Some(format!("Failed to spawn {}: {}", candidate, err));
                continue;
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(input.as_bytes()).is_err() {
                last_error = Some(format!("Failed to write input to {}", candidate));
                continue;
            }
        }

        let output = match child.wait_with_output() {
            Ok(output) => output,
            Err(err) => {
                last_error = Some(format!("Failed to read output from {}: {}", candidate, err));
                continue;
            }
        };
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            if stderr.is_empty() {
                last_error = Some(format!(
                    "{} exited with status {}",
                    candidate, output.status
                ));
            } else {
                last_error = Some(format!(
                    "{} exited with status {}: {}",
                    candidate, output.status, stderr
                ));
            }
            continue;
        }

        match String::from_utf8(output.stdout) {
            Ok(stdout) => return Some(stdout),
            Err(err) => {
                last_error = Some(format!("{} returned non-UTF8 output: {}", candidate, err));
                continue;
            }
        }
    }
    if let Some(err) = last_error {
        record_python_extractor_error(err);
    }
    None
}

fn record_python_extractor_error(message: String) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    static LAST_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

    let store = LAST_ERROR.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = store.lock() {
        *guard = Some(message.clone());
    }

    if !WARNED.swap(true, Ordering::SeqCst) {
        eprintln!(
            "Warning: Python metadata extraction failed: {}. Falling back to regex parsing.",
            message
        );
    }
}

fn merge_metadata(value: &Value, path: &Path, fallback: &ParserMetadata) -> ParserMetadata {
    if value.get("error").is_some() {
        return fallback.clone();
    }

    let mut meta = fallback.clone();
    if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
        if !name.is_empty() {
            meta.name = name.to_string();
        }
    }
    if let Some(version) = value.get("version").and_then(|v| v.as_str()) {
        if !version.is_empty() {
            meta.version = Some(version.to_string());
        }
    }
    if let Some(topics) = value.get("topics").and_then(|v| v.as_array()) {
        meta.topics = topics
            .iter()
            .filter_map(|t| t.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(has_transform) = value.get("has_transform").and_then(|v| v.as_bool()) {
        meta.has_transform = has_transform;
    }
    if let Some(has_parse) = value.get("has_parse").and_then(|v| v.as_bool()) {
        meta.has_parse = has_parse;
    }

    if meta.name.is_empty() {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            meta.name = stem.to_string();
        }
    }

    meta
}

fn fallback_metadata(path: &Path) -> ParserMetadata {
    let mut meta = ParserMetadata::default();
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        meta.name = stem.to_string();
    }

    if let Ok(content) = fs::read_to_string(path) {
        if let Some(name) = extract_attribute(&content, "name") {
            meta.name = name;
        }
        if let Some(version) = extract_attribute(&content, "version") {
            meta.version = Some(version);
        }
    }

    meta
}

pub(crate) fn extract_attribute(content: &str, attr: &str) -> Option<String> {
    let pattern = format!(r#"(?m)^\s*{}\s*=\s*['"]([^'"]+)['"]"#, attr);
    let re = Regex::new(&pattern).ok()?;
    re.captures(content)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}
