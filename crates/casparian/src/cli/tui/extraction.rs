//! Extraction Rule Types for Glob Explorer
//!
//! Types for defining, testing, and publishing extraction rules via the TUI.
//! Aligned with specs/views/discover.md v2.2 Phase 18 and specs/extraction.md.
//!
//! # Dead Code Justification
//! This module contains scaffolding types for the Rule Builder TUI feature
//! currently under active development. Types are defined ahead of their UI
//! integration to establish stable contracts. See specs/rule_builder.md.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use uuid::Uuid;

// =============================================================================
// Rule Draft Types
// =============================================================================

/// TUI working draft - editable in UI before persisting to database
#[derive(Debug, Clone, Default)]
pub struct RuleDraft {
    /// UUID for existing rules, None for new
    pub id: Option<Uuid>,
    /// Scoped to source, or None for global
    pub source_id: Option<Uuid>,
    /// Rule name (required for save)
    pub name: String,
    /// Glob pattern for matching files
    pub glob_pattern: String,
    /// Extraction fields
    pub fields: Vec<FieldDraft>,
    /// Base tag applied to all matching files
    pub base_tag: String,
    /// Conditional tag assignments
    pub tag_conditions: Vec<TagConditionDraft>,
    /// Rule priority (higher = matched first)
    pub priority: i32,
    /// Whether rule is enabled
    pub enabled: bool,
}

impl RuleDraft {
    /// Create a new rule draft from a glob pattern
    pub fn from_pattern(pattern: &str, source_id: Option<Uuid>) -> Self {
        Self {
            id: None,
            source_id,
            name: String::new(),
            glob_pattern: pattern.to_string(),
            fields: Vec::new(),
            base_tag: String::new(),
            tag_conditions: Vec::new(),
            priority: 100,
            enabled: true,
        }
    }

    /// Check if the draft has a name (required for save)
    pub fn has_name(&self) -> bool {
        !self.name.trim().is_empty()
    }

    /// Check if the draft is valid for testing
    pub fn is_valid_for_test(&self) -> bool {
        !self.glob_pattern.is_empty()
    }

    /// Check if the draft is valid for publishing
    pub fn is_valid_for_publish(&self) -> bool {
        self.has_name() && !self.glob_pattern.is_empty()
    }
}

/// Field extraction definition
#[derive(Debug, Clone)]
pub struct FieldDraft {
    /// Field name (column name in output)
    pub name: String,
    /// Where to extract value from
    pub source: FieldSource,
    /// Regex pattern for extraction (optional)
    pub pattern: Option<String>,
    /// Type hint for parsing
    pub type_hint: FieldType,
    /// Optional normalizer
    pub normalizer: Option<Normalizer>,
    /// Default value if extraction fails
    pub default_value: Option<String>,
}

impl Default for FieldDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            source: FieldSource::Filename,
            pattern: None,
            type_hint: FieldType::String,
            normalizer: None,
            default_value: None,
        }
    }
}

/// Source of field value extraction
#[derive(Debug, Clone, PartialEq)]
pub enum FieldSource {
    /// Extract from path segment at index (negative = from end)
    Segment(i32),
    /// Extract from filename only
    Filename,
    /// Extract from full path
    FullPath,
    /// Extract from relative path
    RelPath,
}

impl FieldSource {
    /// Convert to database string format
    pub fn to_db_format(&self) -> (&'static str, Option<String>) {
        match self {
            FieldSource::Segment(n) => ("segment", Some(n.to_string())),
            FieldSource::Filename => ("filename", None),
            FieldSource::FullPath => ("full_path", None),
            FieldSource::RelPath => ("rel_path", None),
        }
    }

    /// Parse from database format
    pub fn from_db_format(source_type: &str, source_value: Option<&str>) -> Option<Self> {
        match source_type {
            "segment" => {
                let n: i32 = source_value?.parse().ok()?;
                Some(FieldSource::Segment(n))
            }
            "filename" => Some(FieldSource::Filename),
            "full_path" => Some(FieldSource::FullPath),
            "rel_path" => Some(FieldSource::RelPath),
            _ => None,
        }
    }

    /// Display name for UI
    pub fn display_name(&self) -> String {
        match self {
            FieldSource::Segment(n) => format!("segment({})", n),
            FieldSource::Filename => "filename".to_string(),
            FieldSource::FullPath => "full_path".to_string(),
            FieldSource::RelPath => "rel_path".to_string(),
        }
    }
}

/// Type hint for field parsing
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FieldType {
    #[default]
    String,
    Integer,
    Date,
    Uuid,
}

impl FieldType {
    pub fn to_db_string(&self) -> &'static str {
        match self {
            FieldType::String => "string",
            FieldType::Integer => "integer",
            FieldType::Date => "date",
            FieldType::Uuid => "uuid",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "integer" => FieldType::Integer,
            "date" => FieldType::Date,
            "uuid" => FieldType::Uuid,
            _ => FieldType::String,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            FieldType::String => "string",
            FieldType::Integer => "integer",
            FieldType::Date => "date",
            FieldType::Uuid => "uuid",
        }
    }
}

/// Value normalizer
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Normalizer {
    Lowercase,
    Uppercase,
    StripLeadingZeros,
}

impl Normalizer {
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Normalizer::Lowercase => "lowercase",
            Normalizer::Uppercase => "uppercase",
            Normalizer::StripLeadingZeros => "strip_leading_zeros",
        }
    }

    pub fn from_db_string(s: &str) -> Option<Self> {
        match s {
            "lowercase" => Some(Normalizer::Lowercase),
            "uppercase" => Some(Normalizer::Uppercase),
            "strip_leading_zeros" => Some(Normalizer::StripLeadingZeros),
            _ => None,
        }
    }
}

/// Conditional tag assignment
#[derive(Debug, Clone)]
pub struct TagConditionDraft {
    /// Field to compare
    pub field: String,
    /// Comparison operator
    pub operator: CompareOp,
    /// Value to compare against
    pub value: String,
    /// Tag to apply if condition matches
    pub tag: String,
    /// Priority (higher = checked first)
    pub priority: i32,
}

impl Default for TagConditionDraft {
    fn default() -> Self {
        Self {
            field: String::new(),
            operator: CompareOp::Eq,
            value: String::new(),
            tag: String::new(),
            priority: 100,
        }
    }
}

/// Comparison operator for tag conditions
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CompareOp {
    #[default]
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Contains,
    Matches,
}

impl CompareOp {
    pub fn to_db_string(&self) -> &'static str {
        match self {
            CompareOp::Eq => "=",
            CompareOp::NotEq => "!=",
            CompareOp::Lt => "<",
            CompareOp::Gt => ">",
            CompareOp::LtEq => "<=",
            CompareOp::GtEq => ">=",
            CompareOp::Contains => "contains",
            CompareOp::Matches => "matches",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "=" => CompareOp::Eq,
            "!=" => CompareOp::NotEq,
            "<" => CompareOp::Lt,
            ">" => CompareOp::Gt,
            "<=" => CompareOp::LtEq,
            ">=" => CompareOp::GtEq,
            "contains" => CompareOp::Contains,
            "matches" => CompareOp::Matches,
            _ => CompareOp::Eq,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            CompareOp::Eq => "=",
            CompareOp::NotEq => "!=",
            CompareOp::Lt => "<",
            CompareOp::Gt => ">",
            CompareOp::LtEq => "<=",
            CompareOp::GtEq => ">=",
            CompareOp::Contains => "contains",
            CompareOp::Matches => "matches",
        }
    }
}

// =============================================================================
// Rule Editor State
// =============================================================================

/// Focus state for the rule editor (Section 13.8)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum RuleEditorFocus {
    /// Editing glob pattern (section 1/4)
    #[default]
    GlobPattern,
    /// Field list (section 2/4)
    FieldList,
    /// Base tag (section 3/4)
    BaseTag,
    /// Conditions (section 4/4)
    Conditions,
}

impl RuleEditorFocus {
    /// Move to next section (Tab)
    pub fn next(self) -> Self {
        match self {
            RuleEditorFocus::GlobPattern => RuleEditorFocus::FieldList,
            RuleEditorFocus::FieldList => RuleEditorFocus::BaseTag,
            RuleEditorFocus::BaseTag => RuleEditorFocus::Conditions,
            RuleEditorFocus::Conditions => RuleEditorFocus::GlobPattern,
        }
    }

    /// Move to previous section (Shift+Tab)
    pub fn prev(self) -> Self {
        match self {
            RuleEditorFocus::GlobPattern => RuleEditorFocus::Conditions,
            RuleEditorFocus::FieldList => RuleEditorFocus::GlobPattern,
            RuleEditorFocus::BaseTag => RuleEditorFocus::FieldList,
            RuleEditorFocus::Conditions => RuleEditorFocus::BaseTag,
        }
    }

    /// Section number for display (1-4)
    pub fn section_number(self) -> u8 {
        match self {
            RuleEditorFocus::GlobPattern => 1,
            RuleEditorFocus::FieldList => 2,
            RuleEditorFocus::BaseTag => 3,
            RuleEditorFocus::Conditions => 4,
        }
    }
}

/// Sub-focus when editing a field
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldEditFocus {
    Source,
    Pattern,
    TypeHint,
}

// =============================================================================
// Test State
// =============================================================================

/// Phase of test execution
#[derive(Debug, Clone)]
pub enum TestPhase {
    /// Test running in background
    Running {
        files_processed: usize,
        files_total: usize,
        current_file: Option<String>,
    },
    /// Test completed successfully
    Complete,
    /// Test was cancelled by user
    Cancelled { files_processed: usize },
    /// Test encountered fatal error
    Error(String),
}

impl Default for TestPhase {
    fn default() -> Self {
        TestPhase::Running {
            files_processed: 0,
            files_total: 0,
            current_file: None,
        }
    }
}

/// Test results collected during test run
#[derive(Debug, Clone, Default)]
pub struct TestResults {
    /// Total files tested
    pub total_files: usize,
    /// Files with complete extraction
    pub complete: usize,
    /// Files with partial extraction
    pub partial: usize,
    /// Files that failed extraction
    pub failed: usize,
    /// Field metrics (histograms, ranges)
    pub field_metrics: HashMap<String, FieldMetrics>,
    /// Tag counts (tag name -> file count)
    pub tag_counts: HashMap<String, usize>,
    /// Sample extractions for preview
    pub sample_extractions: Vec<SampleExtraction>,
    /// File paths that failed
    pub failed_files: Vec<String>,
}

impl TestResults {
    /// Get pass rate as percentage
    pub fn pass_rate(&self) -> f64 {
        if self.total_files == 0 {
            0.0
        } else {
            self.complete as f64 / self.total_files as f64 * 100.0
        }
    }
}

/// Metrics for a single field
#[derive(Debug, Clone, Default)]
pub struct FieldMetrics {
    /// Field name
    pub field_name: String,
    /// Number of unique values
    pub unique_count: usize,
    /// Top values with counts (max 5)
    pub top_values: Vec<(String, usize)>,
    /// Minimum value (for sortable types)
    pub min_value: Option<String>,
    /// Maximum value
    pub max_value: Option<String>,
    /// Count of null/missing values
    pub null_count: usize,
}

/// Sample extraction for preview
#[derive(Debug, Clone)]
pub struct SampleExtraction {
    /// File path
    pub file_path: String,
    /// Extracted field values
    pub fields: HashMap<String, String>,
    /// Tags that would be applied
    pub tags: Vec<String>,
}

/// Category filter for test results
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TestCategory {
    #[default]
    Summary,
    Complete,
    Partial,
    Failed,
    FieldMetrics,
}

impl TestCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            TestCategory::Summary => "Summary",
            TestCategory::Complete => "Complete",
            TestCategory::Partial => "Partial",
            TestCategory::Failed => "Failed",
            TestCategory::FieldMetrics => "Metrics",
        }
    }
}

// =============================================================================
// Publish State
// =============================================================================

/// Phase of publish operation
#[derive(Debug, Clone, Default)]
pub enum PublishPhase {
    /// Showing confirmation dialog
    #[default]
    Confirming,
    /// Checking for conflicts
    Validating,
    /// Writing rule to database
    Saving,
    /// Creating background job
    StartingJob,
}

/// Error during publishing
#[derive(Debug, Clone)]
pub enum PublishError {
    /// Database connection failed
    DatabaseConnection(String),
    /// Rule name already exists for this source
    RuleNameConflict {
        existing_rule_id: String,
        existing_created_at: String,
    },
    /// Glob pattern conflicts with existing rule
    PatternConflict {
        existing_rule_id: String,
        existing_rule_name: String,
    },
    /// Database write failed
    DatabaseWrite(String),
    /// Job creation failed
    JobCreation(String),
    /// User cancelled
    Cancelled,
}

impl PublishError {
    pub fn message(&self) -> String {
        match self {
            PublishError::DatabaseConnection(msg) => format!("Database connection failed: {}", msg),
            PublishError::RuleNameConflict { .. } => "Rule name already exists".to_string(),
            PublishError::PatternConflict { existing_rule_name, .. } => {
                format!("Pattern conflicts with rule '{}'", existing_rule_name)
            }
            PublishError::DatabaseWrite(msg) => format!("Failed to save rule: {}", msg),
            PublishError::JobCreation(msg) => format!("Failed to create job: {}", msg),
            PublishError::Cancelled => "Publishing cancelled".to_string(),
        }
    }

    pub fn recovery_options(&self) -> Vec<RecoveryOption> {
        match self {
            PublishError::DatabaseConnection(_) | PublishError::DatabaseWrite(_) => {
                vec![RecoveryOption::Retry, RecoveryOption::EditRule, RecoveryOption::Cancel]
            }
            PublishError::RuleNameConflict { existing_rule_id, .. } => {
                vec![
                    RecoveryOption::EditRule,
                    RecoveryOption::Overwrite { existing_id: existing_rule_id.clone() },
                    RecoveryOption::Cancel,
                ]
            }
            PublishError::PatternConflict { existing_rule_id, .. } => {
                vec![
                    RecoveryOption::EditRule,
                    RecoveryOption::Overwrite { existing_id: existing_rule_id.clone() },
                    RecoveryOption::Cancel,
                ]
            }
            PublishError::JobCreation(_) => {
                vec![RecoveryOption::Retry, RecoveryOption::Cancel]
            }
            PublishError::Cancelled => vec![RecoveryOption::EditRule, RecoveryOption::Cancel],
        }
    }
}

/// Recovery option for publish errors
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryOption {
    /// Retry the failed operation
    Retry,
    /// Edit the rule (e.g., change name)
    EditRule,
    /// Overwrite existing rule
    Overwrite { existing_id: String },
    /// Cancel and return to browse
    Cancel,
}

impl RecoveryOption {
    pub fn key(&self) -> char {
        match self {
            RecoveryOption::Retry => 'r',
            RecoveryOption::EditRule => 'e',
            RecoveryOption::Overwrite { .. } => 'o',
            RecoveryOption::Cancel => 'c',
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            RecoveryOption::Retry => "Retry",
            RecoveryOption::EditRule => "Edit rule",
            RecoveryOption::Overwrite { .. } => "Overwrite",
            RecoveryOption::Cancel => "Cancel",
        }
    }
}

// =============================================================================
// Histogram Rendering Config
// =============================================================================

/// Configuration for histogram rendering (Phase 18d)
pub struct HistogramConfig {
    /// Maximum bar width in characters
    pub bar_width: usize,
    /// Maximum number of values to show per field
    pub max_values: usize,
    /// Maximum characters for value label before truncation
    pub max_label_width: usize,
    /// Character for filled portion of bar
    pub filled_char: char,
    /// Character for empty portion of bar
    pub empty_char: char,
}

impl Default for HistogramConfig {
    fn default() -> Self {
        Self {
            bar_width: 12,
            max_values: 5,
            max_label_width: 15,
            filled_char: '█',
            empty_char: '░',
        }
    }
}

impl HistogramConfig {
    /// Render a histogram bar
    pub fn render_bar(&self, count: usize, max_count: usize) -> String {
        if count == 0 {
            return self.empty_char.to_string().repeat(self.bar_width);
        }

        let filled = if max_count == 0 {
            0
        } else {
            let ratio = count as f64 / max_count as f64;
            let filled = (ratio * self.bar_width as f64).round() as usize;
            // At least 1 filled char for non-zero counts
            filled.max(1).min(self.bar_width)
        };

        let empty = self.bar_width - filled;
        format!(
            "{}{}",
            self.filled_char.to_string().repeat(filled),
            self.empty_char.to_string().repeat(empty)
        )
    }

    /// Truncate label to fit max width
    pub fn truncate_label(&self, value: &str) -> String {
        if value.len() <= self.max_label_width {
            format!("{:width$}", value, width = self.max_label_width)
        } else {
            let truncated = &value[..self.max_label_width - 3];
            format!("{}...", truncated)
        }
    }
}

// =============================================================================
// Test State Container
// =============================================================================

/// Full state for the Testing phase
#[derive(Debug, Clone)]
pub struct TestState {
    /// Rule being tested
    pub rule: RuleDraft,
    /// Current phase
    pub phase: TestPhase,
    /// Results (populated when complete)
    pub results: Option<TestResults>,
    /// Selected category tab
    pub selected_category: TestCategory,
    /// Scroll offset within current view
    pub scroll_offset: usize,
    /// Cancellation token (not cloneable, handled separately)
    pub cancel_requested: bool,
}

impl TestState {
    pub fn new(rule: RuleDraft, files_total: usize) -> Self {
        Self {
            rule,
            phase: TestPhase::Running {
                files_processed: 0,
                files_total,
                current_file: None,
            },
            results: None,
            selected_category: TestCategory::Summary,
            scroll_offset: 0,
            cancel_requested: false,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.phase, TestPhase::Running { .. })
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.phase, TestPhase::Complete)
    }
}

// =============================================================================
// Publish State Container
// =============================================================================

/// Full state for the Publishing phase
#[derive(Debug, Clone)]
pub struct PublishState {
    /// Rule being published
    pub rule: RuleDraft,
    /// Current phase
    pub phase: PublishPhase,
    /// Job ID if successfully created
    pub job_id: Option<String>,
    /// Error if publishing failed
    pub error: Option<PublishError>,
    /// Number of matching files (for job creation)
    pub matching_files_count: usize,
}

impl PublishState {
    pub fn new(rule: RuleDraft, matching_files_count: usize) -> Self {
        Self {
            rule,
            phase: PublishPhase::Confirming,
            job_id: None,
            error: None,
            matching_files_count,
        }
    }
}

// =============================================================================
// Rule Builder Types (v3.0 Consolidation)
// =============================================================================
// Types for the unified Rule Builder interface that consolidates:
// - GlobExplorer (pattern exploration)
// - RuleCreation (rule editing)
// - Pathfinder/Labeling/SemanticPath
//
// See: specs/views/discover.md v3.0, specs/meta/sessions/ai_consolidation/design.md

/// Filter for displaying test results in Rule Builder
/// Cycles with a/p/f keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultFilter {
    /// Show all matched files
    #[default]
    All,
    /// Show only files that pass extraction
    PassOnly,
    /// Show only files that fail extraction
    FailOnly,
}

impl ResultFilter {
    /// Cycle to next filter (a → p → f → a)
    pub fn next(self) -> Self {
        match self {
            ResultFilter::All => ResultFilter::PassOnly,
            ResultFilter::PassOnly => ResultFilter::FailOnly,
            ResultFilter::FailOnly => ResultFilter::All,
        }
    }

    /// Get display label
    pub fn label(&self) -> &'static str {
        match self {
            ResultFilter::All => "All",
            ResultFilter::PassOnly => "Pass",
            ResultFilter::FailOnly => "Fail",
        }
    }

    /// Get key hint
    pub fn key_hint(&self) -> &'static str {
        match self {
            ResultFilter::All => "a",
            ResultFilter::PassOnly => "p",
            ResultFilter::FailOnly => "f",
        }
    }
}

// =============================================================================
// File Results Panel Types (spec Section 4)
// =============================================================================

/// State for the file results panel (spec Section 4.1)
/// Encodes mutually exclusive phases with their data.
#[derive(Debug, Clone)]
pub enum FileResultsState {
    /// Folder counts + sample filenames (pattern has no <field>)
    Exploration {
        folder_matches: Vec<FolderMatch>,
        expanded_folder_indices: HashSet<usize>,
        detected_patterns: Vec<String>,
    },
    /// Per-file with extracted values (pattern has <field>)
    ExtractionPreview {
        preview_files: Vec<ExtractionPreviewFile>,
    },
    /// Per-file pass/fail with errors (user pressed 't')
    BacktestResults {
        matched_files: Vec<MatchedFile>,
        visible_indices: Vec<usize>,
        backtest: BacktestSummary,
        result_filter: ResultFilter,
    },
}

impl Default for FileResultsState {
    fn default() -> Self {
        FileResultsState::Exploration {
            folder_matches: Vec::new(),
            expanded_folder_indices: HashSet::new(),
            detected_patterns: Vec::new(),
        }
    }
}

/// A folder with match count and sample (Phase 1: Exploration)
#[derive(Debug, Clone)]
pub struct FolderMatch {
    /// Folder path (e.g., "trades/2024/Q1/")
    pub path: String,
    /// Number of files matching in this folder
    pub count: usize,
    /// Sample filename (first match, reveals naming pattern)
    pub sample_filename: String,
    /// Files in this folder (lazily populated on expand)
    pub files: Vec<String>,
}

/// File with extraction preview (Phase 2: Extraction Preview)
#[derive(Debug, Clone)]
pub struct ExtractionPreviewFile {
    /// Full path
    pub path: String,
    /// Relative path for display
    pub relative_path: String,
    /// Extracted values (field_name -> extracted_value)
    pub extractions: HashMap<String, String>,
    /// Type mismatch warnings (e.g., "20240401_amended" is not valid date)
    pub warnings: Vec<String>,
}

/// Test result status for a single file
#[derive(Debug, Clone, PartialEq)]
pub enum FileTestResult {
    /// File has not been tested yet
    NotTested,
    /// File passed extraction (all fields extracted)
    Pass,
    /// File failed extraction
    Fail {
        /// Error message describing the failure
        error: String,
        /// Optional hint for fixing
        hint: Option<String>,
    },
    /// File was explicitly excluded by user
    Excluded {
        /// The exclusion pattern that matched
        pattern: String,
    },
}

impl FileTestResult {
    /// Is this a passing result?
    pub fn is_pass(&self) -> bool {
        matches!(self, FileTestResult::Pass)
    }

    /// Is this a failing result?
    pub fn is_fail(&self) -> bool {
        matches!(self, FileTestResult::Fail { .. })
    }

    /// Is this excluded?
    pub fn is_excluded(&self) -> bool {
        matches!(self, FileTestResult::Excluded { .. })
    }

    /// Get display indicator
    pub fn indicator(&self) -> &'static str {
        match self {
            FileTestResult::NotTested => " ",
            FileTestResult::Pass => "✓",
            FileTestResult::Fail { .. } => "✗",
            FileTestResult::Excluded { .. } => "○",
        }
    }
}

impl Default for FileTestResult {
    fn default() -> Self {
        FileTestResult::NotTested
    }
}

/// Matched file for Rule Builder right panel
#[derive(Debug, Clone)]
pub struct MatchedFile {
    /// Full path
    pub path: String,
    /// Relative path for display
    pub relative_path: String,
    /// Extracted field values (after backtest)
    pub extractions: HashMap<String, String>,
    /// Test result
    pub test_result: FileTestResult,
}

impl MatchedFile {
    /// Check if this file passes the current filter
    pub fn passes_filter(&self, filter: ResultFilter) -> bool {
        match filter {
            ResultFilter::All => !self.test_result.is_excluded(),
            ResultFilter::PassOnly => self.test_result.is_pass(),
            ResultFilter::FailOnly => self.test_result.is_fail(),
        }
    }
}

/// Backtest summary statistics for Rule Builder
#[derive(Debug, Clone, Default)]
pub struct BacktestSummary {
    /// Total files matched by pattern
    pub total_matched: usize,
    /// Files that pass extraction
    pub pass_count: usize,
    /// Files that fail extraction
    pub fail_count: usize,
    /// Files explicitly excluded
    pub excluded_count: usize,
    /// Whether backtest is currently running
    pub is_running: bool,
}

impl BacktestSummary {
    /// Get pass rate as percentage (excluding excluded files)
    pub fn pass_rate(&self) -> f64 {
        let testable = self.pass_count + self.fail_count;
        if testable == 0 {
            0.0
        } else {
            self.pass_count as f64 / testable as f64 * 100.0
        }
    }

    /// Format as status line: "Pass: 847  Fail: 3  Skip: 12"
    pub fn status_line(&self) -> String {
        format!(
            "Pass: {}  Fail: {}  Skip: {}",
            self.pass_count, self.fail_count, self.excluded_count
        )
    }
}

/// Progress tracking for glob traversal (GrandPerspective-style)
#[derive(Debug, Clone, Default)]
pub struct GlobProgress {
    /// Current folder being scanned (e.g., "dir_42/subfolder/")
    pub current_folder: String,
    /// Number of files matched so far
    pub files_found: usize,
    /// Number of folders discovered (denominator for progress)
    pub folders_discovered: usize,
    /// Number of folders scanned (numerator for progress)
    pub folders_scanned: usize,
    /// Whether the glob is in progress
    pub is_active: bool,
    /// When the glob started
    pub started_at: Option<std::time::Instant>,
}

impl GlobProgress {
    /// Start new progress tracking
    pub fn start() -> Self {
        Self {
            current_folder: String::new(),
            files_found: 0,
            folders_discovered: 0,
            folders_scanned: 0,
            is_active: true,
            started_at: Some(std::time::Instant::now()),
        }
    }

    /// Get approximate progress percentage (0-100)
    /// Denominator grows as we discover more folders, so this fluctuates but shows progress
    pub fn percentage(&self) -> u8 {
        if self.folders_discovered == 0 {
            0
        } else {
            ((self.folders_scanned as f64 / self.folders_discovered as f64) * 100.0).min(99.0) as u8
        }
    }

    /// Format status line: "Scanning logs/2024/... (2,450 files, 45%)"
    pub fn status_line(&self) -> String {
        let folder = if self.current_folder.len() > 30 {
            format!("...{}", &self.current_folder[self.current_folder.len()-27..])
        } else {
            self.current_folder.clone()
        };

        let files_str = format_with_commas(self.files_found);
        let pct = self.percentage();

        if folder.is_empty() {
            format!("Scanning... ({} files, {}%)", files_str, pct)
        } else {
            format!("Scanning {}/... ({} files, {}%)", folder.trim_end_matches('/'), files_str, pct)
        }
    }

    /// Complete the progress
    pub fn complete(&mut self) {
        self.is_active = false;
    }
}

/// Format number with comma separators
fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}

// =============================================================================
// Rule Builder State (v3.0 Split-View)
// =============================================================================
// Full state for the unified Rule Builder interface.
// Layout: Left panel (40%) = rule config, Right panel (60%) = file results
// See: specs/meta/sessions/ai_consolidation/design.md Section 2

/// Focus within Rule Builder left panel
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RuleBuilderFocus {
    /// Pattern input field
    Pattern,
    /// Excludes list
    Excludes,
    /// Adding new exclude pattern
    ExcludeInput,
    /// Tag input field
    Tag,
    /// Extraction fields list
    Extractions,
    /// Editing a specific extraction field
    ExtractionEdit(usize),
    /// Options section
    Options,
    /// Right panel: file list (default - allows shortcuts like 's' for scan to work)
    #[default]
    FileList,
    /// Ignore picker dialog
    IgnorePicker,
}

/// Extraction field for Rule Builder (simplified from FieldDraft)
#[derive(Debug, Clone)]
pub struct RuleBuilderField {
    /// Field name (e.g., "year", "mission_id")
    pub name: String,
    /// Source segment or location
    pub source: FieldSource,
    /// Type hint
    pub field_type: FieldType,
    /// Regex pattern (optional)
    pub pattern: Option<String>,
    /// Sample values (populated from analysis)
    pub sample_values: Vec<String>,
    /// Whether field is enabled
    pub enabled: bool,
}

impl RuleBuilderField {
    /// Create from FieldDraft
    pub fn from_draft(draft: &FieldDraft) -> Self {
        Self {
            name: draft.name.clone(),
            source: draft.source.clone(),
            field_type: draft.type_hint.clone(),
            pattern: draft.pattern.clone(),
            sample_values: Vec::new(),
            enabled: true,
        }
    }
}

/// Ignore picker option
#[derive(Debug, Clone)]
pub struct IgnoreOption {
    /// Glob pattern to add to excludes
    pub pattern: String,
    /// Number of files affected
    pub affected_count: usize,
}

// =============================================================================
// Schema-First UI Types (RULE_BUILDER_UI_PLAN.md)
// =============================================================================
// Types for the schema-first Rule Builder UI with path archetypes,
// naming schemes, and synonym detection.

/// A pattern seed suggestion (common glob patterns detected from files)
#[derive(Debug, Clone)]
pub struct PatternSeed {
    /// The glob pattern (e.g., "*.csv", "**/*.csv", "*_report*.csv")
    pub pattern: String,
    /// Number of files matching this pattern
    pub match_count: usize,
    /// Whether this is an extension-based pattern
    pub is_extension: bool,
}

/// A path archetype representing a common directory structure pattern
/// Example: "program_<code>/mission_<id>/<date>/..." with 2,130 files across 42 folders
#[derive(Debug, Clone)]
pub struct PathArchetype {
    /// Normalized template path (e.g., "program_<code>/mission_<id>/<date>/...")
    pub template: String,
    /// Number of files matching this archetype
    pub file_count: usize,
    /// Number of unique folders
    pub folder_count: usize,
    /// Sample paths that match this archetype (for preview)
    pub sample_paths: Vec<String>,
    /// Depth of the pattern (number of segments)
    pub depth: usize,
}

/// A naming scheme representing a common filename pattern
/// Example: "report_<date>_<region>.csv" with 412 files
#[derive(Debug, Clone)]
pub struct NamingScheme {
    /// Normalized filename template (e.g., "report_<date>_<region>.csv")
    pub template: String,
    /// Number of files matching this scheme
    pub file_count: usize,
    /// Example filename
    pub example: String,
    /// Detected fields in the scheme
    pub fields: Vec<String>,
}

/// Confidence level for synonym suggestions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynonymConfidence {
    /// Lexicon match (e.g., msn → mission)
    High,
    /// Edit distance or prefix match
    Medium,
    /// Neighbor-based inference only
    Low,
}

/// A synonym suggestion for path token normalization
/// Example: "msn" ↔ "mission" (high confidence)
#[derive(Debug, Clone)]
pub struct SynonymSuggestion {
    /// The abbreviated/variant token
    pub short_form: String,
    /// The canonical/expanded token
    pub canonical_form: String,
    /// Confidence level
    pub confidence: SynonymConfidence,
    /// Reasoning for the suggestion
    pub reason: String,
    /// Score (for ranking)
    pub score: u8,
    /// Whether user has applied this suggestion
    pub applied: bool,
}

/// Schema evaluation state for the rule builder
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalState {
    Idle,
    Running { progress: u8 },
}

/// Built-in abbreviation lexicon for synonym detection
pub fn get_abbreviation_lexicon() -> &'static [(&'static str, &'static str)] {
    &[
        ("msn", "mission"),
        ("flt", "flight"),
        ("cfg", "config"),
        ("img", "image"),
        ("idx", "index"),
        ("ops", "operations"),
        ("rpt", "report"),
        ("tmp", "temp"),
        ("usr", "user"),
        ("sys", "system"),
        ("msg", "message"),
        ("doc", "document"),
        ("env", "environment"),
        ("srv", "server"),
        ("app", "application"),
    ]
}

/// Full Rule Builder state
/// See: specs/rule_builder.md Section 8.4
#[derive(Debug, Clone)]
pub struct RuleBuilderState {
    // --- Input fields ---
    /// Glob pattern input
    pub pattern: String,
    /// Pattern validation error (if any)
    pub pattern_error: Option<String>,
    /// Exclusion patterns
    pub excludes: Vec<String>,
    /// Input buffer for adding new exclude
    pub exclude_input: String,
    /// Tag name
    pub tag: String,
    /// Extraction fields
    pub extractions: Vec<RuleBuilderField>,
    /// Rule enabled flag
    pub enabled: bool,
    /// Run job on save flag
    pub run_job_on_save: bool,

    // --- File Results State (spec Section 4) ---
    /// Current state of the file results panel
    pub file_results: FileResultsState,

    // --- Schema-First UI (RULE_BUILDER_UI_PLAN.md) ---
    /// Pattern seeds - common glob patterns detected from file extensions and names
    pub pattern_seeds: Vec<PatternSeed>,
    /// Path archetypes - structure-aware path template summaries
    pub path_archetypes: Vec<PathArchetype>,
    /// Naming schemes - common filename patterns
    pub naming_schemes: Vec<NamingScheme>,
    /// Synonym suggestions - detected abbreviation mappings
    pub synonym_suggestions: Vec<SynonymSuggestion>,
    /// Selected pattern seed index
    pub selected_pattern_seed: usize,
    /// Selected path archetype index
    pub selected_archetype: usize,
    /// Selected naming scheme index
    pub selected_naming_scheme: usize,
    /// Selected synonym suggestion index
    pub selected_synonym: usize,
    /// Schema evaluation state
    pub eval_state: EvalState,
    /// Total match count
    pub match_count: usize,

    // --- Selection & Navigation ---
    /// Selected index in right panel (folder or file depending on phase)
    pub selected_file: usize,
    /// Selected extraction index in left panel
    pub selected_extraction: usize,
    /// Selected exclude index
    pub selected_exclude: usize,

    // --- Ignore picker state ---
    /// Options for ignore picker dialog
    pub ignore_options: Vec<IgnoreOption>,
    /// Selected ignore option
    pub ignore_selected: usize,

    // --- UI state ---
    /// Current focus
    pub focus: RuleBuilderFocus,

    // --- Debouncing ---
    /// When pattern was last modified
    pub pattern_changed_at: Option<std::time::Instant>,

    // --- Streaming state ---
    /// Whether a streaming search is in progress
    pub is_streaming: bool,
    /// Elapsed time in milliseconds for current search
    pub stream_elapsed_ms: u64,

    // --- Glob progress ---
    /// Progress tracking for folder traversal (shown in status bar)
    pub glob_progress: GlobProgress,

    // --- Edit mode ---
    /// Rule ID if editing existing rule
    pub editing_rule_id: Option<String>,
    /// Source ID
    pub source_id: Option<String>,
}

impl Default for RuleBuilderState {
    fn default() -> Self {
        Self {
            // Input fields
            pattern: String::new(),
            pattern_error: None,
            excludes: Vec::new(),
            exclude_input: String::new(),
            tag: String::new(),
            extractions: Vec::new(),
            enabled: true,
            run_job_on_save: true,

            // File Results State (default: Exploration)
            file_results: FileResultsState::default(),

            // Schema-First UI
            pattern_seeds: Vec::new(),
            path_archetypes: Vec::new(),
            naming_schemes: Vec::new(),
            synonym_suggestions: Vec::new(),
            selected_pattern_seed: 0,
            selected_archetype: 0,
            selected_naming_scheme: 0,
            selected_synonym: 0,
            eval_state: EvalState::Idle,
            match_count: 0,

            // Selection & Navigation
            selected_file: 0,
            selected_extraction: 0,
            selected_exclude: 0,

            // Ignore picker
            ignore_options: Vec::new(),
            ignore_selected: 0,

            // UI state - default to Pattern so typing edits immediately
            focus: RuleBuilderFocus::Pattern,

            // Debouncing
            pattern_changed_at: None,

            // Streaming state
            is_streaming: false,
            stream_elapsed_ms: 0,

            // Glob progress
            glob_progress: GlobProgress::default(),

            // Edit mode
            editing_rule_id: None,
            source_id: None,
        }
    }
}

impl RuleBuilderState {
    /// Create new Rule Builder for a source
    pub fn new(source_id: Option<String>) -> Self {
        Self {
            source_id,
            ..Default::default()
        }
    }

    /// Create from existing rule draft (for editing)
    pub fn from_draft(draft: &RuleDraft, source_id: Option<String>) -> Self {
        Self {
            pattern: draft.glob_pattern.clone(),
            tag: draft.base_tag.clone(),
            extractions: draft.fields.iter().map(RuleBuilderField::from_draft).collect(),
            enabled: draft.enabled,
            editing_rule_id: draft.id.map(|id| id.to_string()),
            source_id,
            ..Default::default()
        }
    }

    /// Get visible files based on current filter
    pub fn visible_files(&self) -> Box<dyn Iterator<Item = &MatchedFile> + '_> {
        match &self.file_results {
            FileResultsState::BacktestResults { matched_files, visible_indices, .. } => {
                Box::new(visible_indices.iter().filter_map(move |&i| matched_files.get(i)))
            }
            _ => Box::new(std::iter::empty()),
        }
    }

    /// Update visible indices based on current filter (BacktestResults only)
    pub fn update_visible(&mut self) {
        if let FileResultsState::BacktestResults {
            matched_files,
            visible_indices,
            result_filter,
            ..
        } = &mut self.file_results {
            *visible_indices = matched_files
                .iter()
                .enumerate()
                .filter(|(_, f)| f.passes_filter(*result_filter))
                .map(|(i, _)| i)
                .collect();

            // Clamp selection
            if !visible_indices.is_empty() && self.selected_file >= visible_indices.len() {
                self.selected_file = visible_indices.len().saturating_sub(1);
            }
        }
    }

    /// Cycle result filter (a → p → f → a)
    pub fn cycle_filter(&mut self) {
        if let FileResultsState::BacktestResults { result_filter, .. } = &mut self.file_results {
            *result_filter = result_filter.next();
            self.update_visible();
        }
    }

    /// Add exclusion pattern
    pub fn add_exclude(&mut self, pattern: String) {
        if !self.excludes.contains(&pattern) {
            self.excludes.push(pattern);
        }
    }

    /// Remove exclusion pattern at index
    pub fn remove_exclude(&mut self, index: usize) {
        if index < self.excludes.len() {
            self.excludes.remove(index);
            if self.selected_exclude >= self.excludes.len() && !self.excludes.is_empty() {
                self.selected_exclude = self.excludes.len() - 1;
            }
        }
    }

    /// Check if ready to save
    pub fn can_save(&self) -> bool {
        !self.pattern.is_empty() && !self.tag.is_empty()
    }

    /// Convert to RuleDraft for saving
    pub fn to_draft(&self) -> RuleDraft {
        RuleDraft {
            id: self.editing_rule_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
            source_id: self.source_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()),
            name: self.tag.clone(), // Use tag as name by default
            glob_pattern: self.pattern.clone(),
            fields: self.extractions.iter().filter(|f| f.enabled).map(|f| {
                FieldDraft {
                    name: f.name.clone(),
                    source: f.source.clone(),
                    pattern: f.pattern.clone(),
                    type_hint: f.field_type.clone(),
                    normalizer: None,
                    default_value: None,
                }
            }).collect(),
            base_tag: self.tag.clone(),
            tag_conditions: Vec::new(),
            priority: 100,
            enabled: self.enabled,
        }
    }
}

// =============================================================================
// Custom Glob Pattern Parser
// =============================================================================
// Parses patterns with <field> and <field:type> placeholders.
// See: specs/rule_builder.md Section 2.1

/// A placeholder extracted from a custom glob pattern
#[derive(Debug, Clone, PartialEq)]
pub struct FieldPlaceholder {
    /// Field name (e.g., "mission_id")
    pub name: String,
    /// Optional type hint (e.g., "date", "int")
    pub type_hint: Option<String>,
    /// Position in the original pattern (for error highlighting)
    pub position: usize,
    /// Segment index in matched path (negative = from end)
    /// Calculated post-match, initialized to 0
    pub segment_index: i32,
}

/// Result of parsing a custom glob pattern
#[derive(Debug, Clone)]
pub struct ParsedGlobPattern {
    /// Standard glob pattern (placeholders replaced with *)
    pub glob_pattern: String,
    /// Extracted field placeholders
    pub placeholders: Vec<FieldPlaceholder>,
}

/// Error from parsing a custom glob pattern
#[derive(Debug, Clone, PartialEq)]
pub struct GlobParseError {
    /// Error message
    pub message: String,
    /// Position in the pattern where the error occurred
    pub position: usize,
    /// Recovery hint for the user
    pub hint: Option<String>,
}

impl GlobParseError {
    fn unclosed_placeholder(pos: usize) -> Self {
        Self {
            message: format!("Unclosed placeholder at position {}", pos),
            position: pos,
            hint: Some("Add matching '>' or escape with '\\<'".into()),
        }
    }

    fn invalid_field_name(name: &str, pos: usize) -> Self {
        Self {
            message: format!("Invalid field name: '{}'", name),
            position: pos,
            hint: Some("Use lowercase letters and underscores (e.g., 'mission_id')".into()),
        }
    }

    fn empty_field_name(pos: usize) -> Self {
        Self {
            message: "Empty field name".into(),
            position: pos,
            hint: Some("Provide a field name (e.g., '<year>')".into()),
        }
    }

    fn duplicate_field_name(name: &str, pos: usize) -> Self {
        Self {
            message: format!("Duplicate field name: '{}'", name),
            position: pos,
            hint: Some("Each field name must be unique".into()),
        }
    }

    fn unknown_type_hint(hint: &str, pos: usize) -> Self {
        Self {
            message: format!("Unknown type hint: '{}'", hint),
            position: pos,
            hint: Some("Valid types: string, int, integer, date, uuid".into()),
        }
    }

    fn nested_placeholder(pos: usize) -> Self {
        Self {
            message: "Nested placeholders not supported".into(),
            position: pos,
            hint: Some("Close the current placeholder before starting a new one".into()),
        }
    }
}

/// Parse a custom glob pattern with <field> placeholders.
///
/// # Examples
/// ```
/// let result = parse_custom_glob("**/mission_<id>/<date>/*.csv").unwrap();
/// assert_eq!(result.glob_pattern, "**/mission_*/*/*.csv");
/// assert_eq!(result.placeholders.len(), 2);
/// ```
pub fn parse_custom_glob(pattern: &str) -> Result<ParsedGlobPattern, GlobParseError> {
    let mut glob_pattern = String::with_capacity(pattern.len());
    let mut placeholders = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Handle escape sequences
        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '<' || next == '>' {
                // Escaped < or > - treat as literal
                glob_pattern.push(next);
                i += 2;
                continue;
            }
            // Other escape - pass through
            glob_pattern.push(ch);
            glob_pattern.push(next);
            i += 2;
            continue;
        }

        // Handle placeholder start
        if ch == '<' {
            let start_pos = i;
            i += 1;

            // Find matching '>'
            let mut end_pos = None;
            let mut depth = 1;
            let mut j = i;

            while j < chars.len() {
                if chars[j] == '<' {
                    // Nested '<' - error
                    return Err(GlobParseError::nested_placeholder(j));
                } else if chars[j] == '>' {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some(j);
                        break;
                    }
                }
                j += 1;
            }

            let end = end_pos.ok_or_else(|| GlobParseError::unclosed_placeholder(start_pos))?;

            // Extract content between < and >
            let content: String = chars[i..end].iter().collect();

            if content.is_empty() {
                return Err(GlobParseError::empty_field_name(start_pos));
            }

            // Parse content as "field_name" or "field_name:type_hint"
            let (field_name, type_hint) = if let Some(colon_idx) = content.find(':') {
                let name = &content[..colon_idx];
                let hint = &content[colon_idx + 1..];
                (name.to_string(), Some(hint.to_string()))
            } else {
                (content.clone(), None)
            };

            // Validate field name: [a-z_][a-z0-9_]*
            if !is_valid_field_name(&field_name) {
                return Err(GlobParseError::invalid_field_name(&field_name, start_pos));
            }

            // Check for duplicate field names
            if seen_names.contains(&field_name) {
                return Err(GlobParseError::duplicate_field_name(&field_name, start_pos));
            }
            seen_names.insert(field_name.clone());

            // Validate type hint if provided
            if let Some(ref hint) = type_hint {
                if !is_valid_type_hint(hint) {
                    return Err(GlobParseError::unknown_type_hint(hint, start_pos));
                }
            }

            // Add placeholder
            placeholders.push(FieldPlaceholder {
                name: field_name,
                type_hint,
                position: start_pos,
                segment_index: 0, // Calculated post-match
            });

            // Replace placeholder with * in glob pattern
            glob_pattern.push('*');
            i = end + 1;
        } else {
            // Regular character - pass through
            glob_pattern.push(ch);
            i += 1;
        }
    }

    Ok(ParsedGlobPattern {
        glob_pattern,
        placeholders,
    })
}

/// Check if a field name is valid: [a-z_][a-z0-9_]*
fn is_valid_field_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();
    let first = chars.next().unwrap();

    // First char must be lowercase letter or underscore
    if !first.is_ascii_lowercase() && first != '_' {
        return false;
    }

    // Rest must be lowercase letters, digits, or underscores
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Check if a type hint is valid
fn is_valid_type_hint(hint: &str) -> bool {
    matches!(
        hint.to_lowercase().as_str(),
        "string" | "str" | "int" | "integer" | "date" | "uuid"
    )
}

/// Normalize a type hint to standard form
pub fn normalize_type_hint(hint: &str) -> FieldType {
    match hint.to_lowercase().as_str() {
        "int" | "integer" => FieldType::Integer,
        "date" => FieldType::Date,
        "uuid" => FieldType::Uuid,
        _ => FieldType::String,
    }
}

/// Extract field values from a matched path using placeholder positions.
///
/// # Arguments
/// * `matched_path` - The path that was matched by the glob pattern
/// * `parsed` - The parsed glob pattern with placeholders
///
/// # Returns
/// A map of field names to extracted values
pub fn extract_field_values(
    matched_path: &str,
    parsed: &ParsedGlobPattern,
) -> HashMap<String, String> {
    let mut result = HashMap::new();

    if parsed.placeholders.is_empty() {
        return result;
    }

    // Split the matched path into segments
    let segments: Vec<&str> = matched_path.split('/').collect();
    let seg_count = segments.len() as i32;

    // For each placeholder, we need to figure out which segment it corresponds to.
    // This requires comparing the glob pattern structure with the matched path.
    //
    // Simple approach: split both by '/' and match positions
    let glob_segments: Vec<&str> = parsed.glob_pattern.split('/').collect();

    // Track which glob segments have placeholders
    // A glob segment with '*' that came from a placeholder needs to extract value
    let mut placeholder_iter = parsed.placeholders.iter();
    let mut current_placeholder = placeholder_iter.next();

    for (glob_idx, glob_seg) in glob_segments.iter().enumerate() {
        // Skip ** segments - they can match multiple path segments
        if *glob_seg == "**" {
            continue;
        }

        if let Some(ph) = current_placeholder {
            // Check if this glob segment contains a * that came from this placeholder
            // We do this by checking if the placeholder position falls within this segment
            // in the original pattern.
            //
            // Simplified approach: count '*' replacements in glob segments
            if glob_seg.contains('*') {
                // This segment may contain our placeholder's '*'
                // Map glob_idx to matched path segment
                // Handle ** by calculating offset

                // Count leading ** segments in glob pattern
                let leading_wildcards = glob_segments
                    .iter()
                    .take_while(|s| **s == "**")
                    .count();

                // Calculate matched segment index
                let matched_idx = if leading_wildcards > 0 {
                    // If pattern starts with **, segments align from the end
                    // Pattern: **/a/*/c → matched_path has variable prefix
                    let glob_fixed_segments = glob_segments.len() - leading_wildcards;
                    let offset = seg_count as usize - glob_fixed_segments;
                    offset + (glob_idx - leading_wildcards)
                } else {
                    glob_idx
                };

                if matched_idx < segments.len() {
                    let matched_segment = segments[matched_idx];

                    // Extract the value - if glob_seg is just "*", take whole segment
                    // If glob_seg is "prefix_*" or "*_suffix", extract the variable part
                    let value = extract_placeholder_value(glob_seg, matched_segment);
                    result.insert(ph.name.clone(), value);
                }

                current_placeholder = placeholder_iter.next();
            }
        }
    }

    result
}

/// Extract the placeholder value from a matched segment.
/// Handles patterns like "prefix_*", "*_suffix", or just "*".
fn extract_placeholder_value(glob_segment: &str, matched_segment: &str) -> String {
    // Simple case: segment is just "*"
    if glob_segment == "*" {
        return matched_segment.to_string();
    }

    // Find the position of '*' in the glob segment
    if let Some(star_pos) = glob_segment.find('*') {
        let prefix = &glob_segment[..star_pos];
        let suffix = &glob_segment[star_pos + 1..];

        // Extract the middle part
        if matched_segment.starts_with(prefix) && matched_segment.ends_with(suffix) {
            let start = prefix.len();
            let end = matched_segment.len() - suffix.len();
            if start <= end {
                return matched_segment[start..end].to_string();
            }
        }
    }

    // Fallback: return whole segment
    matched_segment.to_string()
}

// =============================================================================
// Schema-First Algorithms (RULE_BUILDER_UI_PLAN.md)
// =============================================================================
// Deterministic algorithms for token normalization, path archetype extraction,
// naming scheme detection, and synonym detection.

use regex::Regex;
use std::sync::LazyLock;

// Compiled regex patterns for token normalization
static DATE_YYYY_MM_DD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}[-/]\d{2}[-/]\d{2}$").unwrap()
});
static DATE_YYYYMMDD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{8}$").unwrap()
});
static DATE_YYYY_MM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}[-/]\d{2}$").unwrap()
});
static DATE_YYYY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}$").unwrap()
});
static UUID_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$").unwrap()
});
static INTEGER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{2,}$").unwrap()
});

/// Normalized token type for schema extraction
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NormalizedToken {
    /// Date-like token (YYYY-MM-DD, YYYYMMDD, etc.)
    Date,
    /// UUID-like token
    Uuid,
    /// Integer sequence (2+ digits)
    Integer,
    /// Literal token (preserved as-is)
    Literal(String),
}

impl NormalizedToken {
    /// Get the placeholder representation for templates
    pub fn placeholder(&self) -> &str {
        match self {
            NormalizedToken::Date => "<date>",
            NormalizedToken::Uuid => "<uuid>",
            NormalizedToken::Integer => "<n>",
            NormalizedToken::Literal(s) => s.as_str(),
        }
    }
}

/// Normalize a single token according to the spec.
///
/// - Replace date-like tokens → `<date>`
/// - Replace UUIDs → `<uuid>`
/// - Replace integer sequences (2+ digits) → `<n>`
/// - Preserve delimiters `_` `-` `.`
pub fn normalize_token(token: &str) -> NormalizedToken {
    let token_lower = token.to_lowercase();

    // Check for date patterns
    if DATE_YYYY_MM_DD.is_match(token) {
        return NormalizedToken::Date;
    }
    if DATE_YYYYMMDD.is_match(token) {
        // Only if it looks like a valid date (e.g., 20240115)
        if let Ok(year) = token[0..4].parse::<u32>() {
            if (1900..=2100).contains(&year) {
                return NormalizedToken::Date;
            }
        }
    }
    if DATE_YYYY_MM.is_match(token) {
        return NormalizedToken::Date;
    }
    // Only treat as year if it's a valid year range
    if DATE_YYYY.is_match(token) {
        if let Ok(year) = token.parse::<u32>() {
            if (1900..=2100).contains(&year) {
                return NormalizedToken::Date;
            }
        }
    }

    // Check for UUID
    if UUID_PATTERN.is_match(token) {
        return NormalizedToken::Uuid;
    }

    // Check for integer sequence (2+ digits)
    if INTEGER_PATTERN.is_match(token) {
        return NormalizedToken::Integer;
    }

    // Preserve as literal
    NormalizedToken::Literal(token_lower)
}

/// Tokenize a path segment by splitting on `_`, `-`, `.`
pub fn tokenize_segment(segment: &str) -> Vec<String> {
    segment
        .split(|c| c == '_' || c == '-' || c == '.')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Normalize a full path into a template.
/// Returns the normalized template and the original path.
pub fn normalize_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.is_empty() {
                return segment.to_string();
            }

            // First, check if the WHOLE segment is a recognizable pattern (date, UUID)
            // This prevents date strings like "2024-01-15" from being split into parts
            let whole_segment_normalized = normalize_token(segment);
            match whole_segment_normalized {
                NormalizedToken::Date | NormalizedToken::Uuid => {
                    return whole_segment_normalized.placeholder().to_string();
                }
                _ => {}
            }

            // Not a whole-segment pattern, so tokenize
            let tokens = tokenize_segment(segment);
            if tokens.is_empty() {
                return segment.to_string();
            }

            // Normalize each token
            let normalized: Vec<_> = tokens
                .iter()
                .map(|t| normalize_token(t))
                .collect();

            // Reconstruct with original delimiters (simplified: use _)
            let parts: Vec<_> = normalized
                .iter()
                .map(|n| n.placeholder())
                .collect();

            // If all tokens normalize to placeholders, return template
            // Otherwise, try to preserve the structure
            if normalized.iter().all(|n| matches!(n, NormalizedToken::Literal(_))) {
                // All literals - return as-is (lowercase)
                segment.to_lowercase()
            } else {
                // Has normalized tokens - reconstruct
                parts.join("_")
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Extract path archetypes from a list of file paths.
///
/// Groups paths by their normalized template and returns the top N archetypes.
pub fn extract_path_archetypes(paths: &[String], top_n: usize) -> Vec<PathArchetype> {
    let mut template_groups: HashMap<String, Vec<&String>> = HashMap::new();

    for path in paths {
        // Get parent directory (exclude filename)
        let parent = if let Some(pos) = path.rfind('/') {
            &path[..pos]
        } else {
            continue; // Skip files without directory
        };

        let template = normalize_path(parent);
        template_groups.entry(template).or_default().push(path);
    }

    // Convert to archetypes and sort by count
    let mut archetypes: Vec<PathArchetype> = template_groups
        .into_iter()
        .map(|(template, files)| {
            let unique_folders: std::collections::HashSet<_> = files
                .iter()
                .filter_map(|p| p.rfind('/').map(|pos| &p[..pos]))
                .collect();

            let depth = template.matches('/').count() + 1;

            PathArchetype {
                template: if template.is_empty() { ".".to_string() } else { format!("{}/...", template) },
                file_count: files.len(),
                folder_count: unique_folders.len(),
                sample_paths: files.iter().take(3).map(|s| (*s).clone()).collect(),
                depth,
            }
        })
        .collect();

    // Sort by file count descending
    archetypes.sort_by(|a, b| b.file_count.cmp(&a.file_count));
    archetypes.truncate(top_n);
    archetypes
}

/// Extract naming schemes from a list of file paths.
///
/// Groups filenames by their normalized template and returns the top N schemes.
pub fn extract_naming_schemes(paths: &[String], top_n: usize) -> Vec<NamingScheme> {
    let mut template_groups: HashMap<String, Vec<&String>> = HashMap::new();

    for path in paths {
        // Extract filename
        let filename = path.rsplit('/').next().unwrap_or(path);

        // Separate extension
        let (name, ext) = if let Some(dot_pos) = filename.rfind('.') {
            (&filename[..dot_pos], Some(&filename[dot_pos..]))
        } else {
            (filename, None)
        };

        // Tokenize and normalize the filename (without extension)
        let tokens = tokenize_segment(name);
        let normalized: Vec<_> = tokens.iter().map(|t| normalize_token(t)).collect();

        // Build template
        let mut template_parts: Vec<String> = Vec::new();
        let mut fields: Vec<String> = Vec::new();

        for (i, norm) in normalized.iter().enumerate() {
            match norm {
                NormalizedToken::Date => {
                    template_parts.push("<date>".to_string());
                    fields.push(format!("field_{}", i));
                }
                NormalizedToken::Uuid => {
                    template_parts.push("<uuid>".to_string());
                    fields.push(format!("field_{}", i));
                }
                NormalizedToken::Integer => {
                    template_parts.push("<n>".to_string());
                    fields.push(format!("field_{}", i));
                }
                NormalizedToken::Literal(s) => {
                    template_parts.push(s.clone());
                }
            }
        }

        let mut template = template_parts.join("_");
        if let Some(e) = ext {
            template.push_str(e);
        }

        // Only include if there's at least one field
        if !fields.is_empty() {
            template_groups.entry(template).or_default().push(path);
        }
    }

    // Convert to naming schemes
    let mut schemes: Vec<NamingScheme> = template_groups
        .into_iter()
        .map(|(template, files)| {
            let example = files.first()
                .and_then(|p| p.rsplit('/').next())
                .unwrap_or("")
                .to_string();

            // Re-extract fields for this template
            let fields: Vec<String> = template
                .split(|c| c == '_' || c == '-' || c == '.')
                .enumerate()
                .filter_map(|(i, part)| {
                    if part.starts_with('<') && part.ends_with('>') {
                        Some(format!("field_{}", i))
                    } else {
                        None
                    }
                })
                .collect();

            NamingScheme {
                template,
                file_count: files.len(),
                example,
                fields,
            }
        })
        .collect();

    // Sort by file count descending
    schemes.sort_by(|a, b| b.file_count.cmp(&a.file_count));
    schemes.truncate(top_n);
    schemes
}

/// Extract pattern seeds (common glob patterns) from file paths.
pub fn extract_pattern_seeds(paths: &[String], top_n: usize) -> Vec<PatternSeed> {
    let mut extension_counts: HashMap<String, usize> = HashMap::new();

    for path in paths {
        // Extract extension
        if let Some(filename) = path.rsplit('/').next() {
            if let Some(dot_pos) = filename.rfind('.') {
                let ext = &filename[dot_pos..];
                *extension_counts.entry(ext.to_lowercase()).or_insert(0) += 1;
            }
        }
    }

    // Build pattern seeds from extensions
    let mut seeds: Vec<PatternSeed> = extension_counts
        .into_iter()
        .map(|(ext, count)| {
            PatternSeed {
                pattern: format!("**/*{}", ext),
                match_count: count,
                is_extension: true,
            }
        })
        .collect();

    // Sort by count descending
    seeds.sort_by(|a, b| b.match_count.cmp(&a.match_count));
    seeds.truncate(top_n);
    seeds
}

/// Compute edit distance between two strings (Levenshtein).
fn edit_distance(a: &str, b: &str) -> usize {
    let m = a.len();
    let n = b.len();

    if m == 0 { return n; }
    if n == 0 { return m; }

    let mut dp = vec![vec![0; n + 1]; m + 1];

    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i-1] == b_chars[j-1] { 0 } else { 1 };
            dp[i][j] = (dp[i-1][j] + 1)
                .min(dp[i][j-1] + 1)
                .min(dp[i-1][j-1] + cost);
        }
    }

    dp[m][n]
}

/// Detect synonym suggestions from a list of file paths.
///
/// Uses the algorithm from RULE_BUILDER_UI_PLAN.md:
/// 1. Tokenize paths
/// 2. Normalize tokens
/// 3. Detect candidates via edit distance, prefix matching, and lexicon
/// 4. Score and filter candidates
pub fn detect_synonyms(paths: &[String], min_sample_count: usize) -> Vec<SynonymSuggestion> {
    if paths.len() < min_sample_count {
        return Vec::new();
    }

    // Collect tokens at each depth with their frequency
    let mut tokens_by_depth: HashMap<usize, HashMap<String, usize>> = HashMap::new();

    for path in paths {
        for (depth, segment) in path.split('/').enumerate() {
            let tokens = tokenize_segment(segment);
            for token in tokens {
                let lower = token.to_lowercase();
                // Skip very short tokens and normalized tokens
                if lower.len() < 2 { continue; }
                if normalize_token(&lower) != NormalizedToken::Literal(lower.clone()) {
                    continue;
                }
                *tokens_by_depth
                    .entry(depth)
                    .or_default()
                    .entry(lower)
                    .or_insert(0) += 1;
            }
        }
    }

    let lexicon = get_abbreviation_lexicon();
    let mut suggestions: Vec<SynonymSuggestion> = Vec::new();
    let mut seen_pairs: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    // Check lexicon matches first
    for (short, canonical) in lexicon {
        for tokens in tokens_by_depth.values() {
            if tokens.contains_key(*short) {
                let pair = (short.to_string(), canonical.to_string());
                if seen_pairs.insert(pair.clone()) {
                    suggestions.push(SynonymSuggestion {
                        short_form: short.to_string(),
                        canonical_form: canonical.to_string(),
                        confidence: SynonymConfidence::High,
                        reason: "Lexicon match".to_string(),
                        score: 3,
                        applied: false,
                    });
                }
            }
        }
    }

    // Check edit distance and prefix matches within same depth
    for tokens in tokens_by_depth.values() {
        let token_list: Vec<_> = tokens.keys().cloned().collect();
        for (i, a) in token_list.iter().enumerate() {
            for b in token_list.iter().skip(i + 1) {
                // Skip if same or already seen
                if a == b { continue; }

                let pair = if a < b {
                    (a.clone(), b.clone())
                } else {
                    (b.clone(), a.clone())
                };
                if seen_pairs.contains(&pair) { continue; }

                let mut score = 0u8;
                let mut reasons = Vec::new();

                // Edit distance check
                let dist = edit_distance(a, b);
                if dist <= 2 && dist > 0 {
                    score += if dist == 1 { 2 } else { 1 };
                    reasons.push(format!("edit distance {}", dist));
                }

                // Prefix check
                if a.len() >= 3 && b.len() >= 3 {
                    if a.starts_with(b) || b.starts_with(a) {
                        let len_diff = (a.len() as i32 - b.len() as i32).abs();
                        if len_diff <= 3 {
                            score += 1;
                            reasons.push("prefix match".to_string());
                        }
                    }
                }

                // Only suggest if score >= 2
                if score >= 2 {
                    seen_pairs.insert(pair);
                    let (short, canonical) = if a.len() < b.len() {
                        (a.clone(), b.clone())
                    } else {
                        (b.clone(), a.clone())
                    };

                    let confidence = if score >= 3 {
                        SynonymConfidence::High
                    } else {
                        SynonymConfidence::Medium
                    };

                    suggestions.push(SynonymSuggestion {
                        short_form: short,
                        canonical_form: canonical,
                        confidence,
                        reason: reasons.join(", "),
                        score,
                        applied: false,
                    });
                }
            }
        }
    }

    // Sort by score descending, then by confidence
    suggestions.sort_by(|a, b| {
        b.score.cmp(&a.score)
            .then_with(|| {
                let conf_ord = |c: &SynonymConfidence| match c {
                    SynonymConfidence::High => 0,
                    SynonymConfidence::Medium => 1,
                    SynonymConfidence::Low => 2,
                };
                conf_ord(&a.confidence).cmp(&conf_ord(&b.confidence))
            })
    });

    // Limit to top 5 per the spec
    suggestions.truncate(5);
    suggestions
}

/// Analyze paths and populate schema-first UI fields in RuleBuilderState.
///
/// This is the main entry point for the schema-first analysis.
pub fn analyze_paths_for_schema_ui(
    state: &mut RuleBuilderState,
    paths: &[String],
    top_n: usize,
) {
    state.pattern_seeds = extract_pattern_seeds(paths, top_n);
    state.path_archetypes = extract_path_archetypes(paths, top_n);
    state.naming_schemes = extract_naming_schemes(paths, top_n);
    state.synonym_suggestions = detect_synonyms(paths, 30); // min 30 paths for synonym detection
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_placeholder() {
        let result = parse_custom_glob("**/mission_<id>/*.csv").unwrap();
        assert_eq!(result.glob_pattern, "**/mission_*/*.csv");
        assert_eq!(result.placeholders.len(), 1);
        assert_eq!(result.placeholders[0].name, "id");
        assert_eq!(result.placeholders[0].type_hint, None);
    }

    #[test]
    fn test_parse_multiple_placeholders() {
        let result = parse_custom_glob("**/mission_<id>/<date>/*.csv").unwrap();
        assert_eq!(result.glob_pattern, "**/mission_*/*/*.csv");
        assert_eq!(result.placeholders.len(), 2);
        assert_eq!(result.placeholders[0].name, "id");
        assert_eq!(result.placeholders[1].name, "date");
    }

    #[test]
    fn test_parse_placeholder_with_type() {
        let result = parse_custom_glob("**/<year:int>/<month:int>/*.csv").unwrap();
        assert_eq!(result.placeholders[0].type_hint, Some("int".into()));
        assert_eq!(result.placeholders[1].type_hint, Some("int".into()));
    }

    #[test]
    fn test_parse_escaped_brackets() {
        let result = parse_custom_glob(r"**/\<notfield\>/*.csv").unwrap();
        assert_eq!(result.glob_pattern, "**/<notfield>/*.csv");
        assert!(result.placeholders.is_empty());
    }

    #[test]
    fn test_error_unclosed_placeholder() {
        let result = parse_custom_glob("**/mission_<id/*.csv");
        assert!(matches!(
            result,
            Err(GlobParseError { message, .. }) if message.contains("Unclosed")
        ));
    }

    #[test]
    fn test_error_empty_field_name() {
        let result = parse_custom_glob("**/mission_<>/*.csv");
        assert!(matches!(
            result,
            Err(GlobParseError { message, .. }) if message.contains("Empty")
        ));
    }

    #[test]
    fn test_error_invalid_field_name() {
        let result = parse_custom_glob("**/mission_<ID>/*.csv");
        assert!(matches!(
            result,
            Err(GlobParseError { message, .. }) if message.contains("Invalid")
        ));
    }

    #[test]
    fn test_error_duplicate_field_name() {
        let result = parse_custom_glob("**/<id>/<id>/*.csv");
        assert!(matches!(
            result,
            Err(GlobParseError { message, .. }) if message.contains("Duplicate")
        ));
    }

    #[test]
    fn test_error_unknown_type() {
        let result = parse_custom_glob("**/<field:unknown>/*.csv");
        assert!(matches!(
            result,
            Err(GlobParseError { message, .. }) if message.contains("Unknown type")
        ));
    }

    #[test]
    fn test_no_placeholders() {
        let result = parse_custom_glob("**/*.csv").unwrap();
        assert_eq!(result.glob_pattern, "**/*.csv");
        assert!(result.placeholders.is_empty());
    }

    #[test]
    fn test_extract_simple_value() {
        let parsed = parse_custom_glob("**/mission_<id>/*.csv").unwrap();
        let values = extract_field_values("/data/mission_42/report.csv", &parsed);
        assert_eq!(values.get("id"), Some(&"42".to_string()));
    }

    #[test]
    fn test_extract_multiple_values() {
        let parsed = parse_custom_glob("**/<year>/<month>/*.csv").unwrap();
        let values = extract_field_values("/data/2024/01/report.csv", &parsed);
        assert_eq!(values.get("year"), Some(&"2024".to_string()));
        assert_eq!(values.get("month"), Some(&"01".to_string()));
    }

    #[test]
    fn test_valid_field_names() {
        assert!(is_valid_field_name("id"));
        assert!(is_valid_field_name("mission_id"));
        assert!(is_valid_field_name("_private"));
        assert!(is_valid_field_name("field123"));
        assert!(!is_valid_field_name("ID"));           // uppercase
        assert!(!is_valid_field_name("123field"));     // starts with digit
        assert!(!is_valid_field_name("field-name"));   // contains hyphen
        assert!(!is_valid_field_name(""));             // empty
    }

    // =========================================================================
    // Schema-First Algorithm Tests (RULE_BUILDER_UI_PLAN.md)
    // =========================================================================

    #[test]
    fn test_normalize_token_date_formats() {
        // YYYY-MM-DD
        assert_eq!(normalize_token("2024-01-15"), NormalizedToken::Date);
        assert_eq!(normalize_token("2024/01/15"), NormalizedToken::Date);

        // YYYYMMDD
        assert_eq!(normalize_token("20240115"), NormalizedToken::Date);

        // YYYY-MM
        assert_eq!(normalize_token("2024-01"), NormalizedToken::Date);
        assert_eq!(normalize_token("2024/01"), NormalizedToken::Date);

        // YYYY (valid year range)
        assert_eq!(normalize_token("2024"), NormalizedToken::Date);
        assert_eq!(normalize_token("1999"), NormalizedToken::Date);

        // Not a year (too small/large)
        assert_eq!(normalize_token("1800"), NormalizedToken::Integer);
        assert_eq!(normalize_token("2500"), NormalizedToken::Integer);
    }

    #[test]
    fn test_normalize_token_uuid() {
        assert_eq!(
            normalize_token("550e8400-e29b-41d4-a716-446655440000"),
            NormalizedToken::Uuid
        );
        // Not a UUID (wrong format)
        assert!(matches!(
            normalize_token("550e8400-e29b-41d4-a716"),
            NormalizedToken::Literal(_)
        ));
    }

    #[test]
    fn test_normalize_token_integer() {
        assert_eq!(normalize_token("42"), NormalizedToken::Integer);
        assert_eq!(normalize_token("001"), NormalizedToken::Integer);
        assert_eq!(normalize_token("12345"), NormalizedToken::Integer);
        // Single digit is literal
        assert_eq!(normalize_token("5"), NormalizedToken::Literal("5".to_string()));
    }

    #[test]
    fn test_normalize_token_literal() {
        assert_eq!(normalize_token("mission"), NormalizedToken::Literal("mission".to_string()));
        assert_eq!(normalize_token("Report"), NormalizedToken::Literal("report".to_string()));
    }

    #[test]
    fn test_tokenize_segment() {
        assert_eq!(tokenize_segment("mission_042"), vec!["mission", "042"]);
        assert_eq!(tokenize_segment("report-2024-01"), vec!["report", "2024", "01"]);
        assert_eq!(tokenize_segment("file.csv"), vec!["file", "csv"]);
        assert_eq!(tokenize_segment("a_b-c.d"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_normalize_path() {
        // Path with dates and IDs
        let path = "ops/mission_042/2024-01-15";
        let normalized = normalize_path(path);
        assert_eq!(normalized, "ops/mission_<n>/<date>");

        // Path with all literals
        let path = "data/sales/reports";
        let normalized = normalize_path(path);
        assert_eq!(normalized, "data/sales/reports");
    }

    #[test]
    fn test_extract_path_archetypes() {
        let paths = vec![
            "ops/mission_001/2024-01-01/telemetry.csv".to_string(),
            "ops/mission_001/2024-01-02/telemetry.csv".to_string(),
            "ops/mission_002/2024-01-01/telemetry.csv".to_string(),
            "data/reports/summary.csv".to_string(),
        ];

        let archetypes = extract_path_archetypes(&paths, 10);
        assert!(!archetypes.is_empty());

        // The mission paths should be grouped together
        let mission_archetype = archetypes.iter()
            .find(|a| a.template.contains("mission"));
        assert!(mission_archetype.is_some());
        assert_eq!(mission_archetype.unwrap().file_count, 3);
    }

    #[test]
    fn test_extract_naming_schemes() {
        // Use paths with same literal prefix/suffix but varying numeric/date parts
        // so they group together in the same template
        let paths = vec![
            "data/report_2024-01-01_final.csv".to_string(),
            "data/report_2024-01-02_final.csv".to_string(),
            "data/report_2024-02-15_final.csv".to_string(),
            "data/summary.txt".to_string(),
        ];

        let schemes = extract_naming_schemes(&paths, 10);
        assert!(!schemes.is_empty());

        // Should find the report_<date>_<n>_<n>_final.csv pattern (3 files)
        let report_scheme = schemes.iter()
            .find(|s| s.template.contains("report"));
        assert!(report_scheme.is_some());
        // All 3 report files share the same template
        assert_eq!(report_scheme.unwrap().file_count, 3);
    }

    #[test]
    fn test_extract_pattern_seeds() {
        let paths = vec![
            "data/file1.csv".to_string(),
            "data/file2.csv".to_string(),
            "logs/app.log".to_string(),
            "docs/readme.md".to_string(),
        ];

        let seeds = extract_pattern_seeds(&paths, 10);
        assert!(!seeds.is_empty());

        // CSV should be the top seed (2 files)
        assert_eq!(seeds[0].pattern, "**/*.csv");
        assert_eq!(seeds[0].match_count, 2);
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(edit_distance("msn", "mission"), 4);
        assert_eq!(edit_distance("cfg", "config"), 3);
        assert_eq!(edit_distance("cat", "car"), 1);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("same", "same"), 0);
    }

    #[test]
    fn test_detect_synonyms_lexicon() {
        // Build paths with known abbreviations
        let paths: Vec<String> = (0..50)
            .map(|i| format!("ops/msn/flight_{:03}/data.csv", i))
            .collect();

        let synonyms = detect_synonyms(&paths, 30);

        // Should find msn -> mission from lexicon
        let msn_match = synonyms.iter()
            .find(|s| s.short_form == "msn");
        assert!(msn_match.is_some());
        assert_eq!(msn_match.unwrap().canonical_form, "mission");
        assert_eq!(msn_match.unwrap().confidence, SynonymConfidence::High);
    }

    #[test]
    fn test_detect_synonyms_insufficient_data() {
        // Less than min_sample_count
        let paths: Vec<String> = (0..10)
            .map(|i| format!("data/file_{}.csv", i))
            .collect();

        let synonyms = detect_synonyms(&paths, 30);
        assert!(synonyms.is_empty());
    }

    #[test]
    fn test_analyze_paths_for_schema_ui() {
        // Use filenames with variable parts (numeric suffixes) so naming_schemes is populated
        let paths: Vec<String> = (0..100)
            .map(|i| format!("ops/mission_{:03}/2024-01-{:02}/telemetry_{}.csv", i % 10, i % 28 + 1, i))
            .collect();

        let mut state = RuleBuilderState::default();
        analyze_paths_for_schema_ui(&mut state, &paths, 8);

        // Should populate all schema-first fields
        assert!(!state.pattern_seeds.is_empty());
        assert!(!state.path_archetypes.is_empty());
        assert!(!state.naming_schemes.is_empty(), "naming_schemes should not be empty - paths have variable numeric parts in filenames");
        // Synonyms may or may not be found depending on path content
    }
}
