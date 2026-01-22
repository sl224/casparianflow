//! Job Manager - Job Lifecycle Management
//!
//! Manages job creation, execution, and cleanup.

use super::{
    Job, JobId, JobProgress, JobState, JobStore, JobType, DEFAULT_MAX_CONCURRENT,
    DEFAULT_TIMEOUT_MS, JOB_TTL_HOURS, STALL_THRESHOLD_MS,
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, info, warn};

/// Job manager for tracking and executing jobs
pub struct JobManager {
    /// Job store for persistence
    store: JobStore,

    /// In-memory job cache
    jobs: HashMap<JobId, Job>,

    /// Currently running job (if any)
    running_job: Option<JobId>,

    /// Maximum concurrent jobs
    max_concurrent: usize,

    /// Job timeout in milliseconds
    timeout_ms: u64,
}

impl JobManager {
    /// Create a new job manager
    pub fn new(jobs_dir: PathBuf) -> Result<Self> {
        let store = JobStore::new(jobs_dir)?;

        // Load existing jobs from store
        let jobs = store.load_all()?;
        let jobs: HashMap<JobId, Job> = jobs.into_iter().map(|j| (j.id.clone(), j)).collect();

        Ok(Self {
            store,
            jobs,
            running_job: None,
            max_concurrent: DEFAULT_MAX_CONCURRENT,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        })
    }

    /// Create a new job
    pub fn create_job(&mut self, job_type: JobType) -> Result<Job> {
        let job = Job::new(job_type);
        self.store.save(&job)?;
        self.jobs.insert(job.id.clone(), job.clone());
        info!("Created job: {} ({})", job.id, job.job_type);
        Ok(job)
    }

    /// Get a job by ID
    pub fn get_job(&self, id: &JobId) -> Option<&Job> {
        self.jobs.get(id)
    }

    /// Get a mutable reference to a job
    pub fn get_job_mut(&mut self, id: &JobId) -> Option<&mut Job> {
        self.jobs.get_mut(id)
    }

    /// Start a job (transition from queued to running)
    pub fn start_job(&mut self, id: &JobId) -> Result<()> {
        // Check if we can start another job
        if self.running_job.is_some() {
            anyhow::bail!("A job is already running");
        }

        let job = self
            .jobs
            .get_mut(id)
            .context("Job not found")?;

        job.start();
        self.running_job = Some(id.clone());
        self.store.save(job)?;

        info!("Started job: {}", id);
        Ok(())
    }

    /// Update job progress
    pub fn update_progress(&mut self, id: &JobId, progress: JobProgress) -> Result<()> {
        let job = self
            .jobs
            .get_mut(id)
            .context("Job not found")?;

        job.update_progress(progress);
        self.store.save(job)?;

        debug!("Updated progress for job: {}", id);
        Ok(())
    }

    /// Complete a job
    pub fn complete_job(&mut self, id: &JobId, result: serde_json::Value) -> Result<()> {
        let job = self
            .jobs
            .get_mut(id)
            .context("Job not found")?;

        job.complete(result);
        self.store.save(job)?;

        if self.running_job.as_ref() == Some(id) {
            self.running_job = None;
        }

        info!("Completed job: {}", id);
        Ok(())
    }

    /// Fail a job
    pub fn fail_job(&mut self, id: &JobId, error: impl Into<String>) -> Result<()> {
        let job = self
            .jobs
            .get_mut(id)
            .context("Job not found")?;

        let error = error.into();
        job.fail(&error);
        self.store.save(job)?;

        if self.running_job.as_ref() == Some(id) {
            self.running_job = None;
        }

        warn!("Failed job {}: {}", id, error);
        Ok(())
    }

    /// Cancel a job
    pub fn cancel_job(&mut self, id: &JobId) -> Result<bool> {
        let job = match self.jobs.get_mut(id) {
            Some(j) => j,
            None => return Ok(false),
        };

        if job.state.is_terminal() {
            return Ok(false);
        }

        job.cancel();
        self.store.save(job)?;

        if self.running_job.as_ref() == Some(id) {
            self.running_job = None;
        }

        info!("Cancelled job: {}", id);
        Ok(true)
    }

    /// List jobs with optional filter
    pub fn list_jobs(&self, status_filter: Option<&str>, limit: usize) -> Vec<&Job> {
        let mut jobs: Vec<&Job> = self
            .jobs
            .values()
            .filter(|j| {
                status_filter
                    .map(|s| j.state.status_str() == s)
                    .unwrap_or(true)
            })
            .collect();

        // Sort by created_at descending (newest first)
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        jobs.into_iter().take(limit).collect()
    }

    /// Check for stalled jobs and mark them
    pub fn check_stalled(&mut self) -> Result<Vec<JobId>> {
        let now = Utc::now();
        let mut stalled = Vec::new();

        for (id, job) in &mut self.jobs {
            if let JobState::Running { started_at, progress } = &job.state {
                let elapsed = (now - progress.updated_at).num_milliseconds() as u64;

                if elapsed > STALL_THRESHOLD_MS {
                    job.state = JobState::Stalled {
                        started_at: *started_at,
                        last_progress_at: progress.updated_at,
                        progress: progress.clone(),
                    };
                    stalled.push(id.clone());
                    warn!("Job {} appears stalled (no progress for {}ms)", id, elapsed);
                }
            }
        }

        // Persist changes
        for id in &stalled {
            if let Some(job) = self.jobs.get(id) {
                self.store.save(job)?;
            }
        }

        Ok(stalled)
    }

    /// Clean up old completed jobs
    pub fn cleanup_old_jobs(&mut self) -> Result<usize> {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::hours(JOB_TTL_HOURS);

        let to_remove: Vec<JobId> = self
            .jobs
            .iter()
            .filter(|(_, job)| {
                job.state.is_terminal() && job.created_at < cutoff
            })
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();

        for id in to_remove {
            self.jobs.remove(&id);
            self.store.delete(&id)?;
        }

        if count > 0 {
            info!("Cleaned up {} old jobs", count);
        }

        Ok(count)
    }

    /// Check if a job can be started
    pub fn can_start_job(&self) -> bool {
        self.running_job.is_none()
    }

    /// Get the currently running job ID
    pub fn running_job_id(&self) -> Option<&JobId> {
        self.running_job.as_ref()
    }
}

/// Handle for interacting with a running job
pub struct JobHandle {
    pub id: JobId,
}

impl JobHandle {
    /// Create a new job handle
    pub fn new(id: JobId) -> Self {
        Self { id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (JobManager, TempDir) {
        let temp = TempDir::new().unwrap();
        let manager = JobManager::new(temp.path().to_path_buf()).unwrap();
        (manager, temp)
    }

    #[test]
    fn test_create_job() {
        let (mut manager, _temp) = create_test_manager();

        let job = manager.create_job(JobType::Backtest).unwrap();

        assert!(matches!(job.state, JobState::Queued { .. }));
        assert_eq!(job.job_type, JobType::Backtest);
    }

    #[test]
    fn test_job_lifecycle() {
        let (mut manager, _temp) = create_test_manager();

        // Create
        let job = manager.create_job(JobType::Backtest).unwrap();
        let id = job.id.clone();

        // Start
        manager.start_job(&id).unwrap();
        let job = manager.get_job(&id).unwrap();
        assert!(matches!(job.state, JobState::Running { .. }));

        // Update progress
        let progress = JobProgress::new().with_items(50, Some(100));
        manager.update_progress(&id, progress).unwrap();

        // Complete
        manager
            .complete_job(&id, serde_json::json!({"pass_rate": 0.95}))
            .unwrap();
        let job = manager.get_job(&id).unwrap();
        assert!(matches!(job.state, JobState::Completed { .. }));
    }

    #[test]
    fn test_cancel_job() {
        let (mut manager, _temp) = create_test_manager();

        let job = manager.create_job(JobType::Run).unwrap();
        let id = job.id.clone();

        let cancelled = manager.cancel_job(&id).unwrap();
        assert!(cancelled);

        let job = manager.get_job(&id).unwrap();
        assert!(matches!(job.state, JobState::Cancelled { .. }));
    }

    #[test]
    fn test_list_jobs() {
        let (mut manager, _temp) = create_test_manager();

        manager.create_job(JobType::Backtest).unwrap();
        manager.create_job(JobType::Run).unwrap();

        let jobs = manager.list_jobs(None, 10);
        assert_eq!(jobs.len(), 2);

        let jobs = manager.list_jobs(Some("queued"), 10);
        assert_eq!(jobs.len(), 2);
    }
}
