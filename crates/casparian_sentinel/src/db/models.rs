//! Database models for Casparian Flow Sentinel (dbx-compatible).
//!
//! These models are backend-agnostic and map from casparian_db rows.

use casparian_db::{BackendError, DbTimestamp, UnifiedDbRow};
use casparian_protocol::{
    JobStatus as ProtocolJobStatus, PluginStatus, ProcessingStatus, QuarantineConfig, RuntimeKind,
    SinkMode,
};

// Re-export canonical enums from protocol for convenience
pub use casparian_protocol::{PluginStatus as PluginStatusEnum, ProcessingStatus as StatusEnum};

pub const TOPIC_CONFIG_COLUMNS: &[&str] = &[
    "id",
    "plugin_name",
    "topic_name",
    "uri",
    "mode",
    "quarantine_allow",
    "quarantine_max_pct",
    "quarantine_max_count",
    "quarantine_dir",
];

pub const PROCESSING_JOB_COLUMNS: &[&str] = &[
    "id",
    "file_id",
    "pipeline_run_id",
    "plugin_name",
    "config_overrides",
    "status",
    "completion_status",
    "priority",
    "worker_host",
    "worker_pid",
    "claim_time",
    "end_time",
    "result_summary",
    "error_message",
    "retry_count",
];

pub const DEAD_LETTER_COLUMNS: &[&str] = &[
    "id",
    "original_job_id",
    "file_id",
    "plugin_name",
    "error_message",
    "retry_count",
    "moved_at",
    "reason",
];

pub const PARSER_HEALTH_COLUMNS: &[&str] = &[
    "parser_name",
    "total_executions",
    "successful_executions",
    "consecutive_failures",
    "last_failure_reason",
    "paused_at",
    "created_at",
    "updated_at",
];

pub const QUARANTINE_COLUMNS: &[&str] = &[
    "id",
    "job_id",
    "row_index",
    "error_reason",
    "raw_data",
    "created_at",
];

pub const QUARANTINE_LIST_COLUMNS: &[&str] =
    &["id", "job_id", "row_index", "error_reason", "created_at"];

// ============================================================================
// Plugin Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct TopicConfig {
    pub id: i64,
    pub plugin_name: String,
    pub topic_name: String,
    pub uri: String,
    /// Sink mode - stored as SinkMode enum, parsed at the boundary.
    pub mode: SinkMode,
    pub quarantine_config: Option<QuarantineConfig>,
}

impl TopicConfig {
    /// Parse TopicConfig from a database row.
    /// Mode is parsed at the boundary with error propagation.
    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        let mode_str: String = row.get_by_name("mode")?;
        let mode = mode_str.parse::<SinkMode>().map_err(|e| {
            BackendError::TypeConversion(format!("Invalid sink mode '{}': {}", mode_str, e))
        })?;

        let allow_quarantine: Option<bool> = row.get_by_name("quarantine_allow")?;
        let max_quarantine_pct: Option<f64> = row.get_by_name("quarantine_max_pct")?;
        let max_quarantine_count: Option<i64> = row.get_by_name("quarantine_max_count")?;
        let quarantine_dir: Option<String> = row.get_by_name("quarantine_dir")?;

        let mut quarantine_config = QuarantineConfig::default();
        let mut has_quarantine_config = false;
        if let Some(value) = allow_quarantine {
            quarantine_config.allow_quarantine = value;
            has_quarantine_config = true;
        }
        if let Some(value) = max_quarantine_pct {
            quarantine_config.max_quarantine_pct = value;
            has_quarantine_config = true;
        }
        if let Some(value) = max_quarantine_count {
            let count = u64::try_from(value).map_err(|_| {
                BackendError::TypeConversion("quarantine_max_count out of range".to_string())
            })?;
            quarantine_config.max_quarantine_count = Some(count);
            has_quarantine_config = true;
        }
        if let Some(value) = quarantine_dir {
            quarantine_config.quarantine_dir = Some(value);
            has_quarantine_config = true;
        }

        Ok(Self {
            id: row.get_by_name("id")?,
            plugin_name: row.get_by_name("plugin_name")?,
            topic_name: row.get_by_name("topic_name")?,
            uri: row.get_by_name("uri")?,
            mode,
            quarantine_config: if has_quarantine_config {
                Some(quarantine_config)
            } else {
                None
            },
        })
    }
}

// ============================================================================
// Job Queue
// ============================================================================

#[derive(Debug, Clone)]
pub struct ProcessingJob {
    pub id: i64,
    pub file_id: i64,
    pub pipeline_run_id: Option<String>,
    pub plugin_name: String,
    pub config_overrides: Option<String>,
    pub status: ProcessingStatus,
    /// Completion outcome (only set for terminal jobs)
    pub completion_status: Option<ProtocolJobStatus>,
    pub priority: i32,
    pub worker_host: Option<String>,
    pub worker_pid: Option<i32>,
    pub claim_time: Option<DbTimestamp>,
    pub end_time: Option<DbTimestamp>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

impl ProcessingJob {
    /// Parse ProcessingJob from a database row.
    /// Status is parsed at the boundary with error propagation.
    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        let status_str: String = row.get_by_name("status")?;
        let status = status_str.parse::<ProcessingStatus>().map_err(|e| {
            BackendError::TypeConversion(format!(
                "Invalid processing status '{}': {}",
                status_str, e
            ))
        })?;

        let completion_status_raw: Option<String> = row.get_by_name("completion_status")?;
        let completion_status = match completion_status_raw {
            Some(s) if !s.is_empty() => Some(s.parse::<ProtocolJobStatus>().map_err(|e| {
                BackendError::TypeConversion(format!("Invalid completion status '{}': {}", s, e))
            })?),
            _ => None,
        };

        Ok(Self {
            id: row.get_by_name("id")?,
            file_id: row.get_by_name("file_id")?,
            pipeline_run_id: row.get_by_name("pipeline_run_id")?,
            plugin_name: row.get_by_name("plugin_name")?,
            config_overrides: row.get_by_name("config_overrides")?,
            status,
            completion_status,
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
// Plugin Manifest
// ============================================================================

#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub id: i64,
    pub plugin_name: String,
    pub version: String,
    pub runtime_kind: RuntimeKind,
    pub entrypoint: String,
    pub platform_os: Option<String>,
    pub platform_arch: Option<String>,
    pub source_code: String,
    pub source_hash: String,
    pub status: PluginStatus,
    pub validation_error: Option<String>,
    pub created_at: DbTimestamp,
    pub deployed_at: Option<DbTimestamp>,
    pub env_hash: String,
    pub artifact_hash: String,
    pub manifest_json: String,
    pub protocol_version: String,
    pub schema_artifacts_json: String,
    pub outputs_json: String,
    pub signature_verified: bool,
    pub signer_id: Option<String>,
    pub publisher_name: Option<String>,
    pub publisher_email: Option<String>,
    pub azure_oid: Option<String>,
    pub system_requirements: Option<String>,
}

impl PluginManifest {
    /// Parse PluginManifest from a database row.
    /// Status is parsed at the boundary with error propagation.
    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        let status_str: String = row.get_by_name("status")?;
        let status = status_str.parse::<PluginStatus>().map_err(|e| {
            BackendError::TypeConversion(format!("Invalid plugin status '{}': {}", status_str, e))
        })?;
        let runtime_str: String = row.get_by_name("runtime_kind")?;
        let runtime_kind = runtime_str.parse::<RuntimeKind>().map_err(|e| {
            BackendError::TypeConversion(format!(
                "Invalid runtime_kind '{}': {}",
                runtime_str, e
            ))
        })?;

        Ok(Self {
            id: row.get_by_name("id")?,
            plugin_name: row.get_by_name("plugin_name")?,
            version: row.get_by_name("version")?,
            runtime_kind,
            entrypoint: row.get_by_name("entrypoint")?,
            platform_os: row.get_by_name("platform_os")?,
            platform_arch: row.get_by_name("platform_arch")?,
            source_code: row.get_by_name("source_code")?,
            source_hash: row.get_by_name("source_hash")?,
            status,
            validation_error: row.get_by_name("validation_error")?,
            created_at: row.get_by_name("created_at")?,
            deployed_at: row.get_by_name("deployed_at")?,
            env_hash: row.get_by_name("env_hash")?,
            artifact_hash: row.get_by_name("artifact_hash")?,
            manifest_json: row.get_by_name("manifest_json")?,
            protocol_version: row.get_by_name("protocol_version")?,
            schema_artifacts_json: row.get_by_name("schema_artifacts_json")?,
            outputs_json: row.get_by_name("outputs_json")?,
            signature_verified: row.get_by_name("signature_verified")?,
            signer_id: row.get_by_name("signer_id")?,
            publisher_name: row.get_by_name("publisher_name")?,
            publisher_email: row.get_by_name("publisher_email")?,
            azure_oid: row.get_by_name("azure_oid")?,
            system_requirements: row.get_by_name("system_requirements")?,
        })
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
    pub moved_at: DbTimestamp,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadLetterReason {
    MaxRetriesExceeded,
    PermanentError,
}

impl DeadLetterReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeadLetterReason::MaxRetriesExceeded => "max_retries_exceeded",
            DeadLetterReason::PermanentError => "permanent_error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParserHealth {
    pub parser_name: String,
    pub total_executions: i64,
    pub successful_executions: i64,
    pub consecutive_failures: i32,
    pub last_failure_reason: Option<String>,
    pub paused_at: Option<DbTimestamp>,
    pub created_at: DbTimestamp,
    pub updated_at: DbTimestamp,
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
    pub row_index: i64,
    pub error_reason: String,
    pub raw_data: Option<Vec<u8>>,
    pub created_at: DbTimestamp,
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

#[derive(Debug, Clone)]
pub struct QuarantinedRowSummary {
    pub id: i64,
    pub job_id: i64,
    pub row_index: i64,
    pub error_reason: String,
    pub created_at: DbTimestamp,
}

impl QuarantinedRowSummary {
    pub fn from_row(row: &UnifiedDbRow) -> Result<Self, BackendError> {
        Ok(Self {
            id: row.get_by_name("id")?,
            job_id: row.get_by_name("job_id")?,
            row_index: row.get_by_name("row_index")?,
            error_reason: row.get_by_name("error_reason")?,
            created_at: row.get_by_name("created_at")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_status_serialization() {
        assert_eq!(
            serde_json::to_string(&ProcessingStatus::Pending).unwrap(),
            format!("\"{}\"", ProcessingStatus::Pending.as_str())
        );
        assert_eq!(
            serde_json::to_string(&ProcessingStatus::Running).unwrap(),
            format!("\"{}\"", ProcessingStatus::Running.as_str())
        );
    }

    #[test]
    fn test_processing_status_from_str() {
        assert_eq!(
            ProcessingStatus::Pending
                .as_str()
                .parse::<ProcessingStatus>()
                .unwrap(),
            ProcessingStatus::Pending
        );
        assert_eq!(
            ProcessingStatus::Running
                .as_str()
                .to_ascii_lowercase()
                .parse::<ProcessingStatus>()
                .unwrap(),
            ProcessingStatus::Running
        );
        assert_eq!(
            ProcessingStatus::Completed
                .as_str()
                .parse::<ProcessingStatus>()
                .unwrap(),
            ProcessingStatus::Completed
        );
    }

    #[test]
    fn test_plugin_status_serialization() {
        assert_eq!(
            serde_json::to_string(&PluginStatus::Active).unwrap(),
            format!("\"{}\"", PluginStatus::Active.as_str())
        );
        assert_eq!(
            serde_json::to_string(&PluginStatus::Superseded).unwrap(),
            format!("\"{}\"", PluginStatus::Superseded.as_str())
        );
    }

    #[test]
    fn test_plugin_status_from_str() {
        assert_eq!(
            PluginStatus::Active
                .as_str()
                .parse::<PluginStatus>()
                .unwrap(),
            PluginStatus::Active
        );
        assert_eq!(
            PluginStatus::Deployed
                .as_str()
                .parse::<PluginStatus>()
                .unwrap(),
            PluginStatus::Deployed
        );
        assert_eq!(
            PluginStatus::Superseded
                .as_str()
                .parse::<PluginStatus>()
                .unwrap(),
            PluginStatus::Superseded
        );
    }

    #[test]
    fn test_plugin_status_normalize() {
        assert_eq!(PluginStatus::Deployed.normalize(), PluginStatus::Active);
        assert_eq!(PluginStatus::Active.normalize(), PluginStatus::Active);
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
            created_at: DbTimestamp::now(),
            updated_at: DbTimestamp::now(),
        };
        assert_eq!(health.success_rate(), 100.0);
    }
}
