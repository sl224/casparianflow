//! Database models for Casparian Flow Sentinel (dbx-compatible).
//!
//! These models are backend-agnostic and map from casparian_db rows.

use casparian_db::{BackendError, UnifiedDbRow};
use casparian_protocol::{SinkMode, WorkerStatus};
use serde::{Deserialize, Serialize};

// ============================================================================
// Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusEnum {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl StatusEnum {
    pub fn from_db(value: &str) -> Self {
        match value {
            "PENDING" | "Pending" => Self::Pending,
            "QUEUED" | "Queued" => Self::Queued,
            "RUNNING" | "Running" => Self::Running,
            "COMPLETED" | "Completed" => Self::Completed,
            "FAILED" | "Failed" => Self::Failed,
            "SKIPPED" | "Skipped" => Self::Skipped,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginStatusEnum {
    Pending,
    Staging,
    Active,
    Rejected,
}

impl PluginStatusEnum {
    pub fn from_db(value: &str) -> Self {
        match value {
            "PENDING" | "Pending" => Self::Pending,
            "STAGING" | "Staging" => Self::Staging,
            "ACTIVE" | "Active" => Self::Active,
            "REJECTED" | "Rejected" => Self::Rejected,
            _ => Self::Pending,
        }
    }
}

// ============================================================================
// Core Models
// ============================================================================

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
    pub file_id: i32,
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

// ============================================================================
// Plugin Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub plugin_name: String,
    pub subscription_tags: String,
    pub default_parameters: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone)]
pub struct TopicConfig {
    pub id: i32,
    pub plugin_name: String,
    pub topic_name: String,
    pub uri: String,
    pub mode: String,
    pub schema_json: Option<String>,
}

impl TopicConfig {
    pub fn sink_mode(&self) -> SinkMode {
        self.mode.parse().unwrap_or_default()
    }

    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        Ok(Self {
            id: row.get_by_name("id")?,
            plugin_name: row.get_by_name("plugin_name")?,
            topic_name: row.get_by_name("topic_name")?,
            uri: row.get_by_name("uri")?,
            mode: row.get_by_name("mode")?,
            schema_json: row.get_by_name("schema_json")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PluginSubscription {
    pub id: i32,
    pub plugin_name: String,
    pub topic_name: String,
    pub is_active: bool,
}

// ============================================================================
// Job Queue
// ============================================================================

#[derive(Debug, Clone)]
pub struct ProcessingJob {
    pub id: i64,
    pub file_id: i32,
    pub pipeline_run_id: Option<String>,
    pub plugin_name: String,
    pub config_overrides: Option<String>,
    pub status: StatusEnum,
    pub priority: i32,
    pub worker_host: Option<String>,
    pub worker_pid: Option<i32>,
    pub claim_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

impl ProcessingJob {
    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        Ok(Self {
            id: row.get_by_name("id")?,
            file_id: row.get_by_name("file_id")?,
            pipeline_run_id: row.get_by_name("pipeline_run_id")?,
            plugin_name: row.get_by_name("plugin_name")?,
            config_overrides: row.get_by_name("config_overrides")?,
            status: StatusEnum::from_db(&row.get_by_name::<String>("status")?),
            priority: row.get_by_name("priority")?,
            worker_host: row.get_by_name("worker_host")?,
            worker_pid: row.get_by_name("worker_pid")?,
            claim_time: row.get_by_name("claim_time")?,
            end_time: row.get_by_name("end_time")?,
            result_summary: row.get_by_name("result_summary")?,
            error_message: row.get_by_name("error_message")?,
            retry_count: row.get_by_name("retry_count")?,
        })
    }
}

// ============================================================================
// Bridge / Publisher
// ============================================================================

#[derive(Debug, Clone)]
pub struct Publisher {
    pub id: i32,
    pub azure_oid: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_active: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct PluginEnvironment {
    pub hash: String,
    pub lockfile_content: String,
    pub size_mb: f64,
    pub last_used: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub id: i32,
    pub plugin_name: String,
    pub version: String,
    pub source_code: String,
    pub source_hash: String,
    pub status: PluginStatusEnum,
    pub signature: Option<String>,
    pub validation_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub deployed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub env_hash: Option<String>,
    pub artifact_hash: Option<String>,
    pub publisher_id: Option<i32>,
    pub system_requirements: Option<String>,
}

// ============================================================================
// Routing & Ignore Rules
// ============================================================================

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

// ============================================================================
// Workers
// ============================================================================

#[derive(Debug, Clone)]
pub struct WorkerNode {
    pub id: i32,
    pub host: String,
    pub pid: i32,
    pub status: String,
    pub current_job_id: Option<i32>,
}

impl WorkerNode {
    pub fn worker_status(&self) -> WorkerStatus {
        self.status.parse().unwrap_or_default()
    }
}

// ============================================================================
// Error Handling & Parser Health
// ============================================================================

#[derive(Debug, Clone)]
pub struct DeadLetterJob {
    pub id: i64,
    pub original_job_id: i64,
    pub file_id: Option<i64>,
    pub plugin_name: String,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub moved_at: chrono::DateTime<chrono::Utc>,
    pub reason: Option<String>,
}

impl DeadLetterJob {
    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        Ok(Self {
            id: row.get_by_name("id")?,
            original_job_id: row.get_by_name("original_job_id")?,
            file_id: row.get_by_name("file_id")?,
            plugin_name: row.get_by_name("plugin_name")?,
            error_message: row.get_by_name("error_message")?,
            retry_count: row.get_by_name("retry_count")?,
            moved_at: row.get_by_name("moved_at")?,
            reason: row.get_by_name("reason")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ParserHealth {
    pub parser_name: String,
    pub total_executions: i64,
    pub successful_executions: i64,
    pub consecutive_failures: i32,
    pub last_failure_reason: Option<String>,
    pub paused_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl ParserHealth {
    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_executions == 0 {
            100.0
        } else {
            (self.successful_executions as f64 / self.total_executions as f64) * 100.0
        }
    }

    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        Ok(Self {
            parser_name: row.get_by_name("parser_name")?,
            total_executions: row.get_by_name("total_executions")?,
            successful_executions: row.get_by_name("successful_executions")?,
            consecutive_failures: row.get_by_name("consecutive_failures")?,
            last_failure_reason: row.get_by_name("last_failure_reason")?,
            paused_at: row.get_by_name("paused_at")?,
            created_at: row.get_by_name("created_at")?,
            updated_at: row.get_by_name("updated_at")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct QuarantinedRow {
    pub id: i64,
    pub job_id: i64,
    pub row_index: i32,
    pub error_reason: String,
    pub raw_data: Option<Vec<u8>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl QuarantinedRow {
    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        Ok(Self {
            id: row.get_by_name("id")?,
            job_id: row.get_by_name("job_id")?,
            row_index: row.get_by_name("row_index")?,
            error_reason: row.get_by_name("error_reason")?,
            raw_data: row.get_by_name("raw_data")?,
            created_at: row.get_by_name("created_at")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_enum_serialization() {
        assert_eq!(
            serde_json::to_string(&StatusEnum::Pending).unwrap(),
            "\"Pending\""
        );
        assert_eq!(
            serde_json::to_string(&StatusEnum::Running).unwrap(),
            "\"Running\""
        );
    }

    #[test]
    fn test_plugin_status_enum() {
        assert_eq!(
            serde_json::to_string(&PluginStatusEnum::Active).unwrap(),
            "\"Active\""
        );
    }

    #[test]
    fn test_parser_health_success_rate() {
        let health = ParserHealth {
            parser_name: "test".to_string(),
            total_executions: 0,
            successful_executions: 0,
            consecutive_failures: 0,
            last_failure_reason: None,
            paused_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert_eq!(health.success_rate(), 100.0);
    }
}
