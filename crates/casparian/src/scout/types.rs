//! Core types for the Scout system
//!
//! Scout is the **File Discovery + Tagging** layer.
//! It watches folders, discovers files, and assigns tags.
//! Actual processing happens in Sentinel (Tag → Plugin → Sink).

pub use casparian_ids::IdParseError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

// ============================================================================
// Identifier Types
// ============================================================================

fn parse_i64_id(label: &str, value: &str) -> Result<i64, IdParseError> {
    let id = value
        .parse::<i64>()
        .map_err(|e| IdParseError::new(format!("Invalid {}: {}", label, e)))?;
    validate_i64_id(label, id)
}

fn validate_i64_id(label: &str, value: i64) -> Result<i64, IdParseError> {
    if value <= 0 {
        return Err(IdParseError::new(format!(
            "Invalid {}: must be positive",
            label
        )));
    }
    Ok(value)
}

fn new_random_id() -> i64 {
    let raw = Uuid::new_v4().as_u128();
    let id = (raw & 0x7fff_ffff_ffff_ffff) as i64;
    if id == 0 {
        1
    } else {
        id
    }
}

/// Unique identifier for a workspace (UUID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceId(Uuid);

impl WorkspaceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, IdParseError> {
        let parsed = Uuid::parse_str(value)
            .map_err(|e| IdParseError::new(format!("Invalid workspace ID: {}", e)))?;
        Ok(Self(parsed))
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for WorkspaceId {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        WorkspaceId::parse(s)
    }
}

impl TryFrom<String> for WorkspaceId {
    type Error = IdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        WorkspaceId::parse(&value)
    }
}

impl Serialize for WorkspaceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = self.0.to_string();
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for WorkspaceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = WorkspaceId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("UUID workspace ID")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                WorkspaceId::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Unique identifier for a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(i64);

impl SourceId {
    pub fn new() -> Self {
        Self(new_random_id())
    }

    pub fn parse(value: &str) -> Result<Self, IdParseError> {
        Ok(Self(parse_i64_id("source ID", value)?))
    }

    pub fn as_i64(&self) -> i64 {
        self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SourceId {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SourceId::parse(s)
    }
}

impl TryFrom<String> for SourceId {
    type Error = IdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        SourceId::parse(&value)
    }
}

impl TryFrom<i64> for SourceId {
    type Error = IdParseError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Ok(Self(validate_i64_id("source ID", value)?))
    }
}

impl Serialize for SourceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for SourceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = SourceId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("positive integer source ID")
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                SourceId::try_from(value).map_err(E::custom)
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let value = i64::try_from(value).map_err(E::custom)?;
                SourceId::try_from(value).map_err(E::custom)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                SourceId::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Unique identifier for a tagging rule (UUID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaggingRuleId(Uuid);

impl TaggingRuleId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, IdParseError> {
        let parsed = Uuid::parse_str(value)
            .map_err(|e| IdParseError::new(format!("Invalid tagging rule ID: {}", e)))?;
        Ok(Self(parsed))
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for TaggingRuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TaggingRuleId {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        TaggingRuleId::parse(s)
    }
}

impl TryFrom<String> for TaggingRuleId {
    type Error = IdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        TaggingRuleId::parse(&value)
    }
}

impl Serialize for TaggingRuleId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = self.0.to_string();
        serializer.serialize_str(&value)
    }
}

impl<'de> Deserialize<'de> for TaggingRuleId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = TaggingRuleId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("UUID tagging rule ID")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                TaggingRuleId::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

// ============================================================================
// Source Types
// ============================================================================

/// A workspace (case) that scopes sources, files, and rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// Unique identifier
    pub id: WorkspaceId,
    /// Human-readable name
    pub name: String,
    /// When the workspace was created
    pub created_at: DateTime<Utc>,
}

/// A source location to watch for files
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// Workspace this source belongs to
    pub workspace_id: WorkspaceId,
    /// Unique identifier
    pub id: SourceId,
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
    pub id: TaggingRuleId,
    /// Human-readable name
    pub name: String,
    /// Workspace ID this rule applies to
    pub workspace_id: WorkspaceId,
    /// Glob pattern to match files (e.g., "*.csv", "data/**/*.json")
    pub pattern: String,
    /// Tag to assign to matching files
    pub tag: String,
    /// Priority (higher = evaluated first)
    pub priority: i32,
    /// Whether this rule is enabled
    pub enabled: bool,
}

/// How a tag was assigned to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TagSource {
    Rule,
    Manual,
}

impl TagSource {
    pub const ALL: &'static [TagSource] = &[TagSource::Rule, TagSource::Manual];

    pub fn as_str(&self) -> &'static str {
        match self {
            TagSource::Rule => "rule",
            TagSource::Manual => "manual",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "rule" => Some(TagSource::Rule),
            "manual" => Some(TagSource::Manual),
            _ => None,
        }
    }
}

/// A tag assigned to a file (multi-tag capable).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTag {
    pub tag: String,
    pub tag_source: TagSource,
    pub rule_id: Option<TaggingRuleId>,
    pub assigned_at: DateTime<Utc>,
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
    pub const ALL: &'static [FileStatus] = &[
        FileStatus::Pending,
        FileStatus::Tagged,
        FileStatus::Queued,
        FileStatus::Processing,
        FileStatus::Processed,
        FileStatus::Failed,
        FileStatus::Skipped,
        FileStatus::Deleted,
    ];

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
    /// Workspace ID this file belongs to
    pub workspace_id: WorkspaceId,
    /// Source ID this file belongs to
    pub source_id: SourceId,
    /// Stable identity for move/rename detection (strength encoded in prefix)
    pub file_uid: String,
    /// Full path to the file
    pub path: String,
    /// Relative path from source root
    pub rel_path: String,
    /// Parent directory path (for O(1) folder navigation)
    /// e.g., "a/b/c.txt" → parent_path = "a/b"
    pub parent_path: String,
    /// Filename only (basename of rel_path)
    /// e.g., "a/b/c.txt" → name = "c.txt"
    pub name: String,
    /// File extension (lowercase, without dot)
    /// e.g., "csv", "json", "rs". None for files without extension.
    pub extension: Option<String>,
    /// True if this row represents a directory entry
    pub is_dir: bool,
    /// File size in bytes
    pub size: u64,
    /// Last modification time (Unix timestamp milliseconds)
    pub mtime: i64,
    /// Content hash (optional, for deduplication)
    pub content_hash: Option<String>,
    /// Current status
    pub status: FileStatus,
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

/// Split a relative path into (parent_path, name).
/// - "a/b/c.txt" → ("a/b", "c.txt")
/// - "file.txt"  → ("", "file.txt")
/// - ""          → ("", "")
fn split_rel_path(rel_path: &str) -> (&str, &str) {
    match rel_path.rfind('/') {
        Some(idx) => (&rel_path[..idx], &rel_path[idx + 1..]),
        None => ("", rel_path),
    }
}

/// Extract file extension from filename.
/// - "file.csv" → Some("csv")
/// - "file.tar.gz" → Some("gz")
/// - ".gitignore" → None (dotfiles without extension)
/// - "README" → None
fn extract_extension(name: &str) -> Option<String> {
    name.rsplit_once('.')
        .filter(|(base, _)| !base.is_empty()) // Skip dotfiles like ".gitignore"
        .map(|(_, ext)| ext.to_lowercase())
}

impl ScannedFile {
    /// Create a new pending file
    pub fn new(
        workspace_id: WorkspaceId,
        source_id: SourceId,
        file_uid: &str,
        path: &str,
        rel_path: &str,
        size: u64,
        mtime: i64,
    ) -> Self {
        let now = Utc::now();
        let (parent_path, name) = split_rel_path(rel_path);
        let extension = extract_extension(name);
        Self {
            id: None,
            workspace_id,
            source_id,
            file_uid: file_uid.to_string(),
            path: path.to_string(),
            rel_path: rel_path.to_string(),
            parent_path: parent_path.to_string(),
            name: name.to_string(),
            extension,
            is_dir: false,
            size,
            mtime,
            content_hash: None,
            status: FileStatus::Pending,
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
    /// For source_id, pass the SourceId for this scan to avoid string duplication.
    pub fn from_parts(
        workspace_id: WorkspaceId,
        source_id: SourceId,
        file_uid: String,
        path: String,
        rel_path: String,
        size: u64,
        mtime: i64,
    ) -> Self {
        let now = Utc::now();
        Self::from_parts_with_now(
            workspace_id,
            source_id,
            file_uid,
            path,
            rel_path,
            size,
            mtime,
            now,
        )
    }

    /// F-007: Create from pre-allocated strings with a provided timestamp.
    ///
    /// This avoids per-file `Utc::now()` calls in hot paths while keeping
    /// timestamps consistent within a scan batch or thread.
    pub fn from_parts_with_now(
        workspace_id: WorkspaceId,
        source_id: SourceId,
        file_uid: String,
        path: String,
        rel_path: String,
        size: u64,
        mtime: i64,
        now: DateTime<Utc>,
    ) -> Self {
        let last_seen_at = now.clone();
        let (parent_path, name) = split_rel_path(&rel_path);
        let extension = extract_extension(name);
        Self {
            id: None,
            workspace_id,
            source_id,
            file_uid,
            path,
            parent_path: parent_path.to_string(),
            name: name.to_string(),
            extension,
            rel_path,
            is_dir: false,
            size,
            mtime,
            content_hash: None,
            status: FileStatus::Pending,
            manual_plugin: None,
            error: None,
            first_seen_at: now,
            last_seen_at,
            processed_at: None,
            sentinel_job_id: None,
            // Extractor metadata defaults (Phase 6)
            metadata_raw: None,
            extraction_status: ExtractionStatus::Pending,
            extracted_at: None,
        }
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
    pub const ALL: &'static [ExtractionStatus] = &[
        ExtractionStatus::Pending,
        ExtractionStatus::Extracted,
        ExtractionStatus::Timeout,
        ExtractionStatus::Crash,
        ExtractionStatus::Stale,
        ExtractionStatus::Error,
    ];

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
    pub const ALL: &'static [ExtractionLogStatus] = &[
        ExtractionLogStatus::Success,
        ExtractionLogStatus::Timeout,
        ExtractionLogStatus::Crash,
        ExtractionLogStatus::Error,
    ];

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
// Parser Lab Types
// ============================================================================

/// Validation status for a parser in Parser Lab
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParserValidationStatus {
    /// Not yet validated
    Pending,
    /// Passed validation
    Valid,
    /// Failed validation (see validation_error for details)
    Invalid,
    /// Error during validation (system error)
    Error,
}

impl ParserValidationStatus {
    pub const ALL: &'static [ParserValidationStatus] = &[
        ParserValidationStatus::Pending,
        ParserValidationStatus::Valid,
        ParserValidationStatus::Invalid,
        ParserValidationStatus::Error,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "valid" => Some(Self::Valid),
            "invalid" => Some(Self::Invalid),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl Default for ParserValidationStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl fmt::Display for ParserValidationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Processing Types
// ============================================================================

/// Statistics from a scan operation
#[derive(Debug, Clone, Default)]
pub struct ScanStats {
    /// Number of directories scanned
    pub dirs_scanned: u64,
    /// GAP-SCAN-005: Number of files found by walker (regardless of persist success)
    pub files_discovered: u64,
    /// GAP-SCAN-005: Number of files successfully persisted to database
    pub files_persisted: u64,
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
pub struct DbStats {
    pub total_workspaces: u64,
    pub total_sources: u64,
    pub total_tagging_rules: u64,
    pub total_tags: u64,
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

/// GAP-SCAN-004: Statistics from populating the folder cache
#[derive(Debug, Clone, Default)]
pub struct FolderCacheStats {
    /// Number of folders stored in cache
    pub folders_cached: u64,
    /// Number of files stored in cache (root-level only)
    pub files_cached: u64,
    /// Number of folders that were truncated (not stored)
    pub folders_truncated: u64,
    /// Number of files that were truncated (not stored)
    pub files_truncated: u64,
    /// True if any truncation occurred
    pub is_truncated: bool,
}

/// GAP-SCAN-004: Truncation info for folder cache
#[derive(Debug, Clone, Default)]
pub struct FolderCacheTruncation {
    /// Number of folders not shown due to limit
    pub folders_truncated: u64,
    /// Number of files not shown due to limit
    pub files_truncated: u64,
    /// Total number of folders in source
    pub total_folders: u64,
    /// Total number of root-level files in source
    pub total_files: u64,
}

impl FolderCacheTruncation {
    /// Returns true if any truncation occurred
    pub fn is_truncated(&self) -> bool {
        self.folders_truncated > 0 || self.files_truncated > 0
    }

    /// Returns a human-readable summary like "+50 more folders, +100 more files"
    pub fn summary(&self) -> Option<String> {
        if !self.is_truncated() {
            return None;
        }
        let mut parts = Vec::new();
        if self.folders_truncated > 0 {
            parts.push(format!("+{} more folders", self.folders_truncated));
        }
        if self.files_truncated > 0 {
            parts.push(format!("+{} more files", self.files_truncated));
        }
        Some(parts.join(", "))
    }
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
        let workspace_id = WorkspaceId::new();
        let rule_id = TaggingRuleId::new();
        let rule = TaggingRule {
            id: rule_id,
            name: "CSV Files".to_string(),
            workspace_id,
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
