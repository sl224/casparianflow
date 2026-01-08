//! Backtest tools: run_backtest, fix_parser
//!
//! These tools handle multi-file validation and parser fixes:
//!
//! - `run_backtest`: Run parser against multiple files with fail-fast optimization
//! - `fix_parser`: Generate parser fixes based on failure analysis

use crate::types::{Tool, ToolError, ToolInputSchema, ToolResult, WorkflowMetadata};
use async_trait::async_trait;
use casparian_backtest::{
    failfast::{FailFastConfig, FileTestResult, ParserRunner},
    high_failure::{FailureHistoryEntry, FileInfo, HighFailureTable},
    iteration::IterationConfig,
    metrics::{FailureCategory, IterationMetrics},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

// =============================================================================
// Run Backtest Tool
// =============================================================================

/// Configuration for a backtest run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Pass rate threshold for high-failure files
    pub high_failure_threshold: f32,
    /// Whether to stop early if high-failure files fail
    pub early_stop_enabled: bool,
    /// Maximum files to test before checking threshold
    pub check_after_n_files: usize,
    /// Maximum iterations to run
    pub max_iterations: usize,
    /// Target pass rate to achieve
    pub pass_rate_threshold: f32,
    /// Maximum duration in seconds
    pub max_duration_secs: u64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            high_failure_threshold: 0.8,
            early_stop_enabled: true,
            check_after_n_files: 10,
            max_iterations: 10,
            pass_rate_threshold: 0.95,
            max_duration_secs: 300,
        }
    }
}

impl From<BacktestConfig> for IterationConfig {
    fn from(config: BacktestConfig) -> Self {
        IterationConfig {
            max_iterations: config.max_iterations,
            max_duration_secs: config.max_duration_secs,
            pass_rate_threshold: config.pass_rate_threshold,
            improvement_threshold: 0.01,
            plateau_window: 3,
            failfast_config: FailFastConfig {
                high_failure_threshold: config.high_failure_threshold,
                early_stop_enabled: config.early_stop_enabled,
                check_after_n_files: config.check_after_n_files,
                min_high_failure_files: 3,
            },
        }
    }
}

/// Result of a single file test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileResultOutput {
    /// File path
    pub path: String,
    /// Whether the test passed
    pub passed: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Failure category if failed
    pub category: Option<String>,
}

/// Result of running a backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunBacktestResultOutput {
    /// Whether the backtest achieved the target pass rate
    pub success: bool,
    /// Final pass rate achieved
    pub final_pass_rate: f32,
    /// Target pass rate
    pub target_pass_rate: f32,
    /// Number of iterations run
    pub iterations_run: usize,
    /// Why the backtest terminated
    pub termination_reason: String,
    /// Total files tested
    pub files_tested: usize,
    /// Files that passed
    pub files_passed: usize,
    /// Files that failed
    pub files_failed: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Whether early stopped
    pub early_stopped: bool,
    /// Failure breakdown by category
    pub failure_categories: Vec<FailureCategoryCount>,
    /// Top failing files
    pub top_failing_files: Vec<String>,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Failure category with count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureCategoryCount {
    pub category: String,
    pub count: usize,
}

/// Run parser against multiple files with fail-fast optimization
///
/// This tool validates a parser against multiple files, prioritizing
/// files that have historically failed for faster feedback.
pub struct RunBacktestTool;

impl RunBacktestTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RunBacktestTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock parser runner for testing
/// In production, this would be replaced by actual parser execution
struct MockParserRunner;

impl MockParserRunner {
    fn new() -> Self {
        Self
    }
}

impl ParserRunner for MockParserRunner {
    async fn run(&self, file_path: &str) -> FileTestResult {
        use std::path::Path;

        let path = Path::new(file_path);

        if !path.exists() {
            return FileTestResult {
                file_path: file_path.to_string(),
                passed: false,
                error: Some(format!("File not found: {}", file_path)),
                category: Some(FailureCategory::FileNotFound),
            };
        }

        // For mock purposes, treat all existing files as passing
        // Real implementation would actually run the parser
        FileTestResult {
            file_path: file_path.to_string(),
            passed: true,
            error: None,
            category: None,
        }
    }
}

#[async_trait]
impl Tool for RunBacktestTool {
    fn name(&self) -> &str {
        "run_backtest"
    }

    fn description(&self) -> &str {
        "Run parser against multiple files with fail-fast optimization. Tests high-failure files first for rapid feedback."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "scope_id": {
                    "type": "string",
                    "description": "Scope ID containing the files to test"
                },
                "parser_code": {
                    "type": "string",
                    "description": "Parser Python code to validate"
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of file paths to test"
                },
                "config": {
                    "type": "object",
                    "description": "Backtest configuration",
                    "properties": {
                        "high_failure_threshold": {
                            "type": "number",
                            "description": "Pass rate threshold for high-failure files (0.0-1.0)"
                        },
                        "early_stop_enabled": {
                            "type": "boolean",
                            "description": "Whether to stop early on high-failure files"
                        },
                        "max_iterations": {
                            "type": "integer",
                            "description": "Maximum iterations to run"
                        },
                        "pass_rate_threshold": {
                            "type": "number",
                            "description": "Target pass rate to achieve (0.0-1.0)"
                        },
                        "max_duration_secs": {
                            "type": "integer",
                            "description": "Maximum duration in seconds"
                        }
                    }
                }
            }),
            vec!["files".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let start = std::time::Instant::now();

        // Extract parameters
        let files: Vec<String> = args
            .get("files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'files' parameter".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if files.is_empty() {
            return Err(ToolError::InvalidParams("'files' array cannot be empty".into()));
        }

        let scope_id_str = args
            .get("scope_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        let scope_id = scope_id_str
            .as_ref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(Uuid::new_v4);

        let _parser_code = args
            .get("parser_code")
            .and_then(|v| v.as_str())
            .unwrap_or(""); // TODO: Use parser_code to execute actual backtest

        // Parse config
        let config: BacktestConfig = args
            .get("config")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let target_pass_rate = config.pass_rate_threshold;
        let failfast_config = FailFastConfig {
            high_failure_threshold: config.high_failure_threshold,
            early_stop_enabled: config.early_stop_enabled,
            check_after_n_files: config.check_after_n_files,
            min_high_failure_files: 3,
        };

        // Create in-memory high failure table
        let high_failure_table = HighFailureTable::in_memory().await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create high failure table: {}", e)))?;

        // Create file info list
        let file_infos: Vec<FileInfo> = files
            .iter()
            .map(|f| FileInfo::new(f, 0))
            .collect();

        // Run parser on each file
        let runner = MockParserRunner::new();
        let mut metrics = IterationMetrics::new(1, 1);
        let mut early_stopped = false;
        let mut high_failure_pass_rate = 1.0;

        // Get files in backtest order (would use high failure table in production)
        let ordered_files = high_failure_table
            .get_backtest_order(&file_infos, &scope_id).await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to order files: {}", e)))?;

        for (idx, file) in ordered_files.iter().enumerate() {
            let result = runner.run(&file.path).await;

            if result.passed {
                metrics.record_pass();
                let _ = high_failure_table.record_success(&file.path, &scope_id).await;
            } else {
                let category = result.category.unwrap_or(FailureCategory::Unknown);
                let error_msg = result.error.as_deref().unwrap_or("Unknown error");
                metrics.record_fail(&file.path, category, error_msg);

                let entry = FailureHistoryEntry::new(1, 1, category, error_msg);
                let _ = high_failure_table.record_failure(&file.path, &scope_id, entry).await;
            }

            // Check for early stop
            if failfast_config.early_stop_enabled
                && file.is_high_failure
                && idx >= failfast_config.min_high_failure_files
            {
                let hf_tested = ordered_files[..=idx]
                    .iter()
                    .filter(|f| f.is_high_failure)
                    .count();

                if hf_tested > 0 {
                    let mut hf_passed = 0;
                    for f in ordered_files[..=idx].iter().filter(|f| f.is_high_failure) {
                        if runner.run(&f.path).await.passed {
                            hf_passed += 1;
                        }
                    }
                    high_failure_pass_rate = hf_passed as f32 / hf_tested as f32;

                    if high_failure_pass_rate < failfast_config.high_failure_threshold {
                        early_stopped = true;
                        break;
                    }
                }
            }
        }

        metrics.finalize();

        let duration_ms = start.elapsed().as_millis() as u64;

        // Determine termination reason
        let termination_reason = if early_stopped {
            format!(
                "Early stopped: high-failure pass rate ({:.1}%) below threshold ({:.1}%)",
                high_failure_pass_rate * 100.0,
                failfast_config.high_failure_threshold * 100.0
            )
        } else if metrics.pass_rate >= target_pass_rate {
            "Target pass rate achieved".to_string()
        } else {
            "All files tested".to_string()
        };

        // Build failure categories
        let failure_categories: Vec<FailureCategoryCount> = metrics
            .failure_summary
            .by_category
            .iter()
            .map(|(cat, count)| FailureCategoryCount {
                category: cat.to_string(),
                count: *count,
            })
            .collect();

        let top_failing_files: Vec<String> = metrics
            .failure_summary
            .top_failing_files
            .iter()
            .take(5)
            .map(|(path, _)| path.clone())
            .collect();

        // Build workflow metadata based on success/failure
        let success = metrics.pass_rate >= target_pass_rate;
        let workflow = WorkflowMetadata::backtest_complete(success);

        let result = RunBacktestResultOutput {
            success,
            final_pass_rate: metrics.pass_rate,
            target_pass_rate,
            iterations_run: 1,
            termination_reason,
            files_tested: metrics.files_tested,
            files_passed: metrics.files_passed,
            files_failed: metrics.files_failed,
            duration_ms,
            early_stopped,
            failure_categories,
            top_failing_files,
            workflow,
        };

        ToolResult::json(&result)
    }
}

// =============================================================================
// Fix Parser Tool
// =============================================================================

/// A proposed fix for a parser issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserFix {
    /// Type of fix
    pub fix_type: String,
    /// Column affected
    pub column: Option<String>,
    /// Description of the fix
    pub description: String,
    /// Code snippet showing the fix
    pub code_snippet: Option<String>,
    /// Confidence level (0.0-1.0)
    pub confidence: f32,
}

/// Result of fix parser analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixParserResultOutput {
    /// Number of issues analyzed
    pub issues_analyzed: usize,
    /// Proposed fixes
    pub fixes: Vec<ParserFix>,
    /// General recommendations
    pub recommendations: Vec<String>,
    /// Most common failure category
    pub primary_issue: Option<String>,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Generate parser fixes based on failure analysis
///
/// This tool analyzes backtest failures and generates suggested fixes
/// for the parser code.
pub struct FixParserTool;

impl FixParserTool {
    pub fn new() -> Self {
        Self
    }

    fn suggest_fix_for_category(&self, category: &str, column: Option<&str>) -> ParserFix {
        match category {
            "type_mismatch" | "Type Mismatch" => ParserFix {
                fix_type: "type_coercion".to_string(),
                column: column.map(String::from),
                description: "Add type coercion or handle mixed types".to_string(),
                code_snippet: Some(format!(
                    r#"# Handle mixed types in column
df = df.with_columns(
    pl.col("{}").cast(pl.Utf8).alias("{}")
)"#,
                    column.unwrap_or("column_name"),
                    column.unwrap_or("column_name")
                )),
                confidence: 0.8,
            },
            "null_not_allowed" | "Null Not Allowed" => ParserFix {
                fix_type: "null_handling".to_string(),
                column: column.map(String::from),
                description: "Add null value handling or change schema to nullable".to_string(),
                code_snippet: Some(format!(
                    r#"# Fill nulls with default value
df = df.with_columns(
    pl.col("{}").fill_null(default_value).alias("{}")
)"#,
                    column.unwrap_or("column_name"),
                    column.unwrap_or("column_name")
                )),
                confidence: 0.9,
            },
            "format_mismatch" | "Format Mismatch" => ParserFix {
                fix_type: "format_parsing".to_string(),
                column: column.map(String::from),
                description: "Add flexible format parsing".to_string(),
                code_snippet: Some(format!(
                    r#"# Try multiple date formats
def parse_date(value):
    for fmt in ["%Y-%m-%d", "%m/%d/%Y", "%d/%m/%Y"]:
        try:
            return datetime.strptime(value, fmt)
        except ValueError:
            continue
    return None"#
                )),
                confidence: 0.7,
            },
            "parse_error" | "Parse Error" => ParserFix {
                fix_type: "error_handling".to_string(),
                column: None,
                description: "Add try/except handling for parse errors".to_string(),
                code_snippet: Some(format!(
                    r#"# Add error handling
try:
    df = pl.read_csv(file_path)
except Exception as e:
    # Handle parse error
    logging.error(f"Parse error: {{e}}")
    df = pl.read_csv(file_path, ignore_errors=True)"#
                )),
                confidence: 0.6,
            },
            _ => ParserFix {
                fix_type: "general".to_string(),
                column: column.map(String::from),
                description: format!("Review handling for {} errors", category),
                code_snippet: None,
                confidence: 0.5,
            },
        }
    }
}

impl Default for FixParserTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FixParserTool {
    fn name(&self) -> &str {
        "fix_parser"
    }

    fn description(&self) -> &str {
        "Generate parser fixes based on failure analysis. Analyzes backtest failures and suggests code changes."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "parser_code": {
                    "type": "string",
                    "description": "Current parser Python code"
                },
                "failures": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "file_path": { "type": "string" },
                            "error": { "type": "string" },
                            "category": { "type": "string" },
                            "column": { "type": "string" },
                            "sample_values": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        }
                    },
                    "description": "List of failures from backtest"
                },
                "failure_summary": {
                    "type": "object",
                    "description": "Aggregated failure summary from backtest",
                    "properties": {
                        "total_failures": { "type": "integer" },
                        "by_category": {
                            "type": "object",
                            "additionalProperties": { "type": "integer" }
                        },
                        "top_failing_files": {
                            "type": "array",
                            "items": {
                                "type": "array",
                                "items": [
                                    { "type": "string" },
                                    { "type": "integer" }
                                ]
                            }
                        }
                    }
                }
            }),
            vec![],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Extract failures
        let failures = args
            .get("failures")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Extract failure summary
        let by_category: std::collections::HashMap<String, usize> = args
            .get("failure_summary")
            .and_then(|v| v.get("by_category"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let total_failures = args
            .get("failure_summary")
            .and_then(|v| v.get("total_failures"))
            .and_then(|v| v.as_u64())
            .unwrap_or(failures.len() as u64) as usize;

        // Generate fixes based on failures
        let mut fixes = Vec::new();
        let mut seen_categories: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Process individual failures
        for failure in &failures {
            let category = failure
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let column = failure.get("column").and_then(|v| v.as_str());

            // Only add one fix per category
            if !seen_categories.contains(category) {
                seen_categories.insert(category.to_string());
                let fix = self.suggest_fix_for_category(category, column);
                fixes.push(fix);
            }
        }

        // Add fixes from summary if not covered
        for (category, _count) in &by_category {
            if !seen_categories.contains(category) {
                seen_categories.insert(category.clone());
                let fix = self.suggest_fix_for_category(category, None);
                fixes.push(fix);
            }
        }

        // Sort fixes by confidence
        fixes.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Generate recommendations
        let mut recommendations = Vec::new();

        if total_failures > 10 {
            recommendations.push(
                "Consider running with early_stop_enabled=true to fail fast".to_string(),
            );
        }

        if fixes.iter().any(|f| f.fix_type == "type_coercion") {
            recommendations.push(
                "Review column types - consider using more flexible types in schema".to_string(),
            );
        }

        if fixes.iter().any(|f| f.fix_type == "null_handling") {
            recommendations.push(
                "Some columns have unexpected nulls - update schema to be nullable or add fill_null".to_string(),
            );
        }

        // Find primary issue
        let primary_issue = by_category
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(cat, _)| cat.clone());

        // Build workflow metadata - fixes suggested, ready for re-test
        let workflow = WorkflowMetadata::parser_fix_suggested();

        let result = FixParserResultOutput {
            issues_analyzed: total_failures,
            fixes,
            recommendations,
            primary_issue,
            workflow,
        };

        ToolResult::json(&result)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[tokio::test]
    async fn test_run_backtest_basic() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let file1 = temp_dir.path().join("test1.csv");
        let file2 = temp_dir.path().join("test2.csv");
        fs::write(&file1, "a,b\n1,2").unwrap();
        fs::write(&file2, "a,b\n3,4").unwrap();

        let tool = RunBacktestTool::new();

        let args = json!({
            "files": [
                file1.to_string_lossy().to_string(),
                file2.to_string_lossy().to_string()
            ],
            "parser_code": "# test parser",
            "config": {
                "early_stop_enabled": false
            }
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let backtest_result: RunBacktestResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(backtest_result.files_tested, 2);
            assert!(backtest_result.success);
        }
    }

    #[tokio::test]
    async fn test_run_backtest_missing_files() {
        let tool = RunBacktestTool::new();

        let args = json!({
            "files": ["/nonexistent/file1.csv", "/nonexistent/file2.csv"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let backtest_result: RunBacktestResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(backtest_result.files_failed, 2);
            assert!(!backtest_result.success);
        }
    }

    #[tokio::test]
    async fn test_fix_parser_with_failures() {
        let tool = FixParserTool::new();

        let args = json!({
            "parser_code": "# test parser",
            "failures": [
                {
                    "file_path": "/path/to/file.csv",
                    "error": "Type mismatch in column 'amount'",
                    "category": "type_mismatch",
                    "column": "amount"
                },
                {
                    "file_path": "/path/to/file2.csv",
                    "error": "Null value in non-nullable column 'id'",
                    "category": "null_not_allowed",
                    "column": "id"
                }
            ]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let fix_result: FixParserResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(fix_result.fixes.len(), 2);
            assert!(fix_result.fixes.iter().any(|f| f.fix_type == "type_coercion"));
            assert!(fix_result.fixes.iter().any(|f| f.fix_type == "null_handling"));
        }
    }

    #[tokio::test]
    async fn test_fix_parser_with_summary() {
        let tool = FixParserTool::new();

        let args = json!({
            "parser_code": "# test parser",
            "failures": [],
            "failure_summary": {
                "total_failures": 15,
                "by_category": {
                    "type_mismatch": 10,
                    "parse_error": 5
                }
            }
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let fix_result: FixParserResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(fix_result.issues_analyzed, 15);
            assert_eq!(fix_result.fixes.len(), 2);
            assert_eq!(fix_result.primary_issue, Some("type_mismatch".to_string()));
        }
    }

    #[tokio::test]
    async fn test_fix_parser_empty() {
        let tool = FixParserTool::new();

        let args = json!({});

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let fix_result: FixParserResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(fix_result.fixes.len(), 0);
        }
    }

    #[test]
    fn test_run_backtest_schema() {
        let tool = RunBacktestTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.as_ref().unwrap().contains(&"files".to_string()));
    }

    #[test]
    fn test_fix_parser_schema() {
        let tool = FixParserTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        // No required fields for fix_parser
    }
}
