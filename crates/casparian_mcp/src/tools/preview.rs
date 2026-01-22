//! casparian_preview - Preview Parser Output
//!
//! Runs a parser on sample files and returns redacted sample output.
//! No output is written - this is read-only.

use super::McpTool;
use crate::approvals::ApprovalManager;
use crate::jobs::JobManager;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::types::{PluginRef, RedactionPolicy, SchemaDefinition, ColumnDefinition, DataType, SimpleDataType};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

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

#[async_trait::async_trait]
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

    async fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        _jobs: &Arc<Mutex<JobManager>>,
        _approvals: &Arc<Mutex<ApprovalManager>>,
        _config: &McpServerConfig,
    ) -> Result<Value> {
        let args: PreviewArgs = serde_json::from_value(args)?;

        // Validate file paths
        for file in &args.files {
            security.validate_path(std::path::Path::new(file))?;
        }

        let _redaction = args.redaction.unwrap_or_default();

        // TODO: Execute parser via casparian_worker bridge
        // For now, return a placeholder response

        let mut outputs = HashMap::new();

        // Placeholder output for demonstration
        outputs.insert(
            "events".to_string(),
            OutputPreview {
                schema: SchemaDefinition {
                    output_name: "events".to_string(),
                    mode: crate::types::SchemaMode::Strict,
                    columns: vec![
                        ColumnDefinition {
                            name: "timestamp".to_string(),
                            data_type: DataType::Simple(SimpleDataType::String),
                            nullable: false,
                            format: Some("%Y-%m-%dT%H:%M:%S%.fZ".to_string()),
                        },
                        ColumnDefinition {
                            name: "event_id".to_string(),
                            data_type: DataType::Simple(SimpleDataType::Int64),
                            nullable: false,
                            format: None,
                        },
                    ],
                },
                schema_hash: "[placeholder_hash]".to_string(),
                sample_rows: vec![],
                row_count: 0,
            },
        );

        let result = PreviewResult {
            outputs,
            errors: vec![],
        };

        Ok(serde_json::to_value(result)?)
    }
}
