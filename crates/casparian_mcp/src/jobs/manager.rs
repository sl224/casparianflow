//! Job Manager - Job Lifecycle Management
//!
//! Manages job creation, execution, and cleanup (Control API or DB-backed).

use super::{Job, JobId, JobProgress, JobSpec, JobState, JobType};
use crate::types::PluginRef;
use anyhow::{Context, Result};
use casparian_db::DbConnection;
use casparian_protocol::{HttpJobStatus, HttpJobType};
use casparian_sentinel::{ApiStorage, ControlClient, DEFAULT_CONTROL_ADDR};
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info, warn};

enum JobBackend {
    Db { db_path: PathBuf },
    Control { control_addr: String },
}

/// Job manager for tracking and executing jobs
pub struct JobManager {
    backend: JobBackend,
}

impl JobManager {
    /// Create a new job manager backed by DuckDB at the given path.
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = DbConnection::open_duckdb(&db_path)
            .with_context(|| format!("Failed to open DB at {}", db_path.display()))?;
        let storage = ApiStorage::new(conn);
        storage.init_schema().context("Failed to init schema")?;

        Ok(Self {
            backend: JobBackend::Db { db_path },
        })
    }

    /// Create a new job manager backed by the Control API.
    pub fn new_control(control_addr: Option<String>) -> Result<Self> {
        let addr = control_addr.unwrap_or_else(|| DEFAULT_CONTROL_ADDR.to_string());
        let client = ControlClient::connect_with_timeout(&addr, Duration::from_millis(500))
            .with_context(|| format!("Failed to connect to Control API at {}", addr))?;
        if !client.ping().unwrap_or(false) {
            anyhow::bail!("Control API did not respond at {}", addr);
        }
        Ok(Self {
            backend: JobBackend::Control { control_addr: addr },
        })
    }

    fn storage(&self) -> Result<ApiStorage> {
        match &self.backend {
            JobBackend::Db { db_path } => {
                let conn = DbConnection::open_duckdb(db_path)
                    .with_context(|| format!("Failed to open DB at {}", db_path.display()))?;
                let storage = ApiStorage::new(conn);
                storage.init_schema().context("Failed to init schema")?;
                Ok(storage)
            }
            JobBackend::Control { .. } => {
                anyhow::bail!("JobManager storage is not available in Control API mode");
            }
        }
    }

    fn control_client(&self) -> Result<ControlClient> {
        match &self.backend {
            JobBackend::Control { control_addr } => {
                ControlClient::connect_with_timeout(control_addr, Duration::from_secs(5))
                    .with_context(|| {
                        format!("Failed to connect to Control API at {}", control_addr)
                    })
            }
            JobBackend::Db { .. } => {
                anyhow::bail!("Control API client is not available in DB mode");
            }
        }
    }

    /// Create a new job with a fully-specified JobSpec.
    pub fn create_job(&self, spec: JobSpec, approval_id: Option<String>) -> Result<Job> {
        let (job_type, plugin_ref, input_dir, output_sink) = job_spec_components(&spec);
        let (plugin_name, plugin_version) = plugin_ref_to_parts(&plugin_ref);

        let spec_json = serde_json::to_string(&spec).context("Failed to serialize job spec")?;
        match &self.backend {
            JobBackend::Db { .. } => {
                let storage = self.storage()?;
                let job_id = storage.create_job(
                    job_type,
                    &plugin_name,
                    plugin_version.as_deref(),
                    &input_dir,
                    output_sink.as_deref(),
                    approval_id.as_deref(),
                    Some(&spec_json),
                )?;

                let protocol_job = storage
                    .get_job(job_id)?
                    .context("Job missing after create")?;
                let job = from_protocol_job(protocol_job)?;
                info!("Created job: {} ({})", job.id, job.job_type);
                Ok(job)
            }
            JobBackend::Control { .. } => {
                let client = self.control_client()?;
                let job_id = client.create_api_job(
                    job_type,
                    &plugin_name,
                    plugin_version.as_deref(),
                    &input_dir,
                    output_sink.as_deref(),
                    approval_id.as_deref(),
                    Some(&spec_json),
                )?;
                let protocol_job = client
                    .get_api_job(job_id)?
                    .context("Job missing after create")?;
                let job = from_protocol_job(protocol_job)?;
                info!("Created job via Control API: {} ({})", job.id, job.job_type);
                Ok(job)
            }
        }
    }

    /// Get a job by ID
    pub fn get_job(&self, id: &JobId) -> Result<Option<Job>> {
        match &self.backend {
            JobBackend::Db { .. } => {
                let storage = self.storage()?;
                let job = storage.get_job(id.to_proto())?;
                match job {
                    Some(protocol_job) => Ok(Some(from_protocol_job(protocol_job)?)),
                    None => Ok(None),
                }
            }
            JobBackend::Control { .. } => {
                let client = self.control_client()?;
                let job = client.get_api_job(id.to_proto())?;
                match job {
                    Some(protocol_job) => Ok(Some(from_protocol_job(protocol_job)?)),
                    None => Ok(None),
                }
            }
        }
    }

    /// Start a job (transition from queued to running)
    pub fn start_job(&self, id: &JobId) -> Result<()> {
        match &self.backend {
            JobBackend::Db { .. } => {
                let storage = self.storage()?;
                let job = storage.get_job(id.to_proto())?.context("Job not found")?;

                match job.status {
                    HttpJobStatus::Queued => {}
                    HttpJobStatus::Running => anyhow::bail!("Job is already running"),
                    HttpJobStatus::Completed | HttpJobStatus::Failed | HttpJobStatus::Cancelled => {
                        anyhow::bail!("Job is already terminal")
                    }
                }

                storage.update_job_status(id.to_proto(), HttpJobStatus::Running)?;
                storage.update_job_progress(id.to_proto(), "running", 0, None, None)?;

                info!("Started job: {}", id);
                Ok(())
            }
            JobBackend::Control { .. } => {
                let client = self.control_client()?;
                let job = client
                    .get_api_job(id.to_proto())?
                    .context("Job not found")?;
                match job.status {
                    HttpJobStatus::Queued => {}
                    HttpJobStatus::Running => anyhow::bail!("Job is already running"),
                    HttpJobStatus::Completed | HttpJobStatus::Failed | HttpJobStatus::Cancelled => {
                        anyhow::bail!("Job is already terminal")
                    }
                }
                client.update_api_job_status(id.to_proto(), HttpJobStatus::Running)?;
                client.update_api_job_progress(
                    id.to_proto(),
                    casparian_protocol::http_types::JobProgress {
                        phase: "running".to_string(),
                        items_done: 0,
                        items_total: None,
                        message: None,
                    },
                )?;
                info!("Started job via Control API: {}", id);
                Ok(())
            }
        }
    }

    /// Update job progress
    pub fn update_progress(&self, id: &JobId, progress: JobProgress) -> Result<()> {
        match &self.backend {
            JobBackend::Db { .. } => {
                let storage = self.storage()?;
                let job = storage.get_job(id.to_proto())?.context("Job not found")?;
                if job.status != HttpJobStatus::Running {
                    anyhow::bail!("Cannot update progress for non-running job");
                }

                storage.update_job_progress(
                    id.to_proto(),
                    progress.phase.as_deref().unwrap_or("running"),
                    progress.items_done,
                    progress.items_total,
                    None,
                )?;

                debug!("Updated progress for job: {}", id);
                Ok(())
            }
            JobBackend::Control { .. } => {
                let client = self.control_client()?;
                let job = client
                    .get_api_job(id.to_proto())?
                    .context("Job not found")?;
                if job.status != HttpJobStatus::Running {
                    anyhow::bail!("Cannot update progress for non-running job");
                }

                client.update_api_job_progress(
                    id.to_proto(),
                    casparian_protocol::http_types::JobProgress {
                        phase: progress
                            .phase
                            .clone()
                            .unwrap_or_else(|| "running".to_string()),
                        items_done: progress.items_done,
                        items_total: progress.items_total,
                        message: None,
                    },
                )?;

                debug!("Updated progress via Control API for job: {}", id);
                Ok(())
            }
        }
    }

    /// Complete a job
    pub fn complete_job(&self, id: &JobId, result: serde_json::Value) -> Result<()> {
        // Persist result (must be parseable)
        let wrapper: JobResultWrapper =
            serde_json::from_value(result).context("Invalid job result payload")?;
        match &self.backend {
            JobBackend::Db { .. } => {
                let storage = self.storage()?;
                let job = storage.get_job(id.to_proto())?.context("Job not found")?;
                if job.status != HttpJobStatus::Running {
                    anyhow::bail!("Cannot complete a non-running job");
                }
                storage.update_job_result(id.to_proto(), &wrapper.into())?;
                storage.update_job_status(id.to_proto(), HttpJobStatus::Completed)?;

                info!("Completed job: {}", id);
                Ok(())
            }
            JobBackend::Control { .. } => {
                let client = self.control_client()?;
                let job = client
                    .get_api_job(id.to_proto())?
                    .context("Job not found")?;
                if job.status != HttpJobStatus::Running {
                    anyhow::bail!("Cannot complete a non-running job");
                }
                client.update_api_job_result(id.to_proto(), wrapper.into())?;
                client.update_api_job_status(id.to_proto(), HttpJobStatus::Completed)?;

                info!("Completed job via Control API: {}", id);
                Ok(())
            }
        }
    }

    /// Fail a job
    pub fn fail_job(&self, id: &JobId, error: impl Into<String>) -> Result<()> {
        let error = error.into();
        match &self.backend {
            JobBackend::Db { .. } => {
                let storage = self.storage()?;
                let job = storage.get_job(id.to_proto())?.context("Job not found")?;
                if matches!(
                    job.status,
                    HttpJobStatus::Completed | HttpJobStatus::Failed | HttpJobStatus::Cancelled
                ) {
                    anyhow::bail!("Cannot fail a terminal job");
                }
                storage.update_job_status(id.to_proto(), HttpJobStatus::Failed)?;
                storage.update_job_error(id.to_proto(), &error)?;

                warn!("Failed job {}: {}", id, error);
                Ok(())
            }
            JobBackend::Control { .. } => {
                let client = self.control_client()?;
                let job = client
                    .get_api_job(id.to_proto())?
                    .context("Job not found")?;
                if matches!(
                    job.status,
                    HttpJobStatus::Completed | HttpJobStatus::Failed | HttpJobStatus::Cancelled
                ) {
                    anyhow::bail!("Cannot fail a terminal job");
                }
                client.update_api_job_status(id.to_proto(), HttpJobStatus::Failed)?;
                client.update_api_job_error(id.to_proto(), &error)?;

                warn!("Failed job via Control API {}: {}", id, error);
                Ok(())
            }
        }
    }

    /// Cancel a job
    pub fn cancel_job(&self, id: &JobId) -> Result<bool> {
        match &self.backend {
            JobBackend::Db { .. } => {
                let storage = self.storage()?;
                let cancelled = storage.cancel_job(id.to_proto())?;
                if cancelled {
                    info!("Cancelled job: {}", id);
                }
                Ok(cancelled)
            }
            JobBackend::Control { .. } => {
                let client = self.control_client()?;
                let cancelled = client.cancel_api_job(id.to_proto())?;
                if cancelled {
                    info!("Cancelled job via Control API: {}", id);
                }
                Ok(cancelled)
            }
        }
    }

    /// List jobs with optional filter
    pub fn list_jobs(&self, status_filter: Option<&str>, limit: usize) -> Result<Vec<Job>> {
        let status = match status_filter {
            Some("queued") => Some(HttpJobStatus::Queued),
            Some("running") => Some(HttpJobStatus::Running),
            Some("completed") => Some(HttpJobStatus::Completed),
            Some("failed") => Some(HttpJobStatus::Failed),
            Some("cancelled") => Some(HttpJobStatus::Cancelled),
            Some(other) => anyhow::bail!("Unsupported status filter: {}", other),
            None => None,
        };
        match &self.backend {
            JobBackend::Db { .. } => {
                let storage = self.storage()?;
                let jobs = storage.list_jobs(status, limit)?;
                jobs.into_iter().map(from_protocol_job).collect()
            }
            JobBackend::Control { .. } => {
                let client = self.control_client()?;
                let jobs = client.list_api_jobs(status, Some(limit as i64), Some(0))?;
                jobs.into_iter().map(from_protocol_job).collect()
            }
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn job_spec_components(spec: &JobSpec) -> (HttpJobType, PluginRef, String, Option<String>) {
    match spec {
        JobSpec::Backtest {
            plugin_ref,
            input_dir,
            ..
        } => (
            HttpJobType::Backtest,
            plugin_ref.clone(),
            input_dir.clone(),
            None,
        ),
        JobSpec::Run {
            plugin_ref,
            input_dir,
            output_dir,
            ..
        } => (
            HttpJobType::Run,
            plugin_ref.clone(),
            input_dir.clone(),
            output_dir.clone(),
        ),
    }
}

fn plugin_ref_to_parts(plugin_ref: &PluginRef) -> (String, Option<String>) {
    match plugin_ref {
        PluginRef::Registered { plugin, version } => (plugin.clone(), version.clone()),
        PluginRef::Path { path } => (path.to_string_lossy().to_string(), None),
    }
}

fn from_protocol_job(pj: casparian_protocol::Job) -> Result<Job> {
    let job_type = match pj.job_type {
        HttpJobType::Backtest => JobType::Backtest,
        HttpJobType::Run | HttpJobType::Preview => JobType::Run,
    };

    let created_at = pj
        .created_at
        .parse()
        .context("Invalid created_at timestamp")?;

    let state = match pj.status {
        HttpJobStatus::Queued => JobState::Queued {
            queued_at: created_at,
        },
        HttpJobStatus::Running => {
            let started_at_str = pj
                .started_at
                .as_ref()
                .context("Missing started_at for running job")?;
            let started_at = started_at_str
                .parse()
                .context("Invalid started_at timestamp")?;
            let progress = pj
                .progress
                .map(|p| JobProgress {
                    phase: Some(p.phase),
                    items_done: p.items_done,
                    items_total: p.items_total,
                    elapsed_ms: 0,
                    eta_ms: None,
                    updated_at: Utc::now(),
                    extra: serde_json::Value::Null,
                })
                .unwrap_or_else(JobProgress::new);
            JobState::Running {
                started_at,
                progress,
            }
        }
        HttpJobStatus::Completed => {
            let started_at_str = pj
                .started_at
                .as_ref()
                .context("Missing started_at for completed job")?;
            let completed_at_str = pj
                .finished_at
                .as_ref()
                .context("Missing finished_at for completed job")?;
            let started_at = started_at_str
                .parse()
                .context("Invalid started_at timestamp")?;
            let completed_at = completed_at_str
                .parse()
                .context("Invalid finished_at timestamp")?;
            let result = pj.result.context("Missing result for completed job")?;
            let result = serde_json::to_value(result).context("Invalid job result JSON")?;
            JobState::Completed {
                started_at,
                completed_at,
                result,
            }
        }
        HttpJobStatus::Failed => {
            let started_at = match pj.started_at.as_ref() {
                Some(value) => Some(value.parse().context("Invalid started_at timestamp")?),
                None => None,
            };
            let failed_at_str = pj
                .finished_at
                .as_ref()
                .context("Missing finished_at for failed job")?;
            let failed_at = failed_at_str
                .parse()
                .context("Invalid finished_at timestamp")?;
            JobState::Failed {
                started_at,
                failed_at,
                error: pj
                    .error_message
                    .context("Missing error_message for failed job")?,
            }
        }
        HttpJobStatus::Cancelled => {
            let cancelled_at_str = pj
                .finished_at
                .as_ref()
                .context("Missing finished_at for cancelled job")?;
            let cancelled_at = cancelled_at_str
                .parse()
                .context("Invalid finished_at timestamp")?;
            JobState::Cancelled { cancelled_at }
        }
    };

    let spec = match pj.spec_json {
        Some(json) => Some(serde_json::from_str(&json).context("Invalid job_spec_json")?),
        None => None,
    };

    if !state.is_terminal() && spec.is_none() {
        anyhow::bail!("Non-terminal job missing job_spec_json");
    }

    let plugin_ref = spec
        .as_ref()
        .map(|s| match s {
            JobSpec::Backtest { plugin_ref, .. } => plugin_ref.clone(),
            JobSpec::Run { plugin_ref, .. } => plugin_ref.clone(),
        })
        .or_else(|| {
            Some(PluginRef::Registered {
                plugin: pj.plugin_name.clone(),
                version: pj.plugin_version.clone(),
            })
        });

    Ok(Job {
        id: JobId::new(pj.job_id.as_u64()),
        job_type,
        state,
        created_at,
        plugin_ref,
        input: Some(pj.input_dir),
        approval_id: pj.approval_id,
        spec,
    })
}

#[derive(serde::Deserialize)]
struct JobResultWrapper {
    #[serde(default)]
    rows_processed: u64,
    #[serde(default)]
    bytes_written: Option<u64>,
    #[serde(default)]
    outputs: Vec<casparian_protocol::OutputInfo>,
    #[serde(default)]
    metrics: HashMap<String, i64>,
}

impl From<JobResultWrapper> for casparian_protocol::JobResult {
    fn from(w: JobResultWrapper) -> Self {
        casparian_protocol::JobResult {
            rows_processed: w.rows_processed,
            bytes_written: w.bytes_written,
            outputs: w.outputs,
            metrics: w.metrics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    fn create_test_manager() -> (JobManager, TempDir) {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.duckdb");
        let manager = JobManager::new(db_path).unwrap();
        (manager, temp)
    }

    #[test]
    fn test_create_job() {
        let (manager, _temp) = create_test_manager();

        let spec = JobSpec::Backtest {
            plugin_ref: PluginRef::registered("test_parser"),
            input_dir: "/data/input".to_string(),
            schemas: None,
            redaction: None,
        };

        let job = manager.create_job(spec, None).unwrap();

        assert!(matches!(job.state, JobState::Queued { .. }));
        assert_eq!(job.job_type, JobType::Backtest);
    }

    #[test]
    fn test_job_lifecycle() {
        let (manager, _temp) = create_test_manager();

        let spec = JobSpec::Backtest {
            plugin_ref: PluginRef::registered("test_parser"),
            input_dir: "/data/input".to_string(),
            schemas: None,
            redaction: None,
        };

        let job = manager.create_job(spec, None).unwrap();
        let id = job.id;

        manager.start_job(&id).unwrap();
        let job = manager.get_job(&id).unwrap().unwrap();
        assert!(matches!(job.state, JobState::Running { .. }));

        let progress = JobProgress::new().with_items(50, Some(100));
        manager.update_progress(&id, progress).unwrap();

        manager
            .complete_job(&id, serde_json::json!({"pass_rate": 0.95}))
            .unwrap();
        let job = manager.get_job(&id).unwrap().unwrap();
        assert!(matches!(job.state, JobState::Completed { .. }));
    }

    #[test]
    fn test_cancel_job() {
        let (manager, _temp) = create_test_manager();

        let spec = JobSpec::Run {
            plugin_ref: PluginRef::registered("test_parser"),
            input_dir: "/data/input".to_string(),
            output_dir: None,
            schemas: None,
        };

        let job = manager.create_job(spec, None).unwrap();
        let id = job.id;

        let cancelled = manager.cancel_job(&id).unwrap();
        assert!(cancelled);

        let job = manager.get_job(&id).unwrap().unwrap();
        assert!(matches!(job.state, JobState::Cancelled { .. }));
    }

    #[test]
    fn test_list_jobs() {
        let (manager, _temp) = create_test_manager();

        let spec1 = JobSpec::Backtest {
            plugin_ref: PluginRef::registered("test_parser"),
            input_dir: "/data/input".to_string(),
            schemas: None,
            redaction: None,
        };
        let spec2 = JobSpec::Run {
            plugin_ref: PluginRef::registered("test_parser"),
            input_dir: "/data/input".to_string(),
            output_dir: None,
            schemas: None,
        };

        manager.create_job(spec1, None).unwrap();
        manager.create_job(spec2, None).unwrap();

        let jobs = manager.list_jobs(None, 10).unwrap();
        assert_eq!(jobs.len(), 2);

        let jobs = manager.list_jobs(Some("queued"), 10).unwrap();
        assert_eq!(jobs.len(), 2);
    }
}
