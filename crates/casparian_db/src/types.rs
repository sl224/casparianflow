//! Unified types for all Casparian Flow database entities.
//!
//! These types are the single source of truth. All interfaces (CLI, Tauri, MCP)
//! should use these types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Scout Types
// ============================================================================

/// A source location to watch for files
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Type of source (local, smb, s3)
    pub source_type: SourceType,
    /// Root path to scan
    pub path: String,
    /// Polling interval in seconds
    pub poll_interval_secs: u64,
    /// Whether this source is enabled
    pub enabled: bool,
}

/// Type of source filesystem
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceType {
    /// Local filesystem
    #[default]
    Local,
    /// SMB/CIFS network share
    Smb {
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
    /// Amazon S3 bucket
    S3 {
        region: String,
        bucket: String,
        #[serde(default)]
        access_key: Option<String>,
        #[serde(default)]
        secret_key: Option<String>,
    },
}

/// A tagging rule maps file patterns to tags
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggingRule {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Source ID this rule applies to
    pub source_id: String,
    /// Glob pattern to match files
    pub pattern: String,
    /// Tag to assign to matching files
    pub tag: String,
    /// Priority (higher = evaluated first)
    pub priority: i32,
    /// Whether this rule is enabled
    pub enabled: bool,
}

/// Status of a discovered file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Pending,
    Tagged,
    Queued,
    Processing,
    Processed,
    Failed,
    Skipped,
    Deleted,
}

impl FileStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Tagged => "tagged",
            Self::Queued => "queued",
            Self::Processing => "processing",
            Self::Processed => "processed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Deleted => "deleted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "tagged" => Some(Self::Tagged),
            "queued" => Some(Self::Queued),
            "processing" => Some(Self::Processing),
            "processed" => Some(Self::Processed),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }
}

impl std::fmt::Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A file discovered by the scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedFile {
    pub id: Option<i64>,
    pub source_id: String,
    pub path: String,
    pub rel_path: String,
    pub size: u64,
    pub mtime: i64,
    pub content_hash: Option<String>,
    pub status: FileStatus,
    pub tag: Option<String>,
    pub tag_source: Option<String>,
    pub rule_id: Option<String>,
    pub manual_plugin: Option<String>,
    pub error: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
    pub sentinel_job_id: Option<i64>,
}

impl ScannedFile {
    pub fn new(source_id: &str, path: &str, rel_path: &str, size: u64, mtime: i64) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            source_id: source_id.to_string(),
            path: path.to_string(),
            rel_path: rel_path.to_string(),
            size,
            mtime,
            content_hash: None,
            status: FileStatus::Pending,
            tag: None,
            tag_source: None,
            rule_id: None,
            manual_plugin: None,
            error: None,
            first_seen_at: now,
            last_seen_at: now,
            processed_at: None,
            sentinel_job_id: None,
        }
    }

    pub fn is_manual(&self) -> bool {
        self.tag_source.as_deref() == Some("manual") || self.manual_plugin.is_some()
    }
}

/// Statistics from the database
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScoutStats {
    pub total_sources: u64,
    pub total_tagging_rules: u64,
    pub total_files: u64,
    pub files_pending: u64,
    pub files_tagged: u64,
    pub files_queued: u64,
    pub files_processing: u64,
    pub files_processed: u64,
    pub files_failed: u64,
    pub bytes_pending: u64,
    pub bytes_processed: u64,
}

/// Result of upserting a file
#[derive(Debug, Clone, Copy)]
pub struct UpsertResult {
    pub id: i64,
    pub is_new: bool,
    pub is_changed: bool,
}

// ============================================================================
// Parser Lab Types
// ============================================================================

/// A parser in the Parser Lab
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parser {
    pub id: String,
    pub name: String,
    pub file_pattern: String,
    pub pattern_type: Option<String>,
    pub source_code: Option<String>,
    pub source_hash: Option<String>,
    pub validation_status: Option<String>,
    pub validation_error: Option<String>,
    pub validation_output: Option<String>,
    pub schema_json: Option<String>,
    pub messages_json: Option<String>,
    pub sink_type: Option<String>,
    pub sink_config_json: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub published_plugin_id: Option<String>,
    pub is_sample: bool,
    pub output_mode: Option<String>,
    pub detected_topics_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A test file for a parser
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserTestFile {
    pub id: String,
    pub parser_id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: Option<i64>,
    pub created_at: DateTime<Utc>,
}

/// Parser validation status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationStatus {
    Pending,
    Valid,
    Invalid,
}

impl ValidationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Valid => "valid",
            Self::Invalid => "invalid",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "valid" => Some(Self::Valid),
            "invalid" => Some(Self::Invalid),
            _ => None,
        }
    }
}

// ============================================================================
// Sentinel/Job Types
// ============================================================================

/// Status of a processing job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum JobStatus {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Queued => "QUEUED",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Skipped => "SKIPPED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "PENDING" => Some(Self::Pending),
            "QUEUED" => Some(Self::Queued),
            "RUNNING" => Some(Self::Running),
            "COMPLETED" => Some(Self::Completed),
            "FAILED" => Some(Self::Failed),
            "SKIPPED" => Some(Self::Skipped),
            _ => None,
        }
    }

    /// Parse from string, converting unknown values to a default
    pub fn from_str(s: &str) -> Self {
        Self::parse(s).unwrap_or(Self::Queued)
    }
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A job in the processing queue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: i64,
    pub file_version_id: Option<i64>,
    pub plugin_name: String,
    pub input_file: Option<String>,
    pub status: JobStatus,
    pub priority: i32,
    pub config_overrides: Option<String>,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub claim_time: Option<String>,
    pub end_time: Option<String>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub logs: Option<String>,
}

/// A plugin manifest entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: i64,
    pub plugin_name: String,
    pub version: String,
    pub source_code: String,
    pub source_hash: String,
    pub env_hash: Option<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub deployed_at: Option<String>,
}

/// Plugin configuration (subscriptions)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfig {
    pub id: i64,
    pub plugin_name: String,
    pub subscription_tags: String,
    pub default_parameters: Option<String>,
    pub enabled: bool,
}

/// Plugin environment (for venv management)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEnvironment {
    pub hash: String,
    pub lockfile_content: String,
    pub created_at: Option<String>,
}

// ============================================================================
// Filter Types (for queries)
// ============================================================================

/// Filter for listing files
#[derive(Debug, Clone, Default)]
pub struct FileFilter {
    pub source_id: Option<String>,
    pub status: Option<FileStatus>,
    pub tag: Option<String>,
    pub untagged_only: bool,
    pub limit: Option<usize>,
}

/// Filter for listing jobs
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub status: Option<JobStatus>,
    pub plugin_name: Option<String>,
    pub limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_status_roundtrip() {
        for status in [
            FileStatus::Pending,
            FileStatus::Tagged,
            FileStatus::Queued,
            FileStatus::Processing,
            FileStatus::Processed,
            FileStatus::Failed,
            FileStatus::Skipped,
            FileStatus::Deleted,
        ] {
            let s = status.as_str();
            let parsed = FileStatus::parse(s).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_job_status_roundtrip() {
        for status in [
            JobStatus::Pending,
            JobStatus::Queued,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Skipped,
        ] {
            let s = status.as_str();
            let parsed = JobStatus::parse(s).unwrap();
            assert_eq!(status, parsed);
        }
    }
}
