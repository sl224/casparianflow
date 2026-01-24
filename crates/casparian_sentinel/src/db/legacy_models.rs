//! Legacy/unused database models (pre-v1).
//!
//! These are kept for reference but are not wired into the sentinel runtime.

use casparian_db::{BackendError, DbTimestamp, UnifiedDbRow};
use casparian_protocol::WorkerStatus;

#[derive(Debug, Clone)]
pub struct SourceRoot {
    pub id: i32,
    pub path: String,
    pub root_type: String,
    pub active: i32,
}

#[derive(Debug, Clone)]
pub struct FileHashRegistry {
    pub content_hash: String,
    pub first_seen: String,
    pub size_bytes: i32,
}

#[derive(Debug, Clone)]
pub struct FileLocation {
    pub id: i32,
    pub source_root_id: i32,
    pub rel_path: String,
    pub filename: String,
    pub last_known_mtime: Option<f64>,
    pub last_known_size: Option<i32>,
    pub current_version_id: Option<i32>,
    pub discovered_time: String,
    pub last_seen_time: String,
}

#[derive(Debug, Clone)]
pub struct FileTag {
    pub file_id: i64,
    pub tag: String,
}

#[derive(Debug, Clone)]
pub struct FileVersion {
    pub id: i32,
    pub location_id: i32,
    pub content_hash: String,
    pub size_bytes: i32,
    pub modified_time: String,
    pub detected_at: String,
    pub applied_tags: String,
}

#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub plugin_name: String,
    pub subscription_tags: String,
    pub default_parameters: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone)]
pub struct PluginSubscription {
    pub id: i32,
    pub plugin_name: String,
    pub topic_name: String,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct Publisher {
    pub id: i32,
    pub azure_oid: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub created_at: DbTimestamp,
    pub last_active: DbTimestamp,
}

#[derive(Debug, Clone)]
pub struct PluginEnvironment {
    pub hash: String,
    pub lockfile_content: String,
    pub size_mb: f64,
    pub last_used: DbTimestamp,
    pub created_at: DbTimestamp,
}

#[derive(Debug, Clone)]
pub struct RoutingRule {
    pub id: i32,
    pub pattern: String,
    pub tag: String,
    pub priority: i32,
}

#[derive(Debug, Clone)]
pub struct IgnoreRule {
    pub id: i32,
    pub pattern: String,
}

#[derive(Debug, Clone)]
pub struct WorkerNode {
    pub id: i32,
    pub host: String,
    pub pid: i32,
    pub status: WorkerStatus,
    pub current_job_id: Option<i32>,
}

impl WorkerNode {
    /// Parse WorkerNode from a database row.
    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        let status_str: String = row.get_by_name("status")?;
        let status = status_str.parse::<WorkerStatus>().map_err(|e| {
            BackendError::TypeConversion(format!("Invalid worker status '{}': {}", status_str, e))
        })?;

        Ok(Self {
            id: row.get_by_name("id")?,
            host: row
                .get_by_name("host")
                .or_else(|_| row.get_by_name("hostname"))?,
            pid: row.get_by_name("pid")?,
            status,
            current_job_id: row.get_by_name("current_job_id")?,
        })
    }
}
