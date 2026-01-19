//! Core types for MCP server
//!
//! Includes wrapper types for domain IDs and the Tool trait definition.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use casparian_ids::{BacktestId, ContractId, ScopeId};

// =============================================================================
// Tool Error Types
// =============================================================================

/// Errors that can occur during tool execution
#[derive(Debug, Error)]
pub enum ToolError {
    /// Invalid parameters provided to the tool
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    /// Resource not found (scope, contract, backtest, etc.)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Tool execution failed
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {message}")]
    Serialization {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// I/O error
    #[error("I/O error: {message}")]
    Io {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl From<serde_json::Error> for ToolError {
    fn from(err: serde_json::Error) -> Self {
        ToolError::Serialization {
            message: err.to_string(),
            source: Some(Box::new(err)),
        }
    }
}

impl From<std::io::Error> for ToolError {
    fn from(err: std::io::Error) -> Self {
        ToolError::Io {
            message: err.to_string(),
            source: Some(Box::new(err)),
        }
    }
}

impl ToolError {
    /// Get the JSON-RPC error code for this error type
    pub fn error_code(&self) -> i32 {
        match self {
            ToolError::InvalidParams(_) => -32602, // Invalid params
            ToolError::NotFound(_) => -32001,      // Custom: not found
            ToolError::ExecutionFailed(_) => -32002, // Custom: execution failed
            ToolError::Internal(_) => -32603,      // Internal error
            ToolError::Serialization { .. } => -32700, // Parse error
            ToolError::Io { .. } => -32603,            // Internal error
        }
    }
}

// =============================================================================
// Tool Trait
// =============================================================================

/// JSON Schema for tool input parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputSchema {
    /// Schema type (always "object" for MCP tools)
    #[serde(rename = "type")]
    pub schema_type: String,

    /// Property definitions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Value>,

    /// Required property names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

impl ToolInputSchema {
    /// Create a new schema with object type
    pub fn new() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: None,
            required: None,
        }
    }

    /// Create a schema with properties
    pub fn with_properties(properties: serde_json::Value, required: Vec<String>) -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: if required.is_empty() {
                None
            } else {
                Some(required)
            },
        }
    }
}

impl Default for ToolInputSchema {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Content blocks returned by the tool
    pub content: Vec<ToolContent>,

    /// Whether this result indicates an error
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

impl ToolResult {
    /// Create a successful text result
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: text.into(),
            }],
            is_error: false,
        }
    }

    /// Create a successful JSON result
    pub fn json<T: Serialize>(value: &T) -> Result<Self, ToolError> {
        let text = serde_json::to_string_pretty(value)?;
        Ok(Self {
            content: vec![ToolContent::Text { text }],
            is_error: false,
        })
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text {
                text: message.into(),
            }],
            is_error: true,
        }
    }
}

/// Content types that can be returned by tools
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    /// Text content
    Text { text: String },

    /// Image content (base64 encoded)
    Image { data: String, mime_type: String },

    /// Resource reference
    Resource { uri: String, mime_type: Option<String> },
}

/// Trait for implementing MCP tools
///
/// Each tool must provide:
/// - A unique name
/// - A description for Claude to understand when to use it
/// - An input schema defining expected parameters
/// - An async execute method that performs the tool's action
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name of the tool
    fn name(&self) -> &str;

    /// Human-readable description of what the tool does
    fn description(&self) -> &str;

    /// JSON Schema for input parameters
    fn input_schema(&self) -> ToolInputSchema;

    /// Execute the tool with the given arguments
    ///
    /// # Arguments
    /// * `args` - JSON object containing tool parameters
    ///
    /// # Returns
    /// * `Ok(ToolResult)` - Tool execution result
    /// * `Err(ToolError)` - Error during execution
    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError>;
}

// =============================================================================
// Human Approval Protocol Types
// =============================================================================

/// Workflow phase identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPhase {
    /// Initial file discovery
    Discovery,
    /// Schema inference from files
    SchemaInference,
    /// Waiting for schema approval
    SchemaApproval,
    /// Generating parser code
    ParserGeneration,
    /// Running backtest against files
    Backtest,
    /// Refining parser based on failures
    ParserRefinement,
    /// Executing the full pipeline
    Execution,
    /// Verifying output
    Verification,
}

impl std::fmt::Display for WorkflowPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkflowPhase::Discovery => write!(f, "discovery"),
            WorkflowPhase::SchemaInference => write!(f, "schema_inference"),
            WorkflowPhase::SchemaApproval => write!(f, "schema_approval"),
            WorkflowPhase::ParserGeneration => write!(f, "parser_generation"),
            WorkflowPhase::Backtest => write!(f, "backtest"),
            WorkflowPhase::ParserRefinement => write!(f, "parser_refinement"),
            WorkflowPhase::Execution => write!(f, "execution"),
            WorkflowPhase::Verification => write!(f, "verification"),
        }
    }
}

/// A decision option for human input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOption {
    /// Unique identifier for this option
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Detailed description of what this option does
    pub description: String,
}

impl DecisionOption {
    /// Create a new decision option
    pub fn new(id: impl Into<String>, label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: description.into(),
        }
    }
}

/// Human decision required
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDecision {
    /// What needs to be decided
    pub question: String,
    /// Available options
    pub options: Vec<DecisionOption>,
    /// Whether this blocks progress
    pub blocking: bool,
    /// Default option if human doesn't respond
    pub default_option: Option<String>,
    /// Context for the decision (column name, file path, etc.)
    pub context: Option<String>,
}

impl HumanDecision {
    /// Create a new human decision
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            options: vec![],
            blocking: true,
            default_option: None,
            context: None,
        }
    }

    /// Add options to the decision
    pub fn with_options(mut self, options: Vec<DecisionOption>) -> Self {
        self.options = options;
        self
    }

    /// Set whether this decision blocks progress
    pub fn blocking(mut self, blocking: bool) -> Self {
        self.blocking = blocking;
        self
    }

    /// Set the default option
    pub fn with_default(mut self, default_option: impl Into<String>) -> Self {
        self.default_option = Some(default_option.into());
        self
    }

    /// Add context to the decision
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// An action that can be taken next in the workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextAction {
    /// Tool name to invoke
    pub tool_name: String,
    /// Human-readable description
    pub description: String,
    /// Whether this action requires human approval
    pub requires_approval: bool,
    /// Whether Claude should automatically suggest this action
    pub auto_suggested: bool,
    /// Estimated parameters for the tool call (optional hints)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_params: Option<serde_json::Value>,
}

impl NextAction {
    /// Create a new next action
    pub fn new(tool_name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            description: description.into(),
            requires_approval: false,
            auto_suggested: false,
            suggested_params: None,
        }
    }

    /// Mark as requiring approval
    pub fn requires_approval(mut self) -> Self {
        self.requires_approval = true;
        self
    }

    /// Mark as auto-suggested
    pub fn auto_suggested(mut self) -> Self {
        self.auto_suggested = true;
        self
    }

    /// Add suggested parameters
    pub fn with_params(mut self, params: serde_json::Value) -> Self {
        self.suggested_params = Some(params);
        self
    }
}

/// Progress indicator for multi-step operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    /// Current step number (1-indexed)
    pub current_step: u32,
    /// Total number of steps
    pub total_steps: u32,
    /// Name of the current step
    pub step_name: String,
    /// Percentage complete (0-100)
    pub percentage: u8,
}

impl Progress {
    /// Create a new progress indicator
    pub fn new(current_step: u32, total_steps: u32, step_name: impl Into<String>) -> Self {
        let percentage = if total_steps > 0 {
            ((current_step as f32 / total_steps as f32) * 100.0) as u8
        } else {
            0
        };
        Self {
            current_step,
            total_steps,
            step_name: step_name.into(),
            percentage,
        }
    }
}

/// Bulk approval option for filter-funnel workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkApprovalOption {
    /// Group ID for bulk selection
    pub group_id: String,
    /// Number of items in group
    pub count: usize,
    /// Preview of what will be approved
    pub preview: String,
    /// Whether all items in group are similar enough for bulk approval
    pub is_homogeneous: bool,
}

impl BulkApprovalOption {
    /// Create a new bulk approval option
    pub fn new(group_id: impl Into<String>, count: usize, preview: impl Into<String>) -> Self {
        Self {
            group_id: group_id.into(),
            count,
            preview: preview.into(),
            is_homogeneous: true,
        }
    }

    /// Mark as heterogeneous (not suitable for bulk approval)
    pub fn heterogeneous(mut self) -> Self {
        self.is_homogeneous = false;
        self
    }
}

/// Workflow metadata for tool responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMetadata {
    /// Current phase
    pub phase: WorkflowPhase,
    /// Whether human approval is needed before next action
    pub needs_approval: bool,
    /// Decisions that need human input
    #[serde(default)]
    pub pending_decisions: Vec<HumanDecision>,
    /// What actions are available next
    #[serde(default)]
    pub next_actions: Vec<NextAction>,
    /// Progress indicator (for multi-step operations)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub progress: Option<Progress>,
    /// Bulk approval options (for filter-funnel workflow)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub bulk_approval_options: Vec<BulkApprovalOption>,
}

impl WorkflowMetadata {
    /// Create workflow metadata for discovery phase
    pub fn discovery() -> Self {
        Self {
            phase: WorkflowPhase::Discovery,
            needs_approval: true,
            pending_decisions: vec![],
            next_actions: vec![
                NextAction::new("apply_scope", "Group files for processing")
                    .auto_suggested(),
            ],
            progress: None,
            bulk_approval_options: vec![],
        }
    }

    /// Create workflow metadata when scope has been applied
    pub fn scope_applied() -> Self {
        Self {
            phase: WorkflowPhase::Discovery,
            needs_approval: true,
            pending_decisions: vec![],
            next_actions: vec![
                NextAction::new("discover_schemas", "Analyze files to infer schema structure")
                    .auto_suggested(),
            ],
            progress: None,
            bulk_approval_options: vec![],
        }
    }

    /// Create workflow metadata for schema inference phase
    pub fn schema_inference() -> Self {
        Self {
            phase: WorkflowPhase::SchemaInference,
            needs_approval: false,
            pending_decisions: vec![],
            next_actions: vec![
                NextAction::new("approve_schemas", "Lock schema as contract")
                    .requires_approval()
                    .auto_suggested(),
                NextAction::new("propose_amendment", "Modify inferred schema"),
            ],
            progress: None,
            bulk_approval_options: vec![],
        }
    }

    /// Create workflow metadata when schema approval is needed
    pub fn schema_approval_needed() -> Self {
        Self {
            phase: WorkflowPhase::SchemaApproval,
            needs_approval: true,
            pending_decisions: vec![],
            next_actions: vec![
                NextAction::new("approve_schemas", "Lock schema as contract")
                    .requires_approval()
                    .auto_suggested(),
                NextAction::new("propose_amendment", "Modify inferred schema")
                    .requires_approval(),
            ],
            progress: None,
            bulk_approval_options: vec![],
        }
    }

    /// Create workflow metadata after schema is approved
    pub fn schema_approved() -> Self {
        Self {
            phase: WorkflowPhase::ParserGeneration,
            needs_approval: false,
            pending_decisions: vec![],
            next_actions: vec![
                NextAction::new("run_backtest", "Test parser against files")
                    .auto_suggested(),
            ],
            progress: None,
            bulk_approval_options: vec![],
        }
    }

    /// Create workflow metadata for backtest phase
    pub fn backtest_complete(success: bool) -> Self {
        if success {
            Self {
                phase: WorkflowPhase::Backtest,
                needs_approval: true,
                pending_decisions: vec![],
                next_actions: vec![
                    NextAction::new("execute_pipeline", "Execute full pipeline with approved schema")
                        .requires_approval()
                        .auto_suggested(),
                ],
                progress: None,
                bulk_approval_options: vec![],
            }
        } else {
            Self {
                phase: WorkflowPhase::ParserRefinement,
                needs_approval: true,
                pending_decisions: vec![],
                next_actions: vec![
                    NextAction::new("fix_parser", "Generate parser fixes based on failures")
                        .auto_suggested(),
                    NextAction::new("run_backtest", "Re-run backtest with updated parser"),
                ],
                progress: None,
                bulk_approval_options: vec![],
            }
        }
    }

    /// Create workflow metadata for parser refinement phase
    pub fn parser_fix_suggested() -> Self {
        Self {
            phase: WorkflowPhase::ParserRefinement,
            needs_approval: true,
            pending_decisions: vec![],
            next_actions: vec![
                NextAction::new("run_backtest", "Re-run backtest with fixed parser")
                    .auto_suggested(),
            ],
            progress: None,
            bulk_approval_options: vec![],
        }
    }

    /// Create workflow metadata for execution phase
    pub fn execution_complete(success: bool) -> Self {
        if success {
            Self {
                phase: WorkflowPhase::Verification,
                needs_approval: false,
                pending_decisions: vec![],
                next_actions: vec![
                    NextAction::new("query_output", "Query processed output data")
                        .auto_suggested(),
                ],
                progress: None,
                bulk_approval_options: vec![],
            }
        } else {
            Self {
                phase: WorkflowPhase::Execution,
                needs_approval: true,
                pending_decisions: vec![],
                next_actions: vec![
                    NextAction::new("execute_pipeline", "Retry pipeline execution"),
                    NextAction::new("fix_parser", "Analyze and fix parser issues"),
                ],
                progress: None,
                bulk_approval_options: vec![],
            }
        }
    }

    /// Create workflow metadata for query output phase
    pub fn query_complete() -> Self {
        Self {
            phase: WorkflowPhase::Verification,
            needs_approval: false,
            pending_decisions: vec![],
            next_actions: vec![
                NextAction::new("query_output", "Run another query"),
            ],
            progress: None,
            bulk_approval_options: vec![],
        }
    }

    /// Add a pending decision
    pub fn with_decision(mut self, decision: HumanDecision) -> Self {
        self.pending_decisions.push(decision);
        self.needs_approval = true;
        self
    }

    /// Add progress information
    pub fn with_progress(mut self, progress: Progress) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Add bulk approval option
    pub fn with_bulk_approval(mut self, option: BulkApprovalOption) -> Self {
        self.bulk_approval_options.push(option);
        self
    }

    /// Set approval requirement
    pub fn needs_approval(mut self, needs: bool) -> Self {
        self.needs_approval = needs;
        self
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_id_creation() {
        let id1 = ScopeId::new();
        let id2 = ScopeId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_scope_id_serialization() {
        let id = ScopeId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: ScopeId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_scope_id_from_str() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id: ScopeId = uuid_str.parse().unwrap();
        assert_eq!(id.to_string(), uuid_str);
    }

    #[test]
    fn test_tool_result_text() {
        let result = ToolResult::text("Hello, world!");
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
    }

    #[test]
    fn test_tool_result_json() {
        #[derive(Serialize)]
        struct Data {
            value: i32,
        }
        let result = ToolResult::json(&Data { value: 42 }).unwrap();
        assert!(!result.is_error);
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("Something went wrong");
        assert!(result.is_error);
    }

    #[test]
    fn test_tool_error_codes() {
        assert_eq!(ToolError::InvalidParams("".into()).error_code(), -32602);
        assert_eq!(ToolError::NotFound("".into()).error_code(), -32001);
        assert_eq!(ToolError::ExecutionFailed("".into()).error_code(), -32002);
        assert_eq!(ToolError::Internal("".into()).error_code(), -32603);
    }

    // =========================================================================
    // Workflow Metadata Tests
    // =========================================================================

    #[test]
    fn test_workflow_phase_display() {
        assert_eq!(WorkflowPhase::Discovery.to_string(), "discovery");
        assert_eq!(WorkflowPhase::SchemaApproval.to_string(), "schema_approval");
        assert_eq!(WorkflowPhase::Backtest.to_string(), "backtest");
    }

    #[test]
    fn test_workflow_phase_serialization() {
        let phase = WorkflowPhase::SchemaInference;
        let json = serde_json::to_string(&phase).unwrap();
        assert_eq!(json, "\"schema_inference\"");

        let parsed: WorkflowPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, phase);
    }

    #[test]
    fn test_decision_option_creation() {
        let option = DecisionOption::new("opt1", "Option 1", "The first option");
        assert_eq!(option.id, "opt1");
        assert_eq!(option.label, "Option 1");
        assert_eq!(option.description, "The first option");
    }

    #[test]
    fn test_human_decision_builder() {
        let decision = HumanDecision::new("Which type for column?")
            .with_options(vec![
                DecisionOption::new("string", "String", "Keep as string"),
                DecisionOption::new("int", "Integer", "Parse as integer"),
            ])
            .blocking(true)
            .with_default("string")
            .with_context("column: amount");

        assert_eq!(decision.question, "Which type for column?");
        assert_eq!(decision.options.len(), 2);
        assert!(decision.blocking);
        assert_eq!(decision.default_option, Some("string".to_string()));
        assert_eq!(decision.context, Some("column: amount".to_string()));
    }

    #[test]
    fn test_next_action_builder() {
        let action = NextAction::new("approve_schemas", "Lock schema as contract")
            .requires_approval()
            .auto_suggested()
            .with_params(serde_json::json!({"scope_id": "test-123"}));

        assert_eq!(action.tool_name, "approve_schemas");
        assert!(action.requires_approval);
        assert!(action.auto_suggested);
        assert!(action.suggested_params.is_some());
    }

    #[test]
    fn test_progress_calculation() {
        let progress = Progress::new(3, 10, "Processing files");
        assert_eq!(progress.current_step, 3);
        assert_eq!(progress.total_steps, 10);
        assert_eq!(progress.percentage, 30);

        // Edge case: zero total steps
        let zero_progress = Progress::new(0, 0, "Empty");
        assert_eq!(zero_progress.percentage, 0);
    }

    #[test]
    fn test_bulk_approval_option() {
        let option = BulkApprovalOption::new("csv_files", 10, "10 CSV files with similar structure");
        assert!(option.is_homogeneous);

        let hetero = option.clone().heterogeneous();
        assert!(!hetero.is_homogeneous);
    }

    #[test]
    fn test_workflow_metadata_discovery() {
        let metadata = WorkflowMetadata::discovery();
        assert_eq!(metadata.phase, WorkflowPhase::Discovery);
        assert!(metadata.needs_approval);
        assert!(!metadata.next_actions.is_empty());
        assert!(metadata.next_actions[0].auto_suggested);
    }

    #[test]
    fn test_workflow_metadata_schema_approval() {
        let metadata = WorkflowMetadata::schema_approval_needed();
        assert_eq!(metadata.phase, WorkflowPhase::SchemaApproval);
        assert!(metadata.needs_approval);

        // Verify both approve and amend are options
        let tool_names: Vec<&str> = metadata.next_actions.iter().map(|a| a.tool_name.as_str()).collect();
        assert!(tool_names.contains(&"approve_schemas"));
        assert!(tool_names.contains(&"propose_amendment"));
    }

    #[test]
    fn test_workflow_metadata_backtest_complete() {
        // Success case
        let success = WorkflowMetadata::backtest_complete(true);
        assert_eq!(success.phase, WorkflowPhase::Backtest);
        assert!(success.next_actions.iter().any(|a| a.tool_name == "execute_pipeline"));

        // Failure case
        let failure = WorkflowMetadata::backtest_complete(false);
        assert_eq!(failure.phase, WorkflowPhase::ParserRefinement);
        assert!(failure.next_actions.iter().any(|a| a.tool_name == "fix_parser"));
    }

    #[test]
    fn test_workflow_metadata_with_decision() {
        let decision = HumanDecision::new("Approve schema?")
            .with_options(vec![
                DecisionOption::new("yes", "Yes", "Approve"),
                DecisionOption::new("no", "No", "Reject"),
            ]);

        let metadata = WorkflowMetadata::schema_inference()
            .with_decision(decision);

        assert!(metadata.needs_approval);
        assert_eq!(metadata.pending_decisions.len(), 1);
    }

    #[test]
    fn test_workflow_metadata_serialization() {
        let metadata = WorkflowMetadata::discovery()
            .with_progress(Progress::new(1, 5, "Scanning"));

        let json = serde_json::to_string(&metadata).unwrap();
        let parsed: WorkflowMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.phase, WorkflowPhase::Discovery);
        assert!(parsed.progress.is_some());
    }
}
