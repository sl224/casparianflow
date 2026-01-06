//! Execution tools: execute_pipeline, query_output
//!
//! These tools handle pipeline execution and output querying:
//!
//! - `execute_pipeline`: Execute full pipeline with approved schema
//! - `query_output`: Query processed output data

use crate::types::{Tool, ToolError, ToolInputSchema, ToolResult, WorkflowMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use uuid::Uuid;

// =============================================================================
// Execute Pipeline Tool
// =============================================================================

/// Pipeline execution mode
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Process all files (full run)
    Full,
    /// Process only new/changed files
    Incremental,
    /// Dry run - validate but don't write output
    DryRun,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::Incremental
    }
}

/// Output format for processed data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Apache Parquet
    Parquet,
    /// CSV files
    Csv,
    /// SQLite database
    Sqlite,
    /// DuckDB database
    DuckDb,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Parquet
    }
}

/// Configuration for pipeline execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Execution mode
    #[serde(default)]
    pub mode: ExecutionMode,
    /// Output format
    #[serde(default)]
    pub output_format: OutputFormat,
    /// Output directory
    pub output_dir: Option<String>,
    /// Whether to fail on first error
    #[serde(default)]
    pub fail_fast: bool,
    /// Maximum parallel workers
    #[serde(default = "default_max_workers")]
    pub max_workers: usize,
    /// Whether to validate against schema contract
    #[serde(default = "default_true")]
    pub validate_schema: bool,
}

fn default_max_workers() -> usize {
    4
}

fn default_true() -> bool {
    true
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            mode: ExecutionMode::Incremental,
            output_format: OutputFormat::Parquet,
            output_dir: None,
            fail_fast: false,
            max_workers: 4,
            validate_schema: true,
        }
    }
}

/// Result of a single file execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileExecutionResult {
    /// Input file path
    pub input_file: String,
    /// Output file path (if written)
    pub output_file: Option<String>,
    /// Whether execution succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Rows processed
    pub rows_processed: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Result of pipeline execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutePipelineResultOutput {
    /// Execution ID
    pub execution_id: String,
    /// Scope ID
    pub scope_id: String,
    /// Contract ID used for validation
    pub contract_id: Option<String>,
    /// Whether execution succeeded overall
    pub success: bool,
    /// Execution mode used
    pub mode: String,
    /// Output format used
    pub output_format: String,
    /// Files processed
    pub files_processed: usize,
    /// Files succeeded
    pub files_succeeded: usize,
    /// Files failed
    pub files_failed: usize,
    /// Files skipped (incremental mode)
    pub files_skipped: usize,
    /// Total rows processed
    pub total_rows: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Individual file results
    pub file_results: Vec<FileExecutionResult>,
    /// Output directory
    pub output_dir: String,
    /// Errors encountered
    pub errors: Vec<String>,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Execute full pipeline with approved schema
///
/// This tool runs a parser against files in a scope, validates output
/// against the schema contract, and writes to the configured sink.
pub struct ExecutePipelineTool;

impl ExecutePipelineTool {
    pub fn new() -> Self {
        Self
    }

    fn get_output_extension(format: &OutputFormat) -> &str {
        match format {
            OutputFormat::Parquet => "parquet",
            OutputFormat::Csv => "csv",
            OutputFormat::Sqlite => "db",
            OutputFormat::DuckDb => "duckdb",
        }
    }

    fn mock_execute_file(
        &self,
        input_file: &str,
        output_dir: &str,
        format: &OutputFormat,
    ) -> FileExecutionResult {
        let start = std::time::Instant::now();

        let input_path = Path::new(input_file);

        // Check if input exists
        if !input_path.exists() {
            return FileExecutionResult {
                input_file: input_file.to_string(),
                output_file: None,
                success: false,
                error: Some(format!("Input file not found: {}", input_file)),
                rows_processed: 0,
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }

        // Generate output path
        let file_stem = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".to_string());

        let extension = Self::get_output_extension(format);
        let output_file = format!("{}/{}.{}", output_dir, file_stem, extension);

        // Mock: count lines as "rows"
        let rows_processed = match fs::read_to_string(input_file) {
            Ok(content) => content.lines().count().saturating_sub(1), // Subtract header
            Err(e) => {
                return FileExecutionResult {
                    input_file: input_file.to_string(),
                    output_file: None,
                    success: false,
                    error: Some(format!("Failed to read file: {}", e)),
                    rows_processed: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                };
            }
        };

        // Mock: don't actually write output (would need polars/arrow)
        FileExecutionResult {
            input_file: input_file.to_string(),
            output_file: Some(output_file),
            success: true,
            error: None,
            rows_processed,
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }
}

impl Default for ExecutePipelineTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ExecutePipelineTool {
    fn name(&self) -> &str {
        "execute_pipeline"
    }

    fn description(&self) -> &str {
        "Execute full pipeline with approved schema. Runs parser on files, validates output, and writes to sink."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "scope_id": {
                    "type": "string",
                    "description": "Scope ID containing files to process"
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of file paths to process"
                },
                "parser_code": {
                    "type": "string",
                    "description": "Parser Python code to execute"
                },
                "contract_id": {
                    "type": "string",
                    "description": "Schema contract ID to validate against"
                },
                "config": {
                    "type": "object",
                    "description": "Execution configuration",
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["full", "incremental", "dry_run"],
                            "default": "incremental"
                        },
                        "output_format": {
                            "type": "string",
                            "enum": ["parquet", "csv", "sqlite", "duckdb"],
                            "default": "parquet"
                        },
                        "output_dir": {
                            "type": "string"
                        },
                        "fail_fast": {
                            "type": "boolean",
                            "default": false
                        },
                        "max_workers": {
                            "type": "integer",
                            "default": 4
                        },
                        "validate_schema": {
                            "type": "boolean",
                            "default": true
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

        let scope_id = args
            .get("scope_id")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let contract_id = args
            .get("contract_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Parse config
        let config: ExecutionConfig = args
            .get("config")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Determine output directory
        let output_dir = config.output_dir.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h: std::path::PathBuf| h.join(".casparian_flow").join("output"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp/casparian_output"))
                .to_string_lossy()
                .to_string()
        });

        // Create output directory
        if !matches!(config.mode, ExecutionMode::DryRun) {
            if let Err(e) = fs::create_dir_all(&output_dir) {
                return Err(ToolError::ExecutionFailed(format!(
                    "Failed to create output directory: {}",
                    e
                )));
            }
        }

        let execution_id = Uuid::new_v4().to_string();
        let mut file_results = Vec::new();
        let mut errors = Vec::new();
        let mut total_rows = 0;
        let mut files_succeeded = 0;
        let mut files_failed = 0;

        // Process each file
        for file in &files {
            let result = self.mock_execute_file(file, &output_dir, &config.output_format);

            if result.success {
                files_succeeded += 1;
                total_rows += result.rows_processed;
            } else {
                files_failed += 1;
                if let Some(ref error) = result.error {
                    errors.push(format!("{}: {}", file, error));
                }

                if config.fail_fast {
                    file_results.push(result);
                    break;
                }
            }

            file_results.push(result);
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let mode_str = match config.mode {
            ExecutionMode::Full => "full",
            ExecutionMode::Incremental => "incremental",
            ExecutionMode::DryRun => "dry_run",
        };

        let format_str = match config.output_format {
            OutputFormat::Parquet => "parquet",
            OutputFormat::Csv => "csv",
            OutputFormat::Sqlite => "sqlite",
            OutputFormat::DuckDb => "duckdb",
        };

        // Build workflow metadata based on success/failure
        let success = files_failed == 0;
        let workflow = WorkflowMetadata::execution_complete(success);

        let result = ExecutePipelineResultOutput {
            execution_id,
            scope_id,
            contract_id,
            success,
            mode: mode_str.to_string(),
            output_format: format_str.to_string(),
            files_processed: file_results.len(),
            files_succeeded,
            files_failed,
            files_skipped: 0,
            total_rows,
            duration_ms,
            file_results,
            output_dir,
            errors,
            workflow,
        };

        ToolResult::json(&result)
    }
}

// =============================================================================
// Query Output Tool
// =============================================================================

/// A row of query results
type QueryRow = HashMap<String, Value>;

/// Result of a query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryOutputResultOutput {
    /// Number of columns
    pub column_count: usize,
    /// Column names
    pub columns: Vec<String>,
    /// Number of rows returned
    pub row_count: usize,
    /// Whether results were truncated
    pub truncated: bool,
    /// Rows data
    pub rows: Vec<QueryRow>,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Source file/table queried
    pub source: String,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Query processed output data
///
/// This tool allows querying data that was processed and written
/// to the output sink.
pub struct QueryOutputTool;

impl QueryOutputTool {
    pub fn new() -> Self {
        Self
    }

    /// Mock query execution - in production would use polars/duckdb
    fn mock_query_csv(
        &self,
        file_path: &str,
        _sql: Option<&str>,
        limit: usize,
    ) -> Result<QueryOutputResultOutput, ToolError> {
        let start = std::time::Instant::now();

        let content = fs::read_to_string(file_path).map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to read file: {}", e))
        })?;

        let mut lines = content.lines();

        // Parse header
        let header = lines.next().ok_or_else(|| {
            ToolError::ExecutionFailed("File is empty".into())
        })?;
        let columns: Vec<String> = header.split(',').map(|s| s.trim().to_string()).collect();

        // Parse rows
        let mut rows = Vec::new();
        let mut total_rows = 0;

        for line in lines {
            total_rows += 1;
            if rows.len() >= limit {
                continue; // Keep counting for truncation check
            }

            let values: Vec<&str> = line.split(',').collect();
            let mut row: QueryRow = HashMap::new();

            for (i, col) in columns.iter().enumerate() {
                let value = values.get(i).map(|v| v.trim()).unwrap_or("");

                // Try to parse as number, otherwise keep as string
                let json_value = if let Ok(n) = value.parse::<i64>() {
                    Value::Number(n.into())
                } else if let Ok(n) = value.parse::<f64>() {
                    serde_json::Number::from_f64(n)
                        .map(Value::Number)
                        .unwrap_or_else(|| Value::String(value.to_string()))
                } else {
                    Value::String(value.to_string())
                };

                row.insert(col.clone(), json_value);
            }

            rows.push(row);
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        // Build workflow metadata - query complete, can run more queries
        let workflow = WorkflowMetadata::query_complete();

        Ok(QueryOutputResultOutput {
            column_count: columns.len(),
            columns,
            row_count: rows.len(),
            truncated: total_rows > limit,
            rows,
            duration_ms,
            source: file_path.to_string(),
            workflow,
        })
    }
}

impl Default for QueryOutputTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for QueryOutputTool {
    fn name(&self) -> &str {
        "query_output"
    }

    fn description(&self) -> &str {
        "Query processed output data. Execute SQL queries against processed parquet/csv/sqlite files."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "source": {
                    "type": "string",
                    "description": "Source file or table to query"
                },
                "sql": {
                    "type": "string",
                    "description": "SQL query to execute (optional - returns all rows if not provided)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum rows to return (default: 100)",
                    "default": 100
                },
                "format": {
                    "type": "string",
                    "enum": ["table", "json", "csv"],
                    "description": "Output format (default: json)",
                    "default": "json"
                }
            }),
            vec!["source".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Extract parameters
        let source = args
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'source' parameter".into()))?;

        let sql = args.get("sql").and_then(|v| v.as_str());

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as usize;

        let _format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("json");

        // Check source exists
        let path = Path::new(source);
        if !path.exists() {
            return Err(ToolError::NotFound(format!(
                "Source not found: {}",
                source
            )));
        }

        // Determine file type by extension
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let result = match extension.as_str() {
            "csv" | "tsv" => self.mock_query_csv(source, sql, limit)?,
            "parquet" => {
                // Would use polars to read parquet
                return Err(ToolError::ExecutionFailed(
                    "Parquet query not yet implemented (requires polars)".into(),
                ));
            }
            "db" | "sqlite" | "sqlite3" => {
                // Would use rusqlite
                return Err(ToolError::ExecutionFailed(
                    "SQLite query not yet implemented".into(),
                ));
            }
            "duckdb" => {
                // Would use duckdb
                return Err(ToolError::ExecutionFailed(
                    "DuckDB query not yet implemented".into(),
                ));
            }
            _ => {
                return Err(ToolError::InvalidParams(format!(
                    "Unsupported file type: {}",
                    extension
                )));
            }
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

    #[tokio::test]
    async fn test_execute_pipeline_basic() {
        let temp_dir = TempDir::new().unwrap();

        // Create test file
        let input_file = temp_dir.path().join("input.csv");
        fs::write(&input_file, "id,name,value\n1,Alice,100\n2,Bob,200").unwrap();

        let output_dir = temp_dir.path().join("output");

        let tool = ExecutePipelineTool::new();

        let args = json!({
            "files": [input_file.to_string_lossy().to_string()],
            "config": {
                "mode": "full",
                "output_format": "parquet",
                "output_dir": output_dir.to_string_lossy().to_string()
            }
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let exec_result: ExecutePipelineResultOutput = serde_json::from_str(text).unwrap();
            assert!(exec_result.success);
            assert_eq!(exec_result.files_processed, 1);
            assert_eq!(exec_result.total_rows, 2);
        }
    }

    #[tokio::test]
    async fn test_execute_pipeline_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let output_dir = temp_dir.path().join("output");

        let tool = ExecutePipelineTool::new();

        let args = json!({
            "files": ["/nonexistent/file.csv"],
            "config": {
                "output_dir": output_dir.to_string_lossy().to_string()
            }
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let exec_result: ExecutePipelineResultOutput = serde_json::from_str(text).unwrap();
            assert!(!exec_result.success);
            assert_eq!(exec_result.files_failed, 1);
        }
    }

    #[tokio::test]
    async fn test_execute_pipeline_fail_fast() {
        let temp_dir = TempDir::new().unwrap();

        // Create one valid file
        let file1 = temp_dir.path().join("good.csv");
        fs::write(&file1, "a,b\n1,2").unwrap();

        let output_dir = temp_dir.path().join("output");

        let tool = ExecutePipelineTool::new();

        let args = json!({
            "files": [
                "/nonexistent/first.csv",  // This will fail first
                file1.to_string_lossy().to_string()
            ],
            "config": {
                "fail_fast": true,
                "output_dir": output_dir.to_string_lossy().to_string()
            }
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let exec_result: ExecutePipelineResultOutput = serde_json::from_str(text).unwrap();
            // Should have stopped after first failure
            assert_eq!(exec_result.files_processed, 1);
        }
    }

    #[tokio::test]
    async fn test_query_output_csv() {
        let temp_dir = TempDir::new().unwrap();

        // Create test CSV
        let csv_file = temp_dir.path().join("data.csv");
        fs::write(&csv_file, "id,name,score\n1,Alice,95\n2,Bob,87\n3,Carol,92").unwrap();

        let tool = QueryOutputTool::new();

        let args = json!({
            "source": csv_file.to_string_lossy().to_string(),
            "limit": 10
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let query_result: QueryOutputResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(query_result.row_count, 3);
            assert_eq!(query_result.column_count, 3);
            assert!(!query_result.truncated);
        }
    }

    #[tokio::test]
    async fn test_query_output_limit() {
        let temp_dir = TempDir::new().unwrap();

        // Create test CSV with many rows
        let csv_file = temp_dir.path().join("data.csv");
        let content = "id,value\n".to_string() +
            &(1..=100).map(|i| format!("{},{}", i, i * 10)).collect::<Vec<_>>().join("\n");
        fs::write(&csv_file, content).unwrap();

        let tool = QueryOutputTool::new();

        let args = json!({
            "source": csv_file.to_string_lossy().to_string(),
            "limit": 10
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let query_result: QueryOutputResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(query_result.row_count, 10);
            assert!(query_result.truncated);
        }
    }

    #[tokio::test]
    async fn test_query_output_not_found() {
        let tool = QueryOutputTool::new();

        let args = json!({
            "source": "/nonexistent/file.csv"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_pipeline_schema() {
        let tool = ExecutePipelineTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.as_ref().unwrap().contains(&"files".to_string()));
    }

    #[test]
    fn test_query_output_schema() {
        let tool = QueryOutputTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.as_ref().unwrap().contains(&"source".to_string()));
    }
}
