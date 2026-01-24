//! casparian_run_request - Request Parser Execution
//!
//! Creates an approval request for parser execution.
//! Human must approve via CLI before the job runs.

use super::McpTool;
use crate::approvals::ApprovalOperation;
use crate::core::CoreHandle;
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::types::{ApprovalSummary, PluginRef, SchemasMap};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct RunRequestTool;

#[derive(Debug, Deserialize)]
struct RunRequestArgs {
    plugin_ref: PluginRef,
    input_dir: String,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    schemas: Option<SchemasMap>,
}

#[derive(Debug, Serialize)]
struct RunRequestSummary {
    description: String,
    file_count: usize,
    estimated_rows: Option<u64>,
    target_path: String,
}

#[derive(Debug, Serialize)]
struct RunRequestResult {
    approval_id: String,
    status: String,
    summary: RunRequestSummary,
    expires_at: String,
    approve_command: String,
}

impl McpTool for RunRequestTool {
    fn name(&self) -> &'static str {
        "casparian_run_request"
    }

    fn description(&self) -> &'static str {
        "Request parser execution (creates approval request)"
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
                "input_dir": {
                    "type": "string",
                    "description": "Directory containing input files"
                },
                "output": {
                    "type": "string",
                    "description": "Output path or sink URL (e.g., parquet://./output/)"
                },
                "schemas": {
                    "type": "object",
                    "description": "Optional schema override"
                }
            },
            "required": ["plugin_ref", "input_dir"]
        })
    }

    fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> Result<Value> {
        let args: RunRequestArgs = serde_json::from_value(args)?;

        // Validate input_dir path
        let input_path = security.validate_path(std::path::Path::new(&args.input_dir))?;

        // Count files in input directory
        let file_count = WalkDir::new(&input_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .count();

        // Default output path
        let output = args
            .output
            .unwrap_or_else(|| "parquet://./output/".to_string());

        // Build summary
        let summary = ApprovalSummary {
            description: format!(
                "Run {} on {} ({} files)",
                args.plugin_ref.display_name(),
                args.input_dir,
                file_count
            ),
            file_count,
            estimated_rows: None, // Would need preview to estimate
            target_path: output.clone(),
        };

        // Create operation
        let operation = ApprovalOperation::Run {
            plugin_ref: args.plugin_ref,
            input_dir: PathBuf::from(&args.input_dir),
            output: output.clone(),
        };

        // Create approval request via Core
        let approval = core.create_approval(operation, summary.clone())?;

        let result = RunRequestResult {
            approval_id: approval.approval_id.to_string(),
            status: "pending_approval".to_string(),
            summary: RunRequestSummary {
                description: summary.description,
                file_count: summary.file_count,
                estimated_rows: summary.estimated_rows,
                target_path: summary.target_path,
            },
            expires_at: approval.expires_at.to_rfc3339(),
            approve_command: approval.approve_command(),
        };

        Ok(serde_json::to_value(result)?)
    }
}
