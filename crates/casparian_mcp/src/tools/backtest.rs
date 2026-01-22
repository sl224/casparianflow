//! casparian_backtest_start - Start Backtest Job
//!
//! Starts a backtest job that validates a parser against a corpus.
//! Returns immediately with a job_id; poll job_status for progress.

use super::McpTool;
use crate::approvals::ApprovalManager;
use crate::jobs::{Job, JobManager, JobType};
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::types::{PluginRef, RedactionPolicy, SchemasMap};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

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

#[async_trait::async_trait]
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

    async fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        jobs: &Arc<Mutex<JobManager>>,
        _approvals: &Arc<Mutex<ApprovalManager>>,
        _config: &McpServerConfig,
    ) -> Result<Value> {
        let args: BacktestArgs = serde_json::from_value(args)?;

        // Validate input_dir path
        security.validate_path(std::path::Path::new(&args.input_dir))?;

        // Create job
        let mut job_manager = jobs.lock().await;

        let mut job = job_manager.create_job(JobType::Backtest)?;
        job = job.with_plugin(args.plugin_ref.clone());
        job = job.with_input(&args.input_dir);

        let job_id = job.id.clone();

        // Check if we can start immediately
        let status = if job_manager.can_start_job() {
            job_manager.start_job(&job_id)?;

            // TODO: Spawn actual backtest execution task
            // For now, the job is started but not executed
            // tokio::spawn(run_backtest(job_id.clone(), args, config.clone()));

            "running"
        } else {
            "queued"
        };

        let result = BacktestStartResult {
            job_id: job_id.to_string(),
            status: status.to_string(),
        };

        Ok(serde_json::to_value(result)?)
    }
}
