//! Extraction Rule Types for Glob Explorer
//!
//! Types for defining, testing, and publishing extraction rules via the TUI.
//! Aligned with specs/views/discover.md v2.2 Phase 18 and specs/extraction.md.

use std::collections::HashMap;
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
