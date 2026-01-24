//! Scout - File Discovery & Tagging
//!
//! Scout discovers files from filesystem sources and assigns tags based on patterns.
//! This is the local module, consolidated from the former casparian_scout crate.

pub mod db;
pub mod error;
pub mod extractor;
pub mod patterns;
pub mod rule_apply;
pub mod scan_path;
pub mod scanner;
pub mod tagger;
pub mod types;

// Re-exports for CLI usage
pub use db::Database;
pub use extractor::{BatchExtractor, ExtractorConfig, ExtractorResult, ExtractorRunner};
pub use patterns::{build_matcher, matches, normalize_glob_pattern};
pub use rule_apply::{
    match_rules_to_files, RuleApplyFile, RuleApplyRule, RuleMatch, TaggingSummary,
};
pub use scanner::{ScanCancelToken, ScanConfig, ScanProgress, Scanner};
pub use types::{
    ExtractionStatus, Extractor, FileStatus, FileTag, ScannedFile, Source, SourceId, SourceType,
    TagSource, TaggingRule, TaggingRuleId, Workspace, WorkspaceId,
};
