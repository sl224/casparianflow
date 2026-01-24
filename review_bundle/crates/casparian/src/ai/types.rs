//! Core types for AI Wizards
//!
//! These types are used across all wizard implementations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Unique identifier for a draft artifact
///
/// Format: 8-character hex string (first 8 chars of UUID)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DraftId(pub String);

impl DraftId {
    /// Generate a new random draft ID
    pub fn new() -> Self {
        let uuid = uuid::Uuid::new_v4();
        Self(uuid.simple().to_string()[..8].to_string())
    }

    /// Create from existing string
    pub fn from_str(s: &str) -> Self {
        Self(s.to_string())
    }

    /// Get the string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for DraftId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DraftId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type of draft artifact
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftType {
    /// YAML extraction rule or Python extractor (from Pathfinder Wizard)
    Extractor,
    /// Python parser code (from Parser Lab)
    Parser,
    /// Semantic tag label (from Labeling Wizard)
    Label,
    /// YAML extraction rule from semantic path analysis
    SemanticRule,
}

impl DraftType {
    /// Get string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            DraftType::Extractor => "extractor",
            DraftType::Parser => "parser",
            DraftType::Label => "label",
            DraftType::SemanticRule => "semantic_rule",
        }
    }

    /// Parse from database string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "extractor" => Some(DraftType::Extractor),
            "parser" => Some(DraftType::Parser),
            "label" => Some(DraftType::Label),
            "semantic_rule" => Some(DraftType::SemanticRule),
            _ => None,
        }
    }
}

impl std::fmt::Display for DraftType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Status of a draft artifact
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    /// Awaiting user review
    Pending,
    /// User approved, committed to runtime
    Approved,
    /// User rejected
    Rejected,
    /// Automatically expired after 24 hours
    Expired,
}

impl DraftStatus {
    /// Get string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            DraftStatus::Pending => "pending",
            DraftStatus::Approved => "approved",
            DraftStatus::Rejected => "rejected",
            DraftStatus::Expired => "expired",
        }
    }

    /// Parse from database string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(DraftStatus::Pending),
            "approved" => Some(DraftStatus::Approved),
            "rejected" => Some(DraftStatus::Rejected),
            "expired" => Some(DraftStatus::Expired),
            _ => None,
        }
    }
}

impl std::fmt::Display for DraftStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Type of AI wizard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WizardType {
    /// Pathfinder: Generate extraction rules from file paths
    Pathfinder,
    /// Parser Lab: Generate parsers from sample files
    ParserLab,
    /// Labeling: Suggest semantic tag names
    Labeling,
    /// Semantic Path: Recognize folder structure patterns
    SemanticPath,
}

impl WizardType {
    /// Get string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            WizardType::Pathfinder => "pathfinder",
            WizardType::ParserLab => "parser_lab",
            WizardType::Labeling => "labeling",
            WizardType::SemanticPath => "semantic_path",
        }
    }

    /// Parse from database string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pathfinder" => Some(WizardType::Pathfinder),
            "parser_lab" => Some(WizardType::ParserLab),
            "labeling" => Some(WizardType::Labeling),
            "semantic_path" => Some(WizardType::SemanticPath),
            _ => None,
        }
    }
}

impl std::fmt::Display for WizardType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Context provided when creating a draft
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DraftContext {
    /// Sample file paths used for analysis
    pub sample_paths: Vec<PathBuf>,
    /// User-provided hints for generation
    pub user_hints: Option<String>,
    /// Source ID if analyzing a specific source
    pub source_id: Option<String>,
    /// Tag name for the generated rule
    pub tag_name: Option<String>,
}

/// A draft artifact awaiting approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    /// Unique identifier
    pub id: DraftId,
    /// Type of artifact
    pub draft_type: DraftType,
    /// Path to the draft file (YAML or Python)
    pub file_path: PathBuf,
    /// Current status
    pub status: DraftStatus,
    /// Context used for generation
    pub source_context: DraftContext,
    /// Model used for generation (if AI-assisted)
    pub model_name: Option<String>,
    /// When the draft was created
    pub created_at: DateTime<Utc>,
    /// When the draft expires (24h from creation)
    pub expires_at: DateTime<Utc>,
    /// When the draft was approved (if approved)
    pub approved_at: Option<DateTime<Utc>>,
    /// Who approved the draft
    pub approved_by: Option<String>,
}

impl Draft {
    /// Check if the draft has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if the draft is still pending
    pub fn is_pending(&self) -> bool {
        self.status == DraftStatus::Pending && !self.is_expired()
    }

    /// Get the draft content by reading from file
    pub fn read_content(&self) -> std::io::Result<String> {
        std::fs::read_to_string(&self.file_path)
    }
}

/// Status of an audit log entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditStatus {
    /// LLM call succeeded
    Success,
    /// LLM call timed out
    Timeout,
    /// LLM call failed with error
    Error,
    /// Retrying after failure
    Retry,
}

impl AuditStatus {
    /// Get string representation for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditStatus::Success => "success",
            AuditStatus::Timeout => "timeout",
            AuditStatus::Error => "error",
            AuditStatus::Retry => "retry",
        }
    }

    /// Parse from database string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "success" => Some(AuditStatus::Success),
            "timeout" => Some(AuditStatus::Timeout),
            "error" => Some(AuditStatus::Error),
            "retry" => Some(AuditStatus::Retry),
            _ => None,
        }
    }
}

impl std::fmt::Display for AuditStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draft_id_generation() {
        let id1 = DraftId::new();
        let id2 = DraftId::new();
        assert_ne!(id1, id2);
        assert_eq!(id1.as_str().len(), 8);
    }

    #[test]
    fn test_draft_type_roundtrip() {
        for dt in [
            DraftType::Extractor,
            DraftType::Parser,
            DraftType::Label,
            DraftType::SemanticRule,
        ] {
            let s = dt.as_str();
            let parsed = DraftType::from_str(s).unwrap();
            assert_eq!(dt, parsed);
        }
    }

    #[test]
    fn test_draft_status_roundtrip() {
        for ds in [
            DraftStatus::Pending,
            DraftStatus::Approved,
            DraftStatus::Rejected,
            DraftStatus::Expired,
        ] {
            let s = ds.as_str();
            let parsed = DraftStatus::from_str(s).unwrap();
            assert_eq!(ds, parsed);
        }
    }

    #[test]
    fn test_wizard_type_roundtrip() {
        for wt in [
            WizardType::Pathfinder,
            WizardType::ParserLab,
            WizardType::Labeling,
            WizardType::SemanticPath,
        ] {
            let s = wt.as_str();
            let parsed = WizardType::from_str(s).unwrap();
            assert_eq!(wt, parsed);
        }
    }
}
