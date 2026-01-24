//! Core types for the Intent Pipeline workflow.
//!
//! All artifact JSON is canonicalized for hashing (sorted keys, stable arrays).

pub use casparian_intent::SessionId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

// ============================================================================
// Core IDs
// ============================================================================

/// FileSet identifier (UUID)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileSetId(Uuid);

impl FileSetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for FileSetId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for FileSetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for FileSetId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Proposal identifier (UUID)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProposalId(Uuid);

impl ProposalId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ProposalId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProposalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ProposalId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Question identifier (UUID)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QuestionId(Uuid);

impl QuestionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for QuestionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for QuestionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Sampling Methods
// ============================================================================

/// How a file set was sampled
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SamplingMethod {
    /// All files included
    All,
    /// Deterministic sample with seed
    DeterministicSample,
    /// Stratified sample (e.g., by directory, extension)
    StratifiedSample,
    /// Top K failing files
    TopKFailures,
}

// ============================================================================
// FileSet Metadata
// ============================================================================

/// Metadata about a file set (no inline file lists!)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSetMeta {
    pub file_set_id: FileSetId,
    pub count: u64,
    pub sampling_method: SamplingMethod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Path to manifest file (relative to session bundle)
    pub manifest_ref: String,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Evidence Types
// ============================================================================

/// Directory prefix with count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirPrefixEvidence {
    pub prefix: String,
    pub count: u64,
}

/// Extension with count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionEvidence {
    pub ext: String,
    pub count: u64,
}

/// Semantic token with count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticTokenEvidence {
    pub token: String,
    pub count: u64,
}

/// Tag collision evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagCollisionEvidence {
    pub tag: String,
    pub count: u64,
}

/// Selection evidence (bounded)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionEvidence {
    #[serde(default)]
    pub top_dir_prefixes: Vec<DirPrefixEvidence>,
    #[serde(default)]
    pub extensions: Vec<ExtensionEvidence>,
    #[serde(default)]
    pub semantic_tokens: Vec<SemanticTokenEvidence>,
    #[serde(default)]
    pub collision_with_existing_tags: Vec<TagCollisionEvidence>,
}

// ============================================================================
// Confidence
// ============================================================================

/// Confidence label
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfidenceLabel {
    Low,
    Med,
    High,
}

impl ConfidenceLabel {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            ConfidenceLabel::High
        } else if score >= 0.5 {
            ConfidenceLabel::Med
        } else {
            ConfidenceLabel::Low
        }
    }
}

/// Confidence with score, label, and reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confidence {
    pub score: f64,
    pub label: ConfidenceLabel,
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl Confidence {
    pub fn new(score: f64, reasons: Vec<String>) -> Self {
        Self {
            score,
            label: ConfidenceLabel::from_score(score),
            reasons,
        }
    }

    pub fn high(reasons: Vec<String>) -> Self {
        Self::new(0.9, reasons)
    }

    pub fn medium(reasons: Vec<String>) -> Self {
        Self::new(0.6, reasons)
    }

    pub fn low(reasons: Vec<String>) -> Self {
        Self::new(0.3, reasons)
    }
}

// ============================================================================
// Next Actions
// ============================================================================

/// Possible next actions after a proposal
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NextAction {
    AskHumanConfirmSelection,
    ProposeTagRules,
    AskHumanConfirmTagRules,
    ProposePathFields,
    AskHumanConfirmPathFields,
    InferSchemaIntent,
    AskHumanResolveAmbiguity,
    GenerateParserDraft,
    StartBacktest,
    ApplyPatch,
    PromoteSchema,
    CreatePublishPlan,
    AskHumanConfirmPublish,
    ExecutePublish,
    CreateRunPlan,
    AskHumanConfirmRun,
    ExecuteRun,
}

// ============================================================================
// Selection Proposal
// ============================================================================

/// Preview of selected and near-miss files (bounded)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionPreview {
    #[serde(default)]
    pub selected_examples: Vec<String>,
    #[serde(default)]
    pub near_miss_examples: Vec<String>,
}

/// Selection proposal (§8.2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionProposal {
    pub proposal_id: ProposalId,
    pub proposal_hash: String,
    pub selected_file_set_id: FileSetId,
    pub near_miss_file_set_id: FileSetId,
    pub evidence: SelectionEvidence,
    pub confidence: Confidence,
    pub preview: SelectionPreview,
    #[serde(default)]
    pub next_actions: Vec<NextAction>,
}

impl SelectionProposal {
    /// Compute canonical hash of the proposal
    pub fn compute_hash(&self) -> String {
        let canonical = serde_json::to_string(&CanonicalSelectionProposal {
            selected_file_set_id: self.selected_file_set_id,
            near_miss_file_set_id: self.near_miss_file_set_id,
        })
        .expect("serialization should not fail");

        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[derive(Serialize)]
struct CanonicalSelectionProposal {
    selected_file_set_id: FileSetId,
    near_miss_file_set_id: FileSetId,
}

// ============================================================================
// Tag Rule Types (§8.3, §8.4, §8.5)
// ============================================================================

/// Magic bytes condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagicBytesCondition {
    pub offset: usize,
    pub hex: String,
}

/// Tag rule "when" conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRuleWhen {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_glob: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extension: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub magic_bytes: Vec<MagicBytesCondition>,
}

/// Tag rule DSL (§8.3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRule {
    pub rule_id: String,
    #[serde(default)]
    pub enabled: bool,
    pub when: TagRuleWhen,
    #[serde(default)]
    pub add_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_to_topic: Option<String>,
}

/// Rule conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConflict {
    pub existing_rule_id: String,
    pub overlap_count: u64,
}

/// Rule evaluation sampling info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluationSampling {
    pub method: String,
    pub seed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Rule evaluation examples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluationExamples {
    #[serde(default)]
    pub matches: Vec<String>,
    #[serde(default)]
    pub near_misses: Vec<String>,
    #[serde(default)]
    pub false_positive_examples: Vec<String>,
}

/// Rule evaluation (§8.4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluation {
    pub matched_file_set_id: FileSetId,
    pub negative_sample_file_set_id: FileSetId,
    pub precision_estimate: f64,
    pub recall_estimate: f64,
    pub false_positive_estimate: f64,
    #[serde(default)]
    pub conflicts: Vec<RuleConflict>,
    pub examples: RuleEvaluationExamples,
    pub sampling: RuleEvaluationSampling,
}

/// Tag rule candidate with evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRuleCandidate {
    pub rule: TagRule,
    pub evaluation: RuleEvaluation,
    pub confidence: Confidence,
}

/// Tag rule proposal (§8.5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRuleProposal {
    pub proposal_id: ProposalId,
    pub proposal_hash: String,
    #[serde(default)]
    pub candidates: Vec<TagRuleCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_rule_id: Option<String>,
    #[serde(default)]
    pub required_human_questions: Vec<HumanQuestion>,
}

// ============================================================================
// Path Field Types (§8.6)
// ============================================================================

/// Path field pattern kind
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathFieldPattern {
    KeyValue { value: String },
    Regex { value: String },
    SegmentPosition { value: usize },
    PartitionDir { value: String },
}

/// Path field source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathFieldSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename_group: Option<String>,
}

/// Path field coverage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathFieldCoverage {
    pub matched_files: u64,
    pub total_files: u64,
}

/// Path field dtype
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathFieldDtype {
    String,
    Int,
    Date,
    Timestamp,
}

/// A derived path field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathField {
    pub field_name: String,
    pub dtype: PathFieldDtype,
    pub pattern: PathFieldPattern,
    pub source: PathFieldSource,
    pub coverage: PathFieldCoverage,
    #[serde(default)]
    pub examples: Vec<String>,
    pub confidence: Confidence,
}

/// Namespacing config for path fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathFieldNamespacing {
    pub default_prefix: String,
    pub allow_promote: bool,
}

impl Default for PathFieldNamespacing {
    fn default() -> Self {
        Self {
            default_prefix: "_cf_path_".to_string(),
            allow_promote: true,
        }
    }
}

/// Same name, different values collision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SameNameCollision {
    pub field_name: String,
    #[serde(default)]
    pub example_paths: Vec<String>,
}

/// Segment overlap collision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentOverlapCollision {
    pub segment_index: usize,
    #[serde(default)]
    pub field_names: Vec<String>,
}

/// Collision with parsed columns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedColumnCollision {
    pub derived_field: String,
    pub parsed_column: String,
}

/// Path field collisions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathFieldCollisions {
    #[serde(default)]
    pub same_name_different_values: Vec<SameNameCollision>,
    #[serde(default)]
    pub segment_overlap: Vec<SegmentOverlapCollision>,
    #[serde(default)]
    pub with_parsed_columns: Vec<ParsedColumnCollision>,
}

/// Path field proposal (§8.6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathFieldProposal {
    pub proposal_id: ProposalId,
    pub proposal_hash: String,
    pub input_file_set_id: FileSetId,
    #[serde(default)]
    pub namespacing: PathFieldNamespacing,
    #[serde(default)]
    pub fields: Vec<PathField>,
    #[serde(default)]
    pub collisions: PathFieldCollisions,
    #[serde(default)]
    pub required_human_questions: Vec<HumanQuestion>,
}

// ============================================================================
// Schema Intent Types (§8.7)
// ============================================================================

/// Schema intent input sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaIntentSources {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_output_sample_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_fields_ref: Option<String>,
}

/// Column source (parsed vs derived)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnSource {
    Parsed,
    Derived,
}

/// Type inference method
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceMethod {
    ConstraintElimination,
    AmbiguousRequiresHuman,
}

/// Type inference evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceEvidence {
    pub null_rate: f64,
    pub distinct: u64,
    pub format_hits: u64,
}

/// Column type inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInference {
    pub method: InferenceMethod,
    #[serde(default)]
    pub candidates: Vec<String>,
    pub evidence: InferenceEvidence,
    pub confidence: Confidence,
}

/// Column constraints
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ColumnConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Value>,
}

/// Schema column definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaIntentColumn {
    pub name: String,
    pub source: ColumnSource,
    pub declared_type: String,
    pub nullable: bool,
    #[serde(default)]
    pub constraints: ColumnConstraints,
    pub inference: ColumnInference,
}

/// Column collision resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionResolution {
    Namespace,
    Rename,
    RequiredHuman,
}

/// Column collision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnCollision {
    pub left: String,
    pub right: String,
    pub resolution: CollisionResolution,
}

/// Safe defaults for schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaSafeDefaults {
    pub timestamp_timezone: String,
    pub string_truncation: String,
    pub numeric_overflow: String,
}

impl Default for SchemaSafeDefaults {
    fn default() -> Self {
        Self {
            timestamp_timezone: "require_utc".to_string(),
            string_truncation: "reject".to_string(),
            numeric_overflow: "reject".to_string(),
        }
    }
}

/// Schema intent proposal (§8.7)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaIntentProposal {
    pub proposal_id: ProposalId,
    pub proposal_hash: String,
    pub input_sources: SchemaIntentSources,
    #[serde(default)]
    pub columns: Vec<SchemaIntentColumn>,
    #[serde(default)]
    pub column_collisions: Vec<ColumnCollision>,
    #[serde(default)]
    pub safe_defaults: SchemaSafeDefaults,
    #[serde(default)]
    pub required_human_questions: Vec<HumanQuestion>,
}

// ============================================================================
// Parser Draft (§8.8)
// ============================================================================

/// Parser identity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserIdentity {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub topics: Vec<String>,
    pub source_hash: String,
}

/// Build/lint status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildStatus {
    Pass,
    Fail,
}

/// Parser draft (§8.8)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserDraft {
    pub draft_id: ProposalId,
    pub parser_identity: ParserIdentity,
    pub repo_ref: String,
    #[serde(default)]
    pub entrypoints: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests_ref: Option<String>,
    pub build_status: BuildStatus,
    pub lint_status: BuildStatus,
}

// ============================================================================
// Backtest Types (§8.9, §8.10)
// ============================================================================

/// Backtest phase
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BacktestPhase {
    Scan,
    Parse,
    Validate,
    Summarize,
}

/// Backtest metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestMetrics {
    pub files_processed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_total_estimate: Option<u64>,
    pub rows_emitted: u64,
    pub rows_quarantined: u64,
}

/// Top column in violation summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationTopColumn {
    pub name: String,
    pub count: u64,
}

/// Violation summary entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationSummaryEntry {
    pub violation_type: String,
    pub count: u64,
    #[serde(default)]
    pub top_columns: Vec<ViolationTopColumn>,
}

/// Backtest progress envelope (§8.9)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestProgressEnvelope {
    pub job_id: String,
    pub phase: BacktestPhase,
    pub elapsed_ms: u64,
    pub metrics: BacktestMetrics,
    #[serde(default)]
    pub top_violation_summary: Vec<ViolationSummaryEntry>,
    pub stalled: bool,
}

/// Backtest quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestQuality {
    pub files_processed: u64,
    pub rows_emitted: u64,
    pub rows_quarantined: u64,
    pub quarantine_pct: f64,
    pub pass_rate_files: f64,
}

/// Suggestion code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationSuggestion {
    pub code: String,
    pub confidence: f64,
}

/// Observed type distribution
pub type ObservedTypes = BTreeMap<String, f64>;

/// Violation example context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationExampleContext {
    pub column: String,
    pub expected: String,
    pub observed_types: ObservedTypes,
    #[serde(default)]
    pub sample_values: Vec<String>,
    #[serde(default)]
    pub suggestions: Vec<ViolationSuggestion>,
}

/// Top K violation entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopKViolation {
    pub violation_type: String,
    pub count: u64,
    #[serde(default)]
    pub top_columns: Vec<ViolationTopColumn>,
    #[serde(default)]
    pub example_contexts: Vec<ViolationExampleContext>,
}

/// Backtest report (§8.10)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestReport {
    pub job_id: String,
    pub input_file_set_id: FileSetId,
    pub iterations_ref: String,
    pub quality: BacktestQuality,
    #[serde(default)]
    pub top_k_violations: Vec<TopKViolation>,
    pub full_report_ref: String,
}

// ============================================================================
// Publish Plan (§8.11)
// ============================================================================

/// Schema publish info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishSchemaInfo {
    pub schema_name: String,
    pub new_version: String,
    pub schema_as_code_ref: String,
    pub compiled_schema_ref: String,
}

/// Parser publish info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishParserInfo {
    pub name: String,
    pub new_version: String,
    pub source_hash: String,
    #[serde(default)]
    pub topics: Vec<String>,
}

/// Publish invariants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishInvariants {
    pub route_to_topic_in_parser_topics: bool,
    pub no_same_name_version_different_hash: bool,
    pub sink_validation_passed: bool,
}

/// Publish plan (§8.11)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishPlan {
    pub proposal_id: ProposalId,
    pub proposal_hash: String,
    pub schema: PublishSchemaInfo,
    pub parser: PublishParserInfo,
    pub invariants: PublishInvariants,
    #[serde(default)]
    pub diff_summary: Vec<String>,
    #[serde(default)]
    pub required_human_questions: Vec<HumanQuestion>,
}

// ============================================================================
// Run Plan (§8.12)
// ============================================================================

/// Sink configuration for run plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlanSink {
    #[serde(rename = "type")]
    pub sink_type: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duckdb_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duckdb_table: Option<String>,
}

/// Write policy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritePolicy {
    NewJobPartition,
    ErrorIfJobExists,
}

/// Job partitioning config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPartitioning {
    pub mode: String,
    pub pattern: String,
}

impl Default for JobPartitioning {
    fn default() -> Self {
        Self {
            mode: "by_job_id".to_string(),
            pattern: "{output}_{job_id}.parquet".to_string(),
        }
    }
}

/// Run plan validations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlanValidations {
    pub sink_valid: bool,
    pub topic_mapping_valid: bool,
}

/// Estimated cost
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimatedCost {
    pub files: u64,
    pub size_bytes: u64,
}

/// Run plan (§8.12)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPlan {
    pub proposal_id: ProposalId,
    pub proposal_hash: String,
    pub input_file_set_id: FileSetId,
    pub route_to_topic: String,
    pub parser_identity: ParserIdentity,
    pub sink: RunPlanSink,
    pub write_policy: WritePolicy,
    #[serde(default)]
    pub job_partitioning: JobPartitioning,
    pub validations: RunPlanValidations,
    pub estimated_cost: EstimatedCost,
}

// ============================================================================
// Human Question (§8.13)
// ============================================================================

/// Question kind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuestionKind {
    ConfirmSelection,
    ResolveAmbiguity,
    ResolveCollision,
    ConfirmPublish,
    ConfirmRun,
}

/// Question option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub option_id: String,
    pub label: String,
    pub consequence: String,
    #[serde(default)]
    pub default: bool,
}

/// Human question (§8.13)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanQuestion {
    pub question_id: QuestionId,
    pub kind: QuestionKind,
    pub prompt: String,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
}

// ============================================================================
// Decision Record (§8.14)
// ============================================================================

/// Decision (approve/reject)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Reject,
}

/// Decision target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTarget {
    pub proposal_id: ProposalId,
    pub approval_target_hash: String,
}

/// Decision record (§8.14)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub timestamp: DateTime<Utc>,
    pub actor: String,
    pub decision: Decision,
    pub target: DecisionTarget,
    #[serde(default)]
    pub choice_payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

// ============================================================================
// Approval Token (§4.4)
// ============================================================================

/// Approval token - single-use, bound to exact choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalToken {
    pub approval_target_hash: String,
    pub session_id: SessionId,
    pub proposal_id: ProposalId,
    pub nonce: String,
    pub created_at: DateTime<Utc>,
    pub consumed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<DateTime<Utc>>,
}

impl ApprovalToken {
    /// Create a new approval token
    pub fn new(
        session_id: SessionId,
        proposal_id: ProposalId,
        choice_payload: &serde_json::Value,
    ) -> Self {
        let nonce = Uuid::new_v4().to_string();
        let approval_target_hash =
            Self::compute_hash(session_id, proposal_id, choice_payload, &nonce);

        Self {
            approval_target_hash,
            session_id,
            proposal_id,
            nonce,
            created_at: Utc::now(),
            consumed: false,
            consumed_at: None,
        }
    }

    /// Compute the approval target hash
    pub fn compute_hash(
        session_id: SessionId,
        proposal_id: ProposalId,
        choice_payload: &serde_json::Value,
        nonce: &str,
    ) -> String {
        let canonical = serde_json::to_string(&serde_json::json!({
            "session_id": session_id,
            "proposal_id": proposal_id,
            "choice_payload": choice_payload,
            "nonce": nonce,
        }))
        .expect("serialization should not fail");

        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verify the token matches the expected values
    pub fn verify(
        &self,
        session_id: SessionId,
        proposal_id: ProposalId,
        choice_payload: &serde_json::Value,
    ) -> bool {
        if self.consumed {
            return false;
        }

        let expected_hash =
            Self::compute_hash(session_id, proposal_id, choice_payload, &self.nonce);
        self.approval_target_hash == expected_hash
    }

    /// Mark the token as consumed
    pub fn consume(&mut self) {
        self.consumed = true;
        self.consumed_at = Some(Utc::now());
    }
}

// ============================================================================
// Session Manifest (§8.1)
// ============================================================================

/// Artifact reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: String,
}

/// Session bundle manifest (§8.1)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManifest {
    pub session_id: SessionId,
    pub created_at: DateTime<Utc>,
    pub intent_text: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corpus_manifest_ref: Option<String>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_roundtrip() {
        let id = SessionId::new();
        let s = id.to_string();
        let parsed: SessionId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_file_set_id_roundtrip() {
        let id = FileSetId::new();
        let s = id.to_string();
        let parsed: FileSetId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_confidence_from_score() {
        assert_eq!(ConfidenceLabel::from_score(0.9), ConfidenceLabel::High);
        assert_eq!(ConfidenceLabel::from_score(0.8), ConfidenceLabel::High);
        assert_eq!(ConfidenceLabel::from_score(0.6), ConfidenceLabel::Med);
        assert_eq!(ConfidenceLabel::from_score(0.5), ConfidenceLabel::Med);
        assert_eq!(ConfidenceLabel::from_score(0.3), ConfidenceLabel::Low);
        assert_eq!(ConfidenceLabel::from_score(0.0), ConfidenceLabel::Low);
    }

    #[test]
    fn test_approval_token_verify() {
        let session_id = SessionId::new();
        let proposal_id = ProposalId::new();
        let choice_payload = serde_json::json!({"selected_rule_id": "rule_1"});

        let token = ApprovalToken::new(session_id, proposal_id, &choice_payload);

        // Should verify with correct values
        assert!(token.verify(session_id, proposal_id, &choice_payload));

        // Should fail with wrong payload
        let wrong_payload = serde_json::json!({"selected_rule_id": "rule_2"});
        assert!(!token.verify(session_id, proposal_id, &wrong_payload));

        // Should fail with wrong session
        let wrong_session = SessionId::new();
        assert!(!token.verify(wrong_session, proposal_id, &choice_payload));
    }

    #[test]
    fn test_approval_token_single_use() {
        let session_id = SessionId::new();
        let proposal_id = ProposalId::new();
        let choice_payload = serde_json::json!({});

        let mut token = ApprovalToken::new(session_id, proposal_id, &choice_payload);

        assert!(!token.consumed);
        assert!(token.verify(session_id, proposal_id, &choice_payload));

        token.consume();

        assert!(token.consumed);
        assert!(token.consumed_at.is_some());
        // Should fail verification after consumption
        assert!(!token.verify(session_id, proposal_id, &choice_payload));
    }

    #[test]
    fn test_selection_proposal_hash() {
        let proposal = SelectionProposal {
            proposal_id: ProposalId::new(),
            proposal_hash: String::new(),
            selected_file_set_id: FileSetId::new(),
            near_miss_file_set_id: FileSetId::new(),
            evidence: SelectionEvidence {
                top_dir_prefixes: vec![],
                extensions: vec![],
                semantic_tokens: vec![],
                collision_with_existing_tags: vec![],
            },
            confidence: Confidence::high(vec![]),
            preview: SelectionPreview {
                selected_examples: vec![],
                near_miss_examples: vec![],
            },
            next_actions: vec![],
        };

        let hash = proposal.compute_hash();
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 hex
    }
}
