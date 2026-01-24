//! casparian_backtest_start - Start Backtest Job
//!
//! Starts a backtest job that validates a parser against a corpus.
//! Returns immediately with a job_id; poll job_status for progress.

use super::McpTool;
use crate::core::CoreHandle;
use crate::jobs::{JobExecutorHandle, JobSpec};
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::types::{PluginRef, RedactionPolicy, SchemasMap};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub struct BacktestStartTool;

#[derive(Debug, Deserialize)]
struct BacktestArgs {
    plugin_ref: PluginRef,
    input_dir: String,
    #[serde(default)]
    schemas: Option<SchemasMap>,
    #[serde(default)]
    redaction: Option<RedactionPolicy>,
}

#[derive(Debug, Serialize)]
struct BacktestStartResult {
    job_id: String,
    status: String,
}

impl McpTool for BacktestStartTool {
    fn name(&self) -> &'static str {
        "casparian_backtest_start"
    }

    fn description(&self) -> &'static str {
        "Start a backtest job (non-blocking)"
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
                "schemas": {
                    "type": "object",
                    "description": "Optional per-output schemas",
                    "additionalProperties": {
                        "type": "object",
                        "properties": {
                            "output_name": { "type": "string" },
                            "mode": { "type": "string", "enum": ["strict", "allow_extra", "allow_missing_optional"] },
                            "columns": { "type": "array" }
                        }
                    }
                },
                "redaction": {
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["none", "truncate", "hash"], "default": "hash" }
                    }
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
        executor: &JobExecutorHandle,
    ) -> Result<Value> {
        let args: BacktestArgs = serde_json::from_value(args)?;

        // Validate input_dir path
        security.validate_path(std::path::Path::new(&args.input_dir))?;

        let spec = JobSpec::Backtest {
            plugin_ref: args.plugin_ref.clone(),
            input_dir: args.input_dir.clone(),
            schemas: args.schemas.clone(),
            redaction: args.redaction.clone(),
        };

        // Create job via Core
        let job = core.create_job(spec, None)?;
        let job_id = job.id;

        // Enqueue for execution - executor will start when ready
        executor.enqueue(job_id)?;

        let result = BacktestStartResult {
            job_id: job_id.to_string(),
            status: "queued".to_string(),
        };

        Ok(serde_json::to_value(result)?)
    }
}
