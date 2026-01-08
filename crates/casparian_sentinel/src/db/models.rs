//! Database models for Casparian Flow Sentinel
//!
//! Ported from Python SQLAlchemy to Rust sqlx.
//! Uses derive macros for FromRow to map database rows to structs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use casparian_protocol::{SinkMode, WorkerStatus};

// ============================================================================
// Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StatusEnum {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PluginStatusEnum {
    Pending,
    Staging,
    Active,
    Rejected,
}

// ============================================================================
// Core Models
// ============================================================================

#[derive(Debug, Clone, FromRow)]
pub struct SourceRoot {
    pub id: i32,
    pub path: String,
    #[sqlx(rename = "type")]
    pub root_type: String,
    pub active: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct FileHashRegistry {
    pub content_hash: String,
    pub first_seen: DateTime<Utc>,
    pub size_bytes: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct FileLocation {
    pub id: i32,
    pub source_root_id: i32,
    pub rel_path: String,
    pub filename: String,
    pub last_known_mtime: Option<f64>,
    pub last_known_size: Option<i32>,
    pub current_version_id: Option<i32>,
    pub discovered_time: DateTime<Utc>,
    pub last_seen_time: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct FileTag {
    pub file_id: i32,
    pub tag: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct FileVersion {
    pub id: i32,
    pub location_id: i32,
    pub content_hash: String,
    pub size_bytes: i32,
    pub modified_time: DateTime<Utc>,
    pub detected_at: DateTime<Utc>,
    pub applied_tags: String,
}

// ============================================================================
// Plugin Configuration
// ============================================================================

#[derive(Debug, Clone, FromRow)]
pub struct PluginConfig {
    pub plugin_name: String,
    pub subscription_tags: String,
    pub default_parameters: Option<String>, // JSON
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TopicConfig {
    pub id: i32,
    pub plugin_name: String,
    pub topic_name: String,
    pub uri: String,
    /// Mode as string for database compatibility. Use `sink_mode()` for typed access.
    pub mode: String,
    pub schema_json: Option<String>,
}

impl TopicConfig {
    /// Get mode as typed enum
    pub fn sink_mode(&self) -> SinkMode {
        self.mode.parse().unwrap_or_default()
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct PluginSubscription {
    pub id: i32,
    pub plugin_name: String,
    pub topic_name: String,
    pub is_active: bool,
}

// ============================================================================
// Job Queue
// ============================================================================

#[derive(Debug, Clone, FromRow)]
pub struct ProcessingJob {
    pub id: i64,
    pub file_version_id: i32,
    pub plugin_name: String,
    pub config_overrides: Option<String>, // JSON
    pub status: StatusEnum,
    pub priority: i32,
    pub worker_host: Option<String>,
    pub worker_pid: Option<i32>,
    pub claim_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

// ============================================================================
// v5.0 Bridge Mode: Publisher & Environment
// ============================================================================

#[derive(Debug, Clone, FromRow)]
pub struct Publisher {
    pub id: i32,
    pub azure_oid: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct PluginEnvironment {
    pub hash: String, // SHA256 of lockfile content
    pub lockfile_content: String,
    pub size_mb: f64,
    pub last_used: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct PluginManifest {
    pub id: i32,
    pub plugin_name: String,
    pub version: String,
    pub source_code: String,
    pub source_hash: String,
    pub status: PluginStatusEnum,
    pub signature: Option<String>,
    pub validation_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub deployed_at: Option<DateTime<Utc>>,
    // v5.0 Bridge Mode fields
    pub env_hash: Option<String>,
    pub artifact_hash: Option<String>,
    pub publisher_id: Option<i32>,
    pub system_requirements: Option<String>, // JSON
}

// ============================================================================
// Routing & Ignore Rules
// ============================================================================

#[derive(Debug, Clone, FromRow)]
pub struct RoutingRule {
    pub id: i32,
    pub pattern: String,
    pub tag: String,
    pub priority: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct IgnoreRule {
    pub id: i32,
    pub source_root_id: Option<i32>,
    pub pattern: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Worker Tracking
// ============================================================================

#[derive(Debug, Clone, FromRow)]
pub struct WorkerNode {
    pub hostname: String,
    pub pid: i32,
    pub ip_address: Option<String>,
    pub env_signature: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    /// Status as string for database compatibility. Use `worker_status()` for typed access.
    pub status: String,
    pub current_job_id: Option<i32>,
}

impl WorkerNode {
    /// Get status as typed enum
    pub fn worker_status(&self) -> WorkerStatus {
        self.status.parse().unwrap_or_default()
    }
}

// ============================================================================
// Error Handling (W5)
// ============================================================================

/// A job that has been moved to the dead letter queue after exhausting retries
///
/// Dead letter jobs are jobs that have failed too many times and have been
/// removed from the main processing queue. They can be replayed manually
/// after the underlying issue has been fixed.
#[derive(Debug, Clone, FromRow)]
pub struct DeadLetterJob {
    pub id: i64,
    pub original_job_id: i64,
    pub file_version_id: Option<i64>,
    pub plugin_name: String,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub moved_at: String,
    pub reason: Option<String>,
}

/// Health tracking for parsers (circuit breaker state)
///
/// This table tracks the health of each parser to implement a circuit breaker
/// pattern. When a parser fails consecutively, it can be paused to prevent
/// further failures from overwhelming the system.
#[derive(Debug, Clone, FromRow)]
pub struct ParserHealth {
    pub parser_name: String,
    pub consecutive_failures: i32,
    pub paused_at: Option<String>,
    pub last_failure_reason: Option<String>,
    pub total_executions: i32,
    pub successful_executions: i32,
}

impl ParserHealth {
    /// Calculate success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_executions == 0 {
            0.0
        } else {
            (self.successful_executions as f64 / self.total_executions as f64) * 100.0
        }
    }

    /// Check if the parser is currently paused
    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }
}

/// A row that failed processing and was quarantined
///
/// When individual rows fail during processing (e.g., schema validation errors),
/// they are quarantined here so the rest of the file can be processed.
#[derive(Debug, Clone, FromRow)]
pub struct QuarantinedRow {
    pub id: i64,
    pub job_id: i64,
    pub row_index: i32,
    pub error_reason: String,
    pub raw_data: Option<Vec<u8>>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_enum_serialization() {
        // Serde serializes enum variants as-is (PascalCase)
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
}
