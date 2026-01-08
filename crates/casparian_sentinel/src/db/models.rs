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
// Parser Health Tracking (Circuit Breaker)
// ============================================================================

/// Parser health tracking for circuit breaker pattern.
///
/// Tracks execution statistics and failure patterns to automatically
/// pause parsers that are failing consistently.
#[derive(Debug, Clone, FromRow)]
pub struct ParserHealth {
    /// Parser name (unique identifier)
    pub parser_name: String,
    /// Total execution count (successes + failures)
    pub total_executions: i64,
    /// Successful execution count
    pub successful_executions: i64,
    /// Consecutive failure count (resets on success)
    pub consecutive_failures: i32,
    /// Reason for last failure (for debugging)
    pub last_failure_reason: Option<String>,
    /// When the circuit breaker tripped (parser paused)
    /// NULL means parser is active, non-NULL means paused
    pub paused_at: Option<DateTime<Utc>>,
    /// When health record was created
    pub created_at: DateTime<Utc>,
    /// When health record was last updated
    pub updated_at: DateTime<Utc>,
}

impl ParserHealth {
    /// Check if this parser is currently paused (circuit open)
    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    /// Get success rate as percentage (0-100)
    pub fn success_rate(&self) -> f64 {
        if self.total_executions == 0 {
            100.0 // No executions = healthy by default
        } else {
            (self.successful_executions as f64 / self.total_executions as f64) * 100.0
        }
    }
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

    #[test]
    fn test_parser_health_success_rate() {
        let now = Utc::now();

        // No executions = 100% (healthy by default)
        let health = ParserHealth {
            parser_name: "test".to_string(),
            total_executions: 0,
            successful_executions: 0,
            consecutive_failures: 0,
            last_failure_reason: None,
            paused_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!((health.success_rate() - 100.0).abs() < 0.01);

        // 8 out of 10 = 80%
        let health = ParserHealth {
            parser_name: "test".to_string(),
            total_executions: 10,
            successful_executions: 8,
            consecutive_failures: 0,
            last_failure_reason: None,
            paused_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!((health.success_rate() - 80.0).abs() < 0.01);
    }

    #[test]
    fn test_parser_health_is_paused() {
        let now = Utc::now();

        // Not paused
        let health = ParserHealth {
            parser_name: "test".to_string(),
            total_executions: 10,
            successful_executions: 5,
            consecutive_failures: 5,
            last_failure_reason: Some("timeout".to_string()),
            paused_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(!health.is_paused());

        // Paused
        let health = ParserHealth {
            parser_name: "test".to_string(),
            total_executions: 10,
            successful_executions: 5,
            consecutive_failures: 5,
            last_failure_reason: Some("timeout".to_string()),
            paused_at: Some(now),
            created_at: now,
            updated_at: now,
        };
        assert!(health.is_paused());
    }
}
