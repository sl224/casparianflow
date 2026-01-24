//! casparian_preview - Preview Parser Output
//!
//! Runs a parser on sample files and returns redacted sample output.
//! No output is written - this is read-only.

use super::McpTool;
use crate::core::CoreHandle;
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::types::{
    ColumnDefinition, DataType, PluginRef, RedactionPolicy, SchemaDefinition, SimpleDataType,
};
use anyhow::{Context, Result};
use casparian_protocol::JobId as ProtoJobId;
use casparian_worker::cancel::CancellationToken;
use casparian_worker::native_runtime::NativeSubprocessRuntime;
use casparian_worker::runtime::{PluginRuntime, RunContext};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub struct PreviewTool;

#[derive(Debug, Deserialize)]
struct PreviewArgs {
    plugin_ref: PluginRef,
    files: Vec<String>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    redaction: Option<RedactionPolicy>,
}

fn default_limit() -> usize {
    100
}

#[derive(Debug, Serialize)]
struct OutputPreview {
    schema: SchemaDefinition,
    schema_hash: String,
    sample_rows: Vec<Value>,
    row_count: usize,
}

#[derive(Debug, Serialize)]
struct PreviewError {
    file: String,
    error: String,
}

#[derive(Debug, Serialize)]
struct PreviewResult {
    outputs: HashMap<String, OutputPreview>,
    errors: Vec<PreviewError>,
}

impl McpTool for PreviewTool {
    fn name(&self) -> &'static str {
        "casparian_preview"
    }

    fn description(&self) -> &'static str {
        "Preview parser output on sample files"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "plugin_ref": {
                    "oneOf": [
                        {
                            "type": "object",
                            "properties": {
                                "plugin": { "type": "string" },
                                "version": { "type": "string" }
                            },
                            "required": ["plugin"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" }
                            },
                            "required": ["path"]
                        }
                    ]
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "maxItems": 10
                },
                "limit": {
                    "type": "integer",
                    "default": 100,
                    "maximum": 1000
                },
                "redaction": {
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["none", "truncate", "hash"], "default": "hash" },
                        "max_sample_count": { "type": "integer", "default": 5 },
                        "max_value_length": { "type": "integer", "default": 100 }
                    }
                }
            },
            "required": ["plugin_ref", "files"]
        })
    }

    fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> Result<Value> {
        let args: PreviewArgs = serde_json::from_value(args)?;

        // Validate file paths
        let mut validated_files = Vec::new();
        for file in &args.files {
            let validated = security.validate_path(std::path::Path::new(file))?;
            validated_files.push(validated);
        }

        let redaction = args.redaction.unwrap_or_default();
        let row_limit = args.limit.min(1000); // Cap at 1000 for safety

        // Resolve parser path
        let parser_path = match resolve_parser_path(&args.plugin_ref) {
            Ok(p) => p,
            Err(e) => {
                return Ok(serde_json::to_value(PreviewResult {
                    outputs: HashMap::new(),
                    errors: vec![PreviewError {
                        file: "parser".to_string(),
                        error: format!("Failed to resolve parser: {}", e),
                    }],
                })?);
            }
        };

        info!("Preview using parser: {:?}", parser_path);

        // Create runtime
        let runtime = NativeSubprocessRuntime::new();
        let cancel_token = CancellationToken::new();

        let mut all_outputs: HashMap<String, OutputPreview> = HashMap::new();
        let mut errors = Vec::new();
        let mut total_rows_per_output: HashMap<String, usize> = HashMap::new();

        // Process each file
        for (idx, file_path) in validated_files.iter().enumerate() {
            let ctx = create_run_context(idx, &parser_path);

            match runtime.run_file(&ctx, file_path, &cancel_token) {
                Ok(run_outputs) => {
                    // Process each output from this file
                    for (batch_idx, batches) in run_outputs.output_batches.iter().enumerate() {
                        let output_name = run_outputs
                            .output_info
                            .get(batch_idx)
                            .map(|i| i.name.clone())
                            .unwrap_or_else(|| format!("output_{}", batch_idx));

                        // Get current row count for this output
                        let current_rows = total_rows_per_output
                            .entry(output_name.clone())
                            .or_insert(0);
                        let remaining_limit = row_limit.saturating_sub(*current_rows);

                        if remaining_limit == 0 {
                            continue; // Already have enough rows for this output
                        }

                        // Convert batches to rows
                        for batch in batches {
                            if *current_rows >= row_limit {
                                break;
                            }

                            let record_batch = batch.as_record_batch();
                            let schema = record_batch.schema();

                            // Get or create output preview entry
                            let preview = match all_outputs.entry(output_name.clone()) {
                                std::collections::hash_map::Entry::Occupied(entry) => {
                                    entry.into_mut()
                                }
                                std::collections::hash_map::Entry::Vacant(entry) => {
                                    // Convert Arrow schema to our schema definition
                                    let columns: Vec<ColumnDefinition> = schema
                                        .fields()
                                        .iter()
                                        .map(|field| ColumnDefinition {
                                            name: field.name().clone(),
                                            data_type: arrow_type_to_data_type(field.data_type()),
                                            nullable: field.is_nullable(),
                                            format: None,
                                        })
                                        .collect();

                                    let schema_def = SchemaDefinition {
                                        output_name: output_name.clone(),
                                        mode: crate::types::SchemaMode::Strict,
                                        columns,
                                    };

                                    // Compute schema hash
                                    let schema_json = serde_json::to_string(&schema_def)
                                        .context("Failed to serialize schema definition")?;
                                    let schema_hash = compute_hash(&schema_json);

                                    entry.insert(OutputPreview {
                                        schema: schema_def,
                                        schema_hash,
                                        sample_rows: Vec::new(),
                                        row_count: 0,
                                    })
                                }
                            };

                            // Convert Arrow rows to JSON
                            let num_rows = record_batch.num_rows();
                            let remaining = row_limit.saturating_sub(*current_rows);
                            let rows_to_take = remaining.min(num_rows);

                            for row_idx in 0..rows_to_take {
                                if preview.sample_rows.len() >= row_limit {
                                    break;
                                }

                                let row_json = arrow_row_to_json(record_batch, row_idx, &redaction);
                                preview.sample_rows.push(row_json);
                                preview.row_count += 1;
                            }

                            *current_rows += rows_to_take;
                        }
                    }
                }
                Err(e) => {
                    warn!("Preview failed on {}: {}", file_path.display(), e);
                    errors.push(PreviewError {
                        file: file_path.display().to_string(),
                        error: format!("{}", e),
                    });
                }
            }
        }

        let result = PreviewResult {
            outputs: all_outputs,
            errors,
        };

        Ok(serde_json::to_value(result)?)
    }
}

/// Resolve parser path from PluginRef
fn resolve_parser_path(plugin_ref: &PluginRef) -> Result<PathBuf> {
    match plugin_ref {
        PluginRef::Path { path } => {
            let path = PathBuf::from(path);
            if path.ends_with("evtx_native") || path.to_string_lossy().contains("evtx_native") {
                return find_evtx_native_binary(&path);
            }
            if path.exists() {
                Ok(path)
            } else {
                anyhow::bail!("Parser not found: {}", path.display())
            }
        }
        PluginRef::Registered { plugin, version: _ } => {
            if plugin == "evtx_native" {
                find_evtx_native_binary(&PathBuf::from("parsers/evtx_native"))
            } else {
                anyhow::bail!("Unknown registered parser: {}", plugin)
            }
        }
    }
}

fn find_evtx_native_binary(base_path: &Path) -> Result<PathBuf> {
    let candidates = vec![
        base_path.join("target/release/evtx_native"),
        PathBuf::from("parsers/evtx_native/target/release/evtx_native"),
        PathBuf::from("target/release/evtx_native"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    // Try to build it
    info!("evtx_native binary not found, attempting to build...");
    let plugin_dir = if base_path.join("Cargo.toml").exists() {
        base_path.to_path_buf()
    } else {
        PathBuf::from("parsers/evtx_native")
    };

    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&plugin_dir)
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        anyhow::bail!("Failed to build evtx_native parser");
    }

    let binary = plugin_dir.join("target/release/evtx_native");
    if binary.exists() {
        Ok(binary)
    } else {
        anyhow::bail!("evtx_native binary not found after build")
    }
}

fn create_run_context(file_idx: usize, parser_path: &Path) -> RunContext {
    let proto_job_id = ProtoJobId::new(file_idx as u64);

    // Provide wildcard schema hash - preview doesn't validate schema
    let mut schema_hashes = HashMap::new();
    // Use wildcard "*" to accept any output in preview mode
    schema_hashes.insert("*".to_string(), "preview".to_string());

    RunContext {
        job_id: proto_job_id,
        file_id: file_idx as i64,
        entrypoint: parser_path.to_string_lossy().to_string(),
        env_hash: None,
        source_code: None,
        schema_hashes,
    }
}

/// Convert Arrow DataType to our DataType
fn arrow_type_to_data_type(arrow_type: &arrow::datatypes::DataType) -> DataType {
    use arrow::datatypes::DataType as ArrowType;

    match arrow_type {
        ArrowType::Null => DataType::Simple(SimpleDataType::String),
        ArrowType::Boolean => DataType::Simple(SimpleDataType::Boolean),
        ArrowType::Int8
        | ArrowType::Int16
        | ArrowType::Int32
        | ArrowType::Int64
        | ArrowType::UInt8
        | ArrowType::UInt16
        | ArrowType::UInt32
        | ArrowType::UInt64 => DataType::Simple(SimpleDataType::Int64),
        ArrowType::Float16 | ArrowType::Float32 | ArrowType::Float64 => {
            DataType::Simple(SimpleDataType::Float64)
        }
        ArrowType::Utf8 | ArrowType::LargeUtf8 => DataType::Simple(SimpleDataType::String),
        ArrowType::Binary | ArrowType::LargeBinary => DataType::Simple(SimpleDataType::Binary),
        ArrowType::Date32 | ArrowType::Date64 => DataType::Simple(SimpleDataType::Date),
        ArrowType::Timestamp(_, _) => DataType::Simple(SimpleDataType::String), // Store as string for now
        _ => DataType::Simple(SimpleDataType::String),                          // Fallback
    }
}

/// Convert a single Arrow row to JSON with redaction
fn arrow_row_to_json(
    batch: &arrow::record_batch::RecordBatch,
    row_idx: usize,
    redaction: &RedactionPolicy,
) -> Value {
    use arrow::array::*;
    use arrow::datatypes::DataType as ArrowType;

    let mut row = serde_json::Map::new();
    let schema = batch.schema();

    for (col_idx, field) in schema.fields().iter().enumerate() {
        let col_name = field.name();
        let array = batch.column(col_idx);

        let value = if array.is_null(row_idx) {
            Value::Null
        } else {
            match array.data_type() {
                ArrowType::Boolean => {
                    let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
                    Value::Bool(arr.value(row_idx))
                }
                ArrowType::Int8 => {
                    let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
                    Value::Number(arr.value(row_idx).into())
                }
                ArrowType::Int16 => {
                    let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
                    Value::Number(arr.value(row_idx).into())
                }
                ArrowType::Int32 => {
                    let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
                    Value::Number(arr.value(row_idx).into())
                }
                ArrowType::Int64 => {
                    let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
                    Value::Number(arr.value(row_idx).into())
                }
                ArrowType::UInt8 => {
                    let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
                    Value::Number(arr.value(row_idx).into())
                }
                ArrowType::UInt16 => {
                    let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
                    Value::Number(arr.value(row_idx).into())
                }
                ArrowType::UInt32 => {
                    let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
                    Value::Number(arr.value(row_idx).into())
                }
                ArrowType::UInt64 => {
                    let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
                    Value::Number(arr.value(row_idx).into())
                }
                ArrowType::Float32 => {
                    let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
                    serde_json::Number::from_f64(arr.value(row_idx) as f64)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }
                ArrowType::Float64 => {
                    let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
                    serde_json::Number::from_f64(arr.value(row_idx))
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }
                ArrowType::Utf8 => {
                    let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
                    let s = arr.value(row_idx);
                    Value::String(redaction.redact(s))
                }
                ArrowType::LargeUtf8 => {
                    let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
                    let s = arr.value(row_idx);
                    Value::String(redaction.redact(s))
                }
                ArrowType::Binary => {
                    let arr = array.as_any().downcast_ref::<BinaryArray>().unwrap();
                    let bytes = arr.value(row_idx);
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                    Value::String(redaction.redact(&encoded))
                }
                ArrowType::LargeBinary => {
                    let arr = array.as_any().downcast_ref::<LargeBinaryArray>().unwrap();
                    let bytes = arr.value(row_idx);
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                    Value::String(redaction.redact(&encoded))
                }
                ArrowType::Timestamp(_, _) => {
                    // Handle timestamp as string
                    let arr = array.as_any().downcast_ref::<TimestampMicrosecondArray>();
                    if let Some(arr) = arr {
                        let micros = arr.value(row_idx);
                        let secs = micros / 1_000_000;
                        let nsecs = ((micros % 1_000_000) * 1000) as u32;
                        if let Some(dt) = chrono::DateTime::from_timestamp(secs, nsecs) {
                            Value::String(dt.to_rfc3339())
                        } else {
                            Value::Number(micros.into())
                        }
                    } else {
                        Value::Null
                    }
                }
                _ => {
                    // Fallback: try to get string representation
                    Value::String(format!("<unsupported:{:?}>", array.data_type()))
                }
            }
        };

        row.insert(col_name.clone(), value);
    }

    Value::Object(row)
}

/// Compute SHA256 hash of a string, return first 16 hex chars
fn compute_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8]) // First 8 bytes = 16 hex chars
}
