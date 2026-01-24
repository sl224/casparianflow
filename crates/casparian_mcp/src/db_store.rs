//! Database-backed storage for MCP jobs and approvals.
//!
//! Uses casparian_sentinel's ApiStorage for DuckDB persistence.
//! This replaces the file-based JSON storage for production use.

use anyhow::{Context, Result};
use casparian_db::DbConnection;
use casparian_protocol::{
    ApprovalOperation as ProtocolApprovalOperation, ApprovalStatus as ProtocolApprovalStatus,
    EventType, HttpJobStatus, HttpJobType, JobResult, OutputInfo,
};
use casparian_sentinel::ApiStorage;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::approvals::{ApprovalId, ApprovalOperation, ApprovalRequest, ApprovalStatus};
use crate::jobs::{Job, JobId, JobProgress, JobState, JobType};
use crate::types::{ApprovalSummary, PluginRef};

/// Database-backed job store using ApiStorage.
pub struct DbJobStore {
    storage: ApiStorage,
}

impl DbJobStore {
    /// Create a new DB job store from a database connection.
    pub fn new(conn: DbConnection) -> Result<Self> {
        let storage = ApiStorage::new(conn);
        storage.init_schema().context("Failed to init schema")?;
        Ok(Self { storage })
    }

    /// Open from a database URL (e.g., "duckdb://~/.casparian_flow/casparian_flow.duckdb").
    pub fn open(db_url: &str) -> Result<Self> {
        let storage = ApiStorage::open(db_url)?;
        storage.init_schema().context("Failed to init schema")?;
        Ok(Self { storage })
    }

    /// Get the underlying ApiStorage (for direct access).
    pub fn storage(&self) -> &ApiStorage {
        &self.storage
    }

    /// Save a job to the database.
    pub fn save(&self, job: &Job) -> Result<()> {
        // Check if job exists
        let existing = self.storage.get_job(job.id)?;

        if existing.is_none() {
            // Create new job
            let (plugin_name, plugin_version) = match &job.plugin_ref {
                Some(PluginRef::Registered { plugin, version }) => {
                    (plugin.clone(), version.clone())
                }
                Some(PluginRef::Path { path }) => (path.to_string_lossy().to_string(), None),
                None => {
                    anyhow::bail!("Job missing plugin_ref");
                }
            };

            let plugin_version_str: Option<&str> = plugin_version.as_deref();

            let job_type = match job.job_type {
                JobType::Backtest => HttpJobType::Backtest,
                JobType::Run => HttpJobType::Run,
            };

            let spec = job.spec.as_ref().context("Job missing spec")?;
            let spec_json = serde_json::to_string(spec).context("Failed to serialize job spec")?;

            let new_id = self.storage.create_job(
                job_type,
                &plugin_name,
                plugin_version_str,
                job.input.as_deref().context("Job missing input_dir")?,
                None, // output_sink set later if needed
                job.approval_id.as_deref(),
                Some(&spec_json),
            )?;
            if new_id.as_u64() != job.id.as_u64() {
                anyhow::bail!(
                    "Job ID mismatch: DB generated {}, expected {}",
                    new_id.as_u64(),
                    job.id.as_u64()
                );
            }
        }

        // Update status and progress based on current state
        let protocol_job_id = job.id;

        match &job.state {
            JobState::Queued { .. } => {
                // Already in queued state after create
            }
            JobState::Running { progress, .. } => {
                self.storage
                    .update_job_status(protocol_job_id, HttpJobStatus::Running)?;
                self.storage.update_job_progress(
                    protocol_job_id,
                    progress.phase.as_deref().unwrap_or("running"),
                    progress.items_done,
                    progress.items_total,
                    None,
                )?;
            }
            JobState::Completed { result, .. } => {
                self.storage
                    .update_job_status(protocol_job_id, HttpJobStatus::Completed)?;
                // Store result if present
                let job_result = serde_json::from_value::<JobResultWrapper>(result.clone())
                    .context("Invalid job result payload")?;
                self.storage
                    .update_job_result(protocol_job_id, &job_result.into())?;
            }
            JobState::Failed { error, .. } => {
                self.storage
                    .update_job_status(protocol_job_id, HttpJobStatus::Failed)?;
                self.storage.update_job_error(protocol_job_id, error)?;
            }
            JobState::Cancelled { .. } => {
                self.storage.cancel_job(protocol_job_id)?;
            }
            JobState::Stalled { progress, .. } => {
                // Stalled is treated as still running in the DB
                self.storage.update_job_progress(
                    protocol_job_id,
                    progress.phase.as_deref().unwrap_or("stalled"),
                    progress.items_done,
                    progress.items_total,
                    Some("Job appears stalled"),
                )?;
            }
        }

        Ok(())
    }

    /// Load a job by ID.
    pub fn load(&self, id: &JobId) -> Result<Option<Job>> {
        let job_opt = self.storage.get_job(*id)?;

        match job_opt {
            Some(pj) => Ok(Some(from_protocol_job(pj)?)),
            None => Ok(None),
        }
    }

    /// Load all jobs.
    pub fn load_all(&self) -> Result<Vec<Job>> {
        let protocol_jobs = self.storage.list_jobs(None, 1000)?;
        protocol_jobs.into_iter().map(from_protocol_job).collect()
    }

    /// Delete a job.
    pub fn delete(&self, id: &JobId) -> Result<bool> {
        // Note: ApiStorage doesn't have a direct delete method.
        // For cleanup, use cleanup_old_data instead.
        // For now, we just mark as cancelled if possible.
        self.storage.cancel_job(*id)
    }

    /// Insert an event for a job.
    pub fn insert_event(&self, job_id: &JobId, event_type: &EventType) -> Result<u64> {
        self.storage.insert_event(*job_id, event_type)
    }

    /// List events for a job.
    pub fn list_events(
        &self,
        job_id: &JobId,
        after_event_id: Option<u64>,
    ) -> Result<Vec<casparian_protocol::Event>> {
        self.storage.list_events(*job_id, after_event_id)
    }
}

/// Database-backed approval store using ApiStorage.
pub struct DbApprovalStore {
    storage: ApiStorage,
}

impl DbApprovalStore {
    /// Create a new DB approval store from a database connection.
    pub fn new(conn: DbConnection) -> Result<Self> {
        let storage = ApiStorage::new(conn);
        storage.init_schema().context("Failed to init schema")?;
        Ok(Self { storage })
    }

    /// Open from a database URL.
    pub fn open(db_url: &str) -> Result<Self> {
        let storage = ApiStorage::open(db_url)?;
        storage.init_schema().context("Failed to init schema")?;
        Ok(Self { storage })
    }

    /// Get the underlying ApiStorage (for direct access).
    pub fn storage(&self) -> &ApiStorage {
        &self.storage
    }

    /// Save an approval request.
    pub fn save(&self, approval: &ApprovalRequest) -> Result<()> {
        // Check if already exists
        let existing = self.storage.get_approval(approval.approval_id.as_ref())?;

        if existing.is_none() {
            // Create new approval
            let protocol_op = to_protocol_operation(&approval.operation);
            let summary = approval.summary.description.clone();
            let expires_in = approval.expires_at.signed_duration_since(Utc::now());

            self.storage.create_approval(
                approval.approval_id.as_ref(),
                &protocol_op,
                &summary,
                expires_in,
            )?;
        } else {
            // Update status if changed
            match &approval.status {
                ApprovalStatus::Approved { .. } => {
                    self.storage.approve(approval.approval_id.as_ref(), None)?;
                    if let Some(job_id) = &approval.job_id {
                        let parsed: u64 = job_id
                            .parse()
                            .with_context(|| format!("Invalid job_id: {}", job_id))?;
                        self.storage.link_approval_to_job(
                            approval.approval_id.as_ref(),
                            casparian_protocol::ApiJobId::new(parsed),
                        )?;
                    }
                }
                ApprovalStatus::Rejected { reason, .. } => {
                    self.storage
                        .reject(approval.approval_id.as_ref(), None, reason.as_deref())?;
                }
                ApprovalStatus::Expired => {
                    self.storage.expire_approvals()?;
                }
                ApprovalStatus::Pending => {
                    // No update needed
                }
            }
        }

        Ok(())
    }

    /// Load an approval by ID.
    pub fn load(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>> {
        let approval_opt = self.storage.get_approval(id.as_ref())?;
        match approval_opt {
            Some(pa) => Ok(Some(from_protocol_approval(pa)?)),
            None => Ok(None),
        }
    }

    /// Load all approvals.
    pub fn load_all(&self) -> Result<Vec<ApprovalRequest>> {
        let protocol_approvals = self.storage.list_approvals(None)?;
        protocol_approvals
            .into_iter()
            .map(from_protocol_approval)
            .collect()
    }

    /// Delete an approval.
    pub fn delete(&self, _id: &ApprovalId) -> Result<bool> {
        // ApiStorage doesn't have direct delete - approvals persist for audit
        Ok(false)
    }

    /// Approve an approval request.
    pub fn approve(&self, id: &ApprovalId) -> Result<bool> {
        self.storage.approve(id.as_ref(), None)
    }

    /// Reject an approval request.
    pub fn reject(&self, id: &ApprovalId, reason: Option<&str>) -> Result<bool> {
        self.storage.reject(id.as_ref(), None, reason)
    }

    /// Expire old pending approvals.
    pub fn expire_approvals(&self) -> Result<usize> {
        self.storage.expire_approvals()
    }
}

// ============================================================================
// Type Conversion Helpers
// ============================================================================

fn from_protocol_job(pj: casparian_protocol::Job) -> Result<Job> {
    let job_type = match pj.job_type {
        HttpJobType::Backtest => JobType::Backtest,
        HttpJobType::Run | HttpJobType::Preview => JobType::Run,
    };

    let created_at: DateTime<Utc> = pj
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
            let started_at: DateTime<Utc> = started_at_str
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
            let started_at: DateTime<Utc> = started_at_str
                .parse()
                .context("Invalid started_at timestamp")?;
            let completed_at: DateTime<Utc> = completed_at_str
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
            let failed_at: DateTime<Utc> = failed_at_str
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
            let cancelled_at: DateTime<Utc> = cancelled_at_str
                .parse()
                .context("Invalid finished_at timestamp")?;
            JobState::Cancelled { cancelled_at }
        }
    };

    let plugin_ref = Some(PluginRef::Registered {
        plugin: pj.plugin_name,
        version: pj.plugin_version,
    });

    Ok(Job {
        id: pj.job_id,
        job_type,
        state,
        created_at,
        plugin_ref,
        input: Some(pj.input_dir),
        approval_id: pj.approval_id,
        spec: None, // JobSpec not persisted in protocol storage
    })
}

fn to_protocol_operation(op: &ApprovalOperation) -> ProtocolApprovalOperation {
    match op {
        ApprovalOperation::Run {
            plugin_ref,
            input_dir,
            output,
        } => {
            let (plugin_name, plugin_version) = match plugin_ref {
                PluginRef::Registered { plugin, version } => (plugin.clone(), version.clone()),
                PluginRef::Path { path } => (path.to_string_lossy().to_string(), None),
            };
            ProtocolApprovalOperation::Run {
                plugin_name,
                plugin_version,
                input_dir: input_dir.to_string_lossy().to_string(),
                file_count: 0, // Will be filled in by caller if needed
                output: Some(output.clone()),
            }
        }
        ApprovalOperation::SchemaPromote {
            ephemeral_id,
            output_path,
        } => {
            // Protocol's SchemaPromote has different fields - adapt by using ephemeral_id as plugin_name
            // and output_path as output_name
            ProtocolApprovalOperation::SchemaPromote {
                plugin_name: ephemeral_id.clone(),
                output_name: output_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "default".to_string()),
                schema: casparian_protocol::SchemaSpec {
                    columns: vec![],
                    mode: casparian_protocol::SchemaMode::Strict,
                },
            }
        }
    }
}

fn from_protocol_operation(op: &ProtocolApprovalOperation) -> Result<ApprovalOperation> {
    match op {
        ProtocolApprovalOperation::Run {
            plugin_name,
            plugin_version,
            input_dir,
            output,
            ..
        } => {
            let output = output
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Run approval missing output"))?;
            Ok(ApprovalOperation::Run {
                plugin_ref: PluginRef::Registered {
                    plugin: plugin_name.clone(),
                    version: plugin_version.clone(),
                },
                input_dir: PathBuf::from(input_dir),
                output,
            })
        }
        ProtocolApprovalOperation::SchemaPromote {
            plugin_name,
            output_name,
            ..
        } => {
            // Protocol's SchemaPromote -> MCP's SchemaPromote
            // Use plugin_name as ephemeral_id and output_name as output_path
            Ok(ApprovalOperation::SchemaPromote {
                ephemeral_id: plugin_name.clone(),
                output_path: PathBuf::from(output_name),
            })
        }
    }
}

fn from_protocol_approval(pa: casparian_protocol::Approval) -> Result<ApprovalRequest> {
    let created_at: DateTime<Utc> = pa
        .created_at
        .parse()
        .context("Invalid created_at timestamp")?;
    let expires_at: DateTime<Utc> = pa
        .expires_at
        .parse()
        .context("Invalid expires_at timestamp")?;

    let status = match pa.status {
        ProtocolApprovalStatus::Pending => ApprovalStatus::Pending,
        ProtocolApprovalStatus::Approved => {
            let approved_at: DateTime<Utc> = pa
                .decided_at
                .as_ref()
                .and_then(|s| s.parse().ok())
                .context("Missing decided_at for approved request")?;
            ApprovalStatus::Approved { approved_at }
        }
        ProtocolApprovalStatus::Rejected => {
            let rejected_at: DateTime<Utc> = pa
                .decided_at
                .as_ref()
                .and_then(|s| s.parse().ok())
                .context("Missing decided_at for rejected request")?;
            ApprovalStatus::Rejected {
                rejected_at,
                reason: pa.rejection_reason.clone(),
            }
        }
        ProtocolApprovalStatus::Expired => ApprovalStatus::Expired,
    };

    let operation = from_protocol_operation(&pa.operation)?;

    Ok(ApprovalRequest {
        approval_id: ApprovalId::from_string(pa.approval_id),
        operation,
        summary: ApprovalSummary {
            description: pa.summary.clone(),
            file_count: 0,
            estimated_rows: None,
            target_path: String::new(),
        },
        created_at,
        expires_at,
        status,
        job_id: pa.job_id.map(|id| id.as_u64().to_string()),
    })
}

// Helper for job result conversion
#[derive(serde::Deserialize)]
struct JobResultWrapper {
    #[serde(default)]
    rows_processed: u64,
    #[serde(default)]
    bytes_written: Option<u64>,
    #[serde(default)]
    outputs: Vec<OutputInfo>,
    #[serde(default)]
    metrics: HashMap<String, i64>,
}

impl From<JobResultWrapper> for JobResult {
    fn from(w: JobResultWrapper) -> Self {
        JobResult {
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
    use crate::jobs::JobSpec;
    use casparian_db::DbConnection;

    #[test]
    fn test_db_job_store_create_and_load() {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        let store = DbJobStore::new(conn).unwrap();

        let plugin_ref = PluginRef::Registered {
            plugin: "test_parser".to_string(),
            version: Some("1.0.0".to_string()),
        };

        let mut job = Job::new(JobId::new(1), JobType::Backtest);
        job.input = Some("/data/input".to_string());
        job.plugin_ref = Some(plugin_ref.clone());
        job.spec = Some(JobSpec::Backtest {
            plugin_ref,
            input_dir: "/data/input".to_string(),
            schemas: None,
            redaction: None,
        });

        store.save(&job).unwrap();

        // Load all jobs to verify
        let jobs = store.load_all().unwrap();
        assert!(!jobs.is_empty());
    }

    #[test]
    fn test_db_approval_store_workflow() {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        let store = DbApprovalStore::new(conn).unwrap();

        let approval = ApprovalRequest::new(
            ApprovalOperation::Run {
                plugin_ref: PluginRef::Registered {
                    plugin: "test_parser".to_string(),
                    version: None,
                },
                input_dir: PathBuf::from("/data/input"),
                output: "parquet://./output".to_string(),
            },
            ApprovalSummary {
                description: "Test approval".to_string(),
                file_count: 10,
                estimated_rows: Some(1000),
                target_path: "./output".to_string(),
            },
        );

        store.save(&approval).unwrap();

        // Load and verify
        let loaded = store.load(&approval.approval_id).unwrap().unwrap();
        assert_eq!(loaded.approval_id, approval.approval_id);
        assert!(matches!(loaded.status, ApprovalStatus::Pending));

        // Approve
        store.approve(&approval.approval_id).unwrap();

        let loaded = store.load(&approval.approval_id).unwrap().unwrap();
        assert!(matches!(loaded.status, ApprovalStatus::Approved { .. }));
    }
}
