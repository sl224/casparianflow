//! Job Store - Persistent Job State
//!
//! Stores job state in JSON files for persistence across server restarts.
//!
//! # Storage Format
//!
//! ```text
//! ~/.casparian_flow/mcp_jobs/
//! ├── {job_id_1}.json
//! ├── {job_id_2}.json
//! └── ...
//! ```

use super::{Job, JobId};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::debug;

/// Persistent job store
pub struct JobStore {
    /// Directory for job files
    dir: PathBuf,
}

impl JobStore {
    /// Create a new job store
    pub fn new(dir: PathBuf) -> Result<Self> {
        // Ensure directory exists
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create job store directory: {}", dir.display()))?;

        Ok(Self { dir })
    }

    /// Get the file path for a job
    fn job_path(&self, id: &JobId) -> PathBuf {
        self.dir.join(format!("{}.json", id.as_u64()))
    }

    /// Save a job to disk
    pub fn save(&self, job: &Job) -> Result<()> {
        let path = self.job_path(&job.id);
        let json = serde_json::to_string_pretty(job)?;

        atomic_write(&path, json.as_bytes())
            .with_context(|| format!("Failed to write job file: {}", path.display()))?;

        debug!("Saved job {} to {}", job.id, path.display());
        Ok(())
    }

    /// Load a job from disk
    pub fn load(&self, id: &JobId) -> Result<Option<Job>> {
        let path = self.job_path(id);

        if !path.exists() {
            return Ok(None);
        }

        let json = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read job file: {}", path.display()))?;

        let job: Job = serde_json::from_str(&json)
            .with_context(|| format!("Failed to parse job file: {}", path.display()))?;

        Ok(Some(job))
    }

    /// Load all jobs from disk
    pub fn load_all(&self) -> Result<Vec<Job>> {
        let mut jobs = Vec::new();

        let entries = fs::read_dir(&self.dir).with_context(|| {
            format!("Failed to read job store directory: {}", self.dir.display())
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let json = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read job file: {}", path.display()))?;
            let job: Job = serde_json::from_str(&json)
                .with_context(|| format!("Failed to parse job file: {}", path.display()))?;
            jobs.push(job);
        }

        debug!("Loaded {} jobs from {}", jobs.len(), self.dir.display());
        Ok(jobs)
    }

    /// Delete a job from disk
    pub fn delete(&self, id: &JobId) -> Result<bool> {
        let path = self.job_path(id);

        if !path.exists() {
            return Ok(false);
        }

        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete job file: {}", path.display()))?;

        debug!("Deleted job {} from {}", id, path.display());
        Ok(true)
    }

    /// Get the storage directory
    #[allow(dead_code)]
    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }
}

/// Atomic write via temp file + rename
fn atomic_write(path: &PathBuf, content: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let temp_path = parent.join(format!(".tmp_{}", uuid::Uuid::new_v4()));
    fs::write(&temp_path, content)
        .with_context(|| format!("Failed to write temp file: {}", temp_path.display()))?;
    fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename temp file to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobType;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load() {
        let temp = TempDir::new().unwrap();
        let store = JobStore::new(temp.path().to_path_buf()).unwrap();

        let job = Job::new(JobId::new(1), JobType::Backtest);
        let id = job.id;

        store.save(&job).unwrap();

        let loaded = store.load(&id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, id);
    }

    #[test]
    fn test_load_nonexistent() {
        let temp = TempDir::new().unwrap();
        let store = JobStore::new(temp.path().to_path_buf()).unwrap();

        let id = JobId::new(9999);
        let loaded = store.load(&id).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_load_all() {
        let temp = TempDir::new().unwrap();
        let store = JobStore::new(temp.path().to_path_buf()).unwrap();

        // Save multiple jobs
        for i in 0..3 {
            let job = Job::new(JobId::new(i + 1), JobType::Backtest);
            store.save(&job).unwrap();
        }

        let jobs = store.load_all().unwrap();
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_delete() {
        let temp = TempDir::new().unwrap();
        let store = JobStore::new(temp.path().to_path_buf()).unwrap();

        let job = Job::new(JobId::new(1), JobType::Backtest);
        let id = job.id;

        store.save(&job).unwrap();
        assert!(store.load(&id).unwrap().is_some());

        let deleted = store.delete(&id).unwrap();
        assert!(deleted);

        assert!(store.load(&id).unwrap().is_none());
    }
}
