use anyhow::{Context, Result};
use casparian_protocol::JobId;
use casparian_sinks::OutputBatch;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bridge::{self, BridgeConfig, OutputInfo};
use crate::venv_manager::VenvManager;

pub struct RunContext {
    pub job_id: JobId,
    pub file_id: i64,
    pub entrypoint: String,
    pub env_hash: Option<String>,
    pub source_code: Option<String>,
    pub schema_hashes: HashMap<String, String>,
}

pub struct RunOutputs {
    pub output_batches: Vec<Vec<OutputBatch>>,
    pub output_info: Vec<OutputInfo>,
    pub logs: String,
}

pub trait PluginRuntime {
    fn run_file(&self, ctx: &RunContext, input_path: &Path) -> Result<RunOutputs>;
}

pub struct PythonShimRuntime {
    venv_manager: Arc<VenvManager>,
    shim_path: PathBuf,
}

impl PythonShimRuntime {
    pub fn new(venv_manager: Arc<VenvManager>, shim_path: PathBuf) -> Self {
        Self {
            venv_manager,
            shim_path,
        }
    }
}

impl PluginRuntime for PythonShimRuntime {
    fn run_file(&self, ctx: &RunContext, input_path: &Path) -> Result<RunOutputs> {
        let env_hash = ctx
            .env_hash
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Environment hash is required"))?;
        if env_hash.trim().is_empty() {
            anyhow::bail!("Environment hash is required");
        }
        if env_hash == "system" {
            anyhow::bail!("System env_hash is not supported; deploy with a lockfile");
        }

        let interpreter = self.venv_manager.interpreter_path(env_hash);
        if !interpreter.exists() {
            anyhow::bail!(
                "Environment {} not installed on worker. Preinstall the env for this plugin.",
                env_hash
            );
        }

        let source_code = ctx
            .source_code
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Source code is required"))?;

        let config = BridgeConfig {
            interpreter_path: interpreter,
            source_code,
            file_path: input_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid input path"))?
                .to_string(),
            job_id: ctx.job_id,
            file_id: ctx.file_id,
            shim_path: self.shim_path.clone(),
            inherit_stdio: false,
        };

        let result = bridge::execute_bridge(config).context("Bridge execution failed")?;

        Ok(RunOutputs {
            output_batches: result.output_batches,
            output_info: result.output_info,
            logs: result.logs,
        })
    }
}
