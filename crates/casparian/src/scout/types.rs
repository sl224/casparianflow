//! Core types for the Scout system
//!
//! Scout is the **File Discovery + Tagging** layer.
//! It watches folders, discovers files, and assigns tags.
//! Actual processing happens in Sentinel (Tag → Plugin → Sink).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ============================================================================
// Serde helpers for Arc<str>
// ============================================================================

/// Custom serde for Arc<str> - serializes as String, deserializes efficiently
mod arc_str_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::sync::Arc;

    pub fn serialize<S>(value: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Arc::from(s))
    }
}

// ============================================================================
// Source Types
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

// ============================================================================
// Tagging Rule Types
// ============================================================================

/// A tagging rule maps file patterns to tags
///
/// When a file matches the pattern, it gets assigned the tag.
/// The tag determines which plugin processes the file (via Sentinel).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggingRule {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Source ID this rule applies to
    pub source_id: String,
    /// Glob pattern to match files (e.g., "*.csv", "data/**/*.json")
    pub pattern: String,
    /// Tag to assign to matching files
    pub tag: String,
    /// Priority (higher = evaluated first)
    pub priority: i32,
    /// Whether this rule is enabled
    pub enabled: bool,
}

// ============================================================================
// File Types
// ============================================================================

/// Status of a discovered file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    /// File discovered, awaiting tagging
    Pending,
    /// File has been tagged, awaiting processing
    Tagged,
    /// File has been submitted to processing queue
    Queued,
    /// File is being processed by a worker
    Processing,
    /// File has been successfully processed
    Processed,
    /// File processing failed
    Failed,
    /// File was skipped (user decision or no matching rule)
    Skipped,
    /// File was deleted from source
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

/// A file discovered by the scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedFile {
    /// Database ID (None if not yet persisted)
    pub id: Option<i64>,
    /// Source ID this file belongs to
    /// PERF: Uses Arc<str> to share source_id across all files in a scan,
    /// eliminating 1M allocations when scanning 1M files.
    #[serde(with = "arc_str_serde")]
    pub source_id: Arc<str>,
    /// Full path to the file
    pub path: String,
    /// Relative path from source root
    pub rel_path: String,
    /// File size in bytes
    pub size: u64,
    /// Last modification time (Unix timestamp milliseconds)
    pub mtime: i64,
    /// Content hash (optional, for deduplication)
    pub content_hash: Option<String>,
    /// Current status
    pub status: FileStatus,
    /// Assigned tag (None = untagged)
    pub tag: Option<String>,
    /// How the tag was assigned: "rule" (auto) or "manual"
    pub tag_source: Option<String>,
    /// ID of the tagging rule that matched (if tag_source = "rule")
    pub rule_id: Option<String>,
    /// Manual plugin override (None = use tag subscription)
    pub manual_plugin: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// When the file was first discovered
    pub first_seen_at: DateTime<Utc>,
    /// When the file was last seen in a scan
    pub last_seen_at: DateTime<Utc>,
    /// When the file was processed (if applicable)
    pub processed_at: Option<DateTime<Utc>>,
    /// Sentinel job ID if submitted for processing
    pub sentinel_job_id: Option<i64>,
    // --- Extractor metadata (Phase 6) ---
    /// Raw extracted metadata as JSON blob
    pub metadata_raw: Option<String>,
    /// Extraction status
    pub extraction_status: ExtractionStatus,
    /// When metadata was last extracted
    pub extracted_at: Option<DateTime<Utc>>,
}

impl ScannedFile {
    /// Create a new pending file
    pub fn new(source_id: &str, path: &str, rel_path: &str, size: u64, mtime: i64) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            source_id: Arc::from(source_id),
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
            // Extractor metadata defaults (Phase 6)
            metadata_raw: None,
            extraction_status: ExtractionStatus::Pending,
            extracted_at: None,
        }
    }

    /// F-007: Create from pre-allocated strings to avoid redundant allocations
    ///
    /// Use this in hot paths where strings are already owned (e.g., scanner).
    /// For source_id, use Arc<str> to share across all files in a scan.
    /// PERF: No allocation - Arc is cloned (ref count bump only).
    pub fn from_parts(
        source_id: Arc<str>,
        path: String,
        rel_path: String,
        size: u64,
        mtime: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            source_id, // PERF: No allocation - just Arc clone (ref count bump)
            path,
            rel_path,
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
            // Extractor metadata defaults (Phase 6)
            metadata_raw: None,
            extraction_status: ExtractionStatus::Pending,
            extracted_at: None,
        }
    }

    /// Check if this file has any manual overrides (manual tag or manual plugin)
    #[allow(dead_code)] // Will be used for processing integration
    pub fn is_manual(&self) -> bool {
        self.tag_source.as_deref() == Some("manual") || self.manual_plugin.is_some()
    }
}

// ============================================================================
// Extractor Types (Phase 6)
// ============================================================================

/// Status of metadata extraction for a file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractionStatus {
    /// Not yet extracted
    Pending,
    /// Successfully extracted
    Extracted,
    /// Extraction timed out
    Timeout,
    /// Extractor crashed
    Crash,
    /// Metadata stale (extractor changed since extraction)
    Stale,
    /// Extraction failed with error
    Error,
}

impl ExtractionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Extracted => "extracted",
            Self::Timeout => "timeout",
            Self::Crash => "crash",
            Self::Stale => "stale",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "extracted" => Some(Self::Extracted),
            "timeout" => Some(Self::Timeout),
            "crash" => Some(Self::Crash),
            "stale" => Some(Self::Stale),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl Default for ExtractionStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// A Python extractor that extracts metadata from file paths
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extractor {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Path to the Python source file
    pub source_path: String,
    /// SHA-256 hash of the source code
    pub source_hash: String,
    /// Whether this extractor is enabled
    pub enabled: bool,
    /// Timeout per file in seconds
    pub timeout_secs: u32,
    /// Number of consecutive failures (for fail-fast)
    pub consecutive_failures: u32,
    /// When the extractor was auto-paused (None = not paused)
    pub paused_at: Option<DateTime<Utc>>,
    /// When the extractor was created
    pub created_at: DateTime<Utc>,
    /// When the extractor was last updated
    pub updated_at: DateTime<Utc>,
}

impl Extractor {
    /// Check if extractor is paused due to failures
    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }
}

/// Log entry for extractor execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionLogEntry {
    /// Log entry ID
    pub id: i64,
    /// File ID that was processed
    pub file_id: i64,
    /// Extractor ID that ran
    pub extractor_id: String,
    /// Result status
    pub status: ExtractionLogStatus,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Snapshot of extracted metadata
    pub metadata_snapshot: Option<String>,
    /// When the extraction ran
    pub executed_at: DateTime<Utc>,
}

/// Status of an extraction log entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractionLogStatus {
    Success,
    Timeout,
    Crash,
    Error,
}

impl ExtractionLogStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Timeout => "timeout",
            Self::Crash => "crash",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "success" => Some(Self::Success),
            "timeout" => Some(Self::Timeout),
            "crash" => Some(Self::Crash),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

// ============================================================================
// Processing Types
// ============================================================================

/// Statistics from a scan operation
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Fields used in scan reporting
pub struct ScanStats {
    /// Number of directories scanned
    pub dirs_scanned: u64,
    /// Number of files discovered
    pub files_discovered: u64,
    /// Number of new files
    pub files_new: u64,
    /// Number of changed files
    pub files_changed: u64,
    /// Number of unchanged files
    pub files_unchanged: u64,
    /// Number of deleted files (no longer present)
    pub files_deleted: u64,
    /// Total bytes scanned
    pub bytes_scanned: u64,
    /// Number of errors encountered
    pub errors: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Result of upserting a file into the database
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Used in internal scan operations
pub struct UpsertResult {
    /// Database ID of the file
    pub id: i64,
    /// True if this is a new file (first time seen)
    pub is_new: bool,
    /// True if the file was modified since last scan
    pub is_changed: bool,
}

/// Result of batch upserting multiple files into the database
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchUpsertResult {
    /// Count of new files (first time seen)
    pub new: u64,
    /// Count of changed files (modified since last scan)
    pub changed: u64,
    /// Count of unchanged files
    pub unchanged: u64,
    /// Count of files that failed to upsert
    pub errors: u64,
}

/// Statistics from the database
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Will be used for status reporting
pub struct DbStats {
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
    fn test_file_status_parse_unknown() {
        assert!(FileStatus::parse("invalid").is_none());
        assert!(FileStatus::parse("").is_none());
    }

    #[test]
    fn test_file_status_case_insensitive() {
        assert_eq!(FileStatus::parse("PENDING"), Some(FileStatus::Pending));
        assert_eq!(FileStatus::parse("Tagged"), Some(FileStatus::Tagged));
    }

    #[test]
    fn test_tagging_rule_serialization() {
        let rule = TaggingRule {
            id: "rule-1".to_string(),
            name: "CSV Files".to_string(),
            source_id: "src-1".to_string(),
            pattern: "*.csv".to_string(),
            tag: "csv_data".to_string(),
            priority: 10,
            enabled: true,
        };

        let json = serde_json::to_string(&rule).unwrap();
        let parsed: TaggingRule = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tag, "csv_data");
        assert_eq!(parsed.priority, 10);
    }
}
