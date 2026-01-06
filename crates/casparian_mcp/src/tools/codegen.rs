//! Code generation tools: refine_parser
//!
//! This tool handles iterative parser refinement:
//!
//! - `refine_parser`: Takes failed parser code + errors + constraints and produces an improved version

use crate::types::{Tool, ToolError, ToolInputSchema, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

// =============================================================================
// Input/Output Types
// =============================================================================

/// Input for refining a parser
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineParserInput {
    /// Current parser code that failed
    pub current_code: String,
    /// Errors from the failed backtest
    pub errors: Vec<ParserError>,
    /// Schema constraints to satisfy
    pub constraints: SchemaConstraints,
    /// Current attempt number (1-based)
    pub attempt: u32,
    /// Maximum attempts before escalating
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
}

fn default_max_attempts() -> u32 {
    3
}

/// Error from a parser run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserError {
    /// File that caused the error
    pub file_path: String,
    /// Line number in the file (if known)
    pub line_number: Option<u32>,
    /// Error type (e.g., "type_mismatch", "null_value", "parse_error")
    pub error_type: String,
    /// Error message
    pub message: String,
    /// The problematic value
    pub value: Option<String>,
    /// Column that caused the error
    pub column: Option<String>,
}

/// Schema constraints that the parser must satisfy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaConstraints {
    /// Expected column types from approved schema
    pub columns: Vec<ColumnConstraintDef>,
    /// Whether nulls are allowed per column
    #[serde(default)]
    pub nullable_columns: Vec<String>,
    /// Required columns (must not have nulls)
    #[serde(default)]
    pub required_columns: Vec<String>,
}

/// Definition of a column constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnConstraintDef {
    /// Column name
    pub name: String,
    /// Expected data type
    pub expected_type: String,
    /// Whether the column allows nulls
    pub nullable: bool,
    /// Optional format (e.g., date format)
    pub format: Option<String>,
}

/// Result of parser refinement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineParserResult {
    /// Refined parser code
    pub refined_code: String,
    /// What was changed
    pub changes_made: Vec<String>,
    /// Current attempt number
    pub attempt: u32,
    /// Status: "retry", "success", "escalate"
    pub status: String,
    /// Message explaining the status
    pub message: String,
    /// If escalating, explain why
    pub escalation_reason: Option<String>,
    /// Suggested manual fixes if escalating
    pub suggested_manual_fixes: Option<Vec<String>>,
}

// =============================================================================
// Internal Types
// =============================================================================

/// Analysis of errors to determine fix strategy
#[derive(Debug, Clone)]
struct ErrorAnalysis {
    /// Errors grouped by type
    by_type: HashMap<String, Vec<ParserError>>,
    /// Errors grouped by column
    by_column: HashMap<String, Vec<ParserError>>,
    /// Primary error type (most common)
    primary_type: Option<String>,
    /// Patterns detected
    patterns: Vec<ErrorPattern>,
}

/// A detected pattern in the errors
#[derive(Debug, Clone)]
struct ErrorPattern {
    /// Type of pattern
    pattern_type: String,
    /// Affected columns
    columns: Vec<String>,
    /// Suggested fix
    fix_type: FixType,
    /// Confidence in this pattern (0.0-1.0)
    confidence: f32,
}

/// Types of fixes that can be applied
#[derive(Debug, Clone, PartialEq)]
enum FixType {
    /// Add null handling (.fill_null() or allow nulls)
    NullHandling,
    /// Change type cast
    TypeCast,
    /// Add error handling (try/except)
    ErrorHandling,
    /// Try different date format
    DateFormat,
    /// Clean/validate values
    ValueCleaning,
    /// Keep as string instead of converting
    KeepAsString,
}

/// A fix to apply to the code
#[derive(Debug, Clone)]
struct Fix {
    /// Type of fix
    fix_type: FixType,
    /// Target column
    column: Option<String>,
    /// Description of what the fix does
    description: String,
    /// Code to add/modify
    code_change: String,
}

// =============================================================================
// RefineParserTool
// =============================================================================

/// Tool for iteratively refining parser code based on errors
///
/// This tool analyzes parser errors and applies targeted fixes.
/// It is bounded to a maximum number of attempts before escalating to human review.
pub struct RefineParserTool;

impl RefineParserTool {
    pub fn new() -> Self {
        Self
    }

    /// Analyze errors to determine fix strategy
    fn analyze_errors(&self, errors: &[ParserError]) -> ErrorAnalysis {
        let mut by_type: HashMap<String, Vec<ParserError>> = HashMap::new();
        let mut by_column: HashMap<String, Vec<ParserError>> = HashMap::new();

        for error in errors {
            by_type
                .entry(error.error_type.clone())
                .or_default()
                .push(error.clone());

            if let Some(ref col) = error.column {
                by_column.entry(col.clone()).or_default().push(error.clone());
            }
        }

        // Find primary error type
        let primary_type = by_type
            .iter()
            .max_by_key(|(_, errs)| errs.len())
            .map(|(t, _)| t.clone());

        // Detect patterns
        let mut patterns = Vec::new();

        // Pattern: All null errors in same column
        for (col, col_errors) in &by_column {
            let null_errors: Vec<_> = col_errors
                .iter()
                .filter(|e| e.error_type == "null_value" || e.error_type == "null_not_allowed")
                .collect();

            if null_errors.len() == col_errors.len() && !null_errors.is_empty() {
                patterns.push(ErrorPattern {
                    pattern_type: "all_nulls_single_column".to_string(),
                    columns: vec![col.clone()],
                    fix_type: FixType::NullHandling,
                    confidence: 0.95,
                });
            }
        }

        // Pattern: Type mismatch suggesting wrong cast
        for (col, col_errors) in &by_column {
            let type_errors: Vec<_> = col_errors
                .iter()
                .filter(|e| e.error_type == "type_mismatch")
                .collect();

            if !type_errors.is_empty() {
                // Check if values suggest keeping as string
                let has_non_numeric = type_errors.iter().any(|e| {
                    e.value
                        .as_ref()
                        .map(|v| v.parse::<f64>().is_err())
                        .unwrap_or(false)
                });

                if has_non_numeric {
                    patterns.push(ErrorPattern {
                        pattern_type: "mixed_types_keep_string".to_string(),
                        columns: vec![col.clone()],
                        fix_type: FixType::KeepAsString,
                        confidence: 0.8,
                    });
                } else {
                    patterns.push(ErrorPattern {
                        pattern_type: "type_cast_issue".to_string(),
                        columns: vec![col.clone()],
                        fix_type: FixType::TypeCast,
                        confidence: 0.7,
                    });
                }
            }
        }

        // Pattern: Parse errors suggesting date format issue
        for (col, col_errors) in &by_column {
            let parse_errors: Vec<_> = col_errors
                .iter()
                .filter(|e| e.error_type == "parse_error" || e.error_type == "format_mismatch")
                .collect();

            if !parse_errors.is_empty() {
                // Check if values look like dates
                let looks_like_date = parse_errors.iter().any(|e| {
                    e.value.as_ref().map(|v| {
                        v.contains('/') || v.contains('-') && v.len() >= 8 && v.len() <= 10
                    }).unwrap_or(false)
                });

                if looks_like_date {
                    patterns.push(ErrorPattern {
                        pattern_type: "date_format_issue".to_string(),
                        columns: vec![col.clone()],
                        fix_type: FixType::DateFormat,
                        confidence: 0.75,
                    });
                }
            }
        }

        // Pattern: Value errors suggesting cleaning needed
        let value_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.error_type == "value_error")
            .collect();

        if !value_errors.is_empty() {
            let affected_cols: Vec<String> = value_errors
                .iter()
                .filter_map(|e| e.column.clone())
                .collect();

            patterns.push(ErrorPattern {
                pattern_type: "value_cleaning_needed".to_string(),
                columns: affected_cols,
                fix_type: FixType::ValueCleaning,
                confidence: 0.65,
            });
        }

        // Sort patterns by confidence
        patterns.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        ErrorAnalysis {
            by_type,
            by_column,
            primary_type,
            patterns,
        }
    }

    /// Generate a fix based on the fix type
    fn generate_fix(&self, fix_type: &FixType, column: Option<&str>, constraints: &SchemaConstraints) -> Fix {
        let col_name = column.unwrap_or("column_name");

        match fix_type {
            FixType::NullHandling => {
                // Check if column should be nullable based on constraints
                let should_be_nullable = constraints.nullable_columns.contains(&col_name.to_string());
                let is_required = constraints.required_columns.contains(&col_name.to_string());

                if is_required {
                    Fix {
                        fix_type: FixType::NullHandling,
                        column: Some(col_name.to_string()),
                        description: format!("Fill nulls in required column '{}' with default value", col_name),
                        code_change: format!(
                            r#"    # Fill nulls in required column
    df = df.with_columns(
        pl.col("{}").fill_null(strategy="zero").alias("{}")
    )"#,
                            col_name, col_name
                        ),
                    }
                } else if should_be_nullable {
                    Fix {
                        fix_type: FixType::NullHandling,
                        column: Some(col_name.to_string()),
                        description: format!("Column '{}' allows nulls - no fill needed", col_name),
                        code_change: format!(
                            r#"    # Column allows nulls - ensure proper null handling
    # No fill_null needed for nullable column '{}'"#,
                            col_name
                        ),
                    }
                } else {
                    Fix {
                        fix_type: FixType::NullHandling,
                        column: Some(col_name.to_string()),
                        description: format!("Add null handling for column '{}'", col_name),
                        code_change: format!(
                            r#"    # Handle nulls in column
    df = df.with_columns(
        pl.col("{}").fill_null(strategy="forward").alias("{}")
    )"#,
                            col_name, col_name
                        ),
                    }
                }
            }
            FixType::TypeCast => {
                // Find expected type from constraints
                let expected_type = constraints
                    .columns
                    .iter()
                    .find(|c| c.name == col_name)
                    .map(|c| c.expected_type.as_str())
                    .unwrap_or("Utf8");

                let polars_type = match expected_type.to_lowercase().as_str() {
                    "int64" | "integer" | "int" => "Int64",
                    "float64" | "float" | "double" => "Float64",
                    "boolean" | "bool" => "Boolean",
                    "date" => "Date",
                    "timestamp" | "datetime" => "Datetime",
                    _ => "Utf8",
                };

                Fix {
                    fix_type: FixType::TypeCast,
                    column: Some(col_name.to_string()),
                    description: format!("Cast column '{}' to {} with error handling", col_name, polars_type),
                    code_change: format!(
                        r#"    # Safe type cast with error handling
    df = df.with_columns(
        pl.col("{}").cast(pl.{}, strict=False).alias("{}")
    )"#,
                        col_name, polars_type, col_name
                    ),
                }
            }
            FixType::ErrorHandling => Fix {
                fix_type: FixType::ErrorHandling,
                column: column.map(String::from),
                description: "Add try/except error handling around parsing".to_string(),
                code_change: r#"    # Add error handling
    try:
        df = pl.read_csv(file_path)
    except Exception as e:
        # Fall back to reading with ignore_errors
        df = pl.read_csv(file_path, ignore_errors=True)
        logging.warning(f"Recovered from parse error: {e}")"#
                    .to_string(),
            },
            FixType::DateFormat => {
                // Check for format hint in constraints
                let format_hint = constraints
                    .columns
                    .iter()
                    .find(|c| c.name == col_name)
                    .and_then(|c| c.format.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or("%Y-%m-%d");

                Fix {
                    fix_type: FixType::DateFormat,
                    column: Some(col_name.to_string()),
                    description: format!("Try multiple date formats for column '{}'", col_name),
                    code_change: format!(
                        r#"    # Try multiple date formats
    def try_parse_date(s):
        if s is None:
            return None
        for fmt in ["{}", "%m/%d/%Y", "%d/%m/%Y", "%Y/%m/%d"]:
            try:
                return datetime.strptime(str(s), fmt)
            except ValueError:
                continue
        return None

    df = df.with_columns(
        pl.col("{}").map_elements(try_parse_date).alias("{}")
    )"#,
                        format_hint, col_name, col_name
                    ),
                }
            }
            FixType::ValueCleaning => Fix {
                fix_type: FixType::ValueCleaning,
                column: column.map(String::from),
                description: format!("Clean and validate values in column '{}'", col_name),
                code_change: format!(
                    r#"    # Clean and validate values
    df = df.with_columns(
        pl.col("{}")
            .str.strip_chars()
            .str.replace_all(r"[^\w\s.-]", "")
            .alias("{}")
    )"#,
                    col_name, col_name
                ),
            },
            FixType::KeepAsString => Fix {
                fix_type: FixType::KeepAsString,
                column: Some(col_name.to_string()),
                description: format!("Keep column '{}' as string due to mixed types", col_name),
                code_change: format!(
                    r#"    # Keep as string due to mixed types
    df = df.with_columns(
        pl.col("{}").cast(pl.Utf8).alias("{}")
    )"#,
                    col_name, col_name
                ),
            },
        }
    }

    /// Apply a fix to the parser code
    fn apply_fix(&self, code: &str, fix: &Fix) -> String {
        // Find a good insertion point - after the read_csv or initial df definition
        let insertion_markers = [
            "df = pl.read_csv",
            "df = polars.read_csv",
            "df = pd.read_csv",
            "df = pandas.read_csv",
            "# Processing",
            "# Transform",
        ];

        for marker in insertion_markers {
            if let Some(pos) = code.find(marker) {
                // Find the end of the line
                if let Some(line_end) = code[pos..].find('\n') {
                    let insertion_point = pos + line_end + 1;
                    let mut result = code.to_string();
                    result.insert_str(insertion_point, &format!("\n{}\n", fix.code_change));
                    return result;
                }
            }
        }

        // If no good insertion point found, append at the end before return
        if let Some(return_pos) = code.rfind("return") {
            let mut result = code.to_string();
            result.insert_str(return_pos, &format!("{}\n\n    ", fix.code_change));
            return result;
        }

        // Last resort: append to end
        format!("{}\n\n{}", code, fix.code_change)
    }

    /// Check if we should escalate to human review
    fn should_escalate(
        &self,
        attempt: u32,
        max_attempts: u32,
        errors: &[ParserError],
        previous_error_types: Option<&[String]>,
    ) -> (bool, Option<String>) {
        // Exceeded max attempts
        if attempt >= max_attempts {
            return (
                true,
                Some(format!(
                    "Exceeded maximum attempts ({}/{})",
                    attempt, max_attempts
                )),
            );
        }

        // Same errors keep appearing (not making progress)
        if let Some(prev_types) = previous_error_types {
            let current_types: Vec<_> = errors.iter().map(|e| &e.error_type).collect();
            let same_errors = prev_types.iter().all(|t| current_types.contains(&t));
            if same_errors && !current_types.is_empty() {
                return (
                    true,
                    Some("Same errors persist after fix attempt - not making progress".to_string()),
                );
            }
        }

        // Errors require human judgment (ambiguous data)
        let ambiguous_errors = errors.iter().filter(|e| {
            e.error_type == "ambiguous_type" ||
            e.error_type == "schema_mismatch" ||
            e.message.to_lowercase().contains("ambiguous")
        }).count();

        if ambiguous_errors > errors.len() / 2 && !errors.is_empty() {
            return (
                true,
                Some("Majority of errors require human judgment to resolve".to_string()),
            );
        }

        (false, None)
    }

    /// Generate suggested manual fixes for escalation
    fn generate_manual_fixes(&self, analysis: &ErrorAnalysis) -> Vec<String> {
        let mut suggestions = Vec::new();

        if let Some(ref primary) = analysis.primary_type {
            suggestions.push(format!(
                "Review the '{}' errors - {} occurrences",
                primary,
                analysis.by_type.get(primary).map(|v| v.len()).unwrap_or(0)
            ));
        }

        for (col, errors) in &analysis.by_column {
            if errors.len() > 1 {
                let error_types: Vec<_> = errors.iter().map(|e| &e.error_type).collect();
                suggestions.push(format!(
                    "Column '{}' has multiple error types: {:?} - consider schema change",
                    col, error_types
                ));
            }
        }

        // Add specific suggestions based on patterns
        for pattern in &analysis.patterns {
            match pattern.fix_type {
                FixType::KeepAsString => {
                    suggestions.push(format!(
                        "Consider changing schema for columns {:?} to string type",
                        pattern.columns
                    ));
                }
                FixType::DateFormat => {
                    suggestions.push(format!(
                        "Review date formats in columns {:?} - may need standardization at source",
                        pattern.columns
                    ));
                }
                _ => {}
            }
        }

        if suggestions.is_empty() {
            suggestions.push("Review the source data for quality issues".to_string());
            suggestions.push("Consider relaxing schema constraints".to_string());
        }

        suggestions
    }
}

impl Default for RefineParserTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for RefineParserTool {
    fn name(&self) -> &str {
        "refine_parser"
    }

    fn description(&self) -> &str {
        "Refine parser code based on errors from failed backtest. Analyzes error patterns and applies targeted fixes. Bounded to max 3 attempts before escalating to human review."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "current_code": {
                    "type": "string",
                    "description": "Current parser Python code that failed"
                },
                "errors": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "file_path": { "type": "string", "description": "File that caused the error" },
                            "line_number": { "type": "integer", "description": "Line number (if known)" },
                            "error_type": {
                                "type": "string",
                                "description": "Error type: type_mismatch, null_value, parse_error, format_mismatch, value_error"
                            },
                            "message": { "type": "string", "description": "Error message" },
                            "value": { "type": "string", "description": "Problematic value" },
                            "column": { "type": "string", "description": "Column that caused the error" }
                        },
                        "required": ["file_path", "error_type", "message"]
                    },
                    "description": "Errors from the failed backtest"
                },
                "constraints": {
                    "type": "object",
                    "properties": {
                        "columns": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "expected_type": { "type": "string" },
                                    "nullable": { "type": "boolean" },
                                    "format": { "type": "string" }
                                },
                                "required": ["name", "expected_type", "nullable"]
                            }
                        },
                        "nullable_columns": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "required_columns": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    },
                    "description": "Schema constraints the parser must satisfy"
                },
                "attempt": {
                    "type": "integer",
                    "description": "Current attempt number (1-based)",
                    "default": 1
                },
                "max_attempts": {
                    "type": "integer",
                    "description": "Maximum attempts before escalating (default: 3)",
                    "default": 3
                },
                "previous_error_types": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Error types from previous attempt (for progress detection)"
                }
            }),
            vec!["current_code".to_string(), "errors".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Extract required parameters
        let current_code = args
            .get("current_code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'current_code' parameter".into()))?;

        let errors_value = args
            .get("errors")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'errors' parameter".into()))?;

        let errors: Vec<ParserError> = serde_json::from_value(Value::Array(errors_value.clone()))
            .map_err(|e| ToolError::InvalidParams(format!("Invalid errors format: {}", e)))?;

        // Extract optional parameters
        let constraints: SchemaConstraints = args
            .get("constraints")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_else(|| SchemaConstraints {
                columns: vec![],
                nullable_columns: vec![],
                required_columns: vec![],
            });

        let attempt = args
            .get("attempt")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(1);

        let max_attempts = args
            .get("max_attempts")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(3);

        let previous_error_types: Option<Vec<String>> = args
            .get("previous_error_types")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        // Check if we should escalate
        let (should_escalate, escalation_reason) =
            self.should_escalate(attempt, max_attempts, &errors, previous_error_types.as_deref());

        if should_escalate {
            let analysis = self.analyze_errors(&errors);
            let manual_fixes = self.generate_manual_fixes(&analysis);

            let result = RefineParserResult {
                refined_code: current_code.to_string(),
                changes_made: vec![],
                attempt,
                status: "escalate".to_string(),
                message: "Escalating to human review".to_string(),
                escalation_reason,
                suggested_manual_fixes: Some(manual_fixes),
            };

            return ToolResult::json(&result);
        }

        // Analyze errors and generate fixes
        let analysis = self.analyze_errors(&errors);
        let mut refined_code = current_code.to_string();
        let mut changes_made = Vec::new();

        // Apply fixes based on detected patterns
        for pattern in &analysis.patterns {
            for col in &pattern.columns {
                let fix = self.generate_fix(&pattern.fix_type, Some(col), &constraints);
                refined_code = self.apply_fix(&refined_code, &fix);
                changes_made.push(fix.description);
            }
        }

        // If no patterns detected but we have errors, try generic fixes
        if changes_made.is_empty() && !errors.is_empty() {
            // Group by error type and apply appropriate fixes
            for (error_type, type_errors) in &analysis.by_type {
                let fix_type = match error_type.as_str() {
                    "null_value" | "null_not_allowed" => FixType::NullHandling,
                    "type_mismatch" => FixType::TypeCast,
                    "parse_error" => FixType::ErrorHandling,
                    "format_mismatch" => FixType::DateFormat,
                    "value_error" => FixType::ValueCleaning,
                    _ => FixType::ErrorHandling,
                };

                // Get first affected column if any
                let column = type_errors.first().and_then(|e| e.column.as_deref());
                let fix = self.generate_fix(&fix_type, column, &constraints);
                refined_code = self.apply_fix(&refined_code, &fix);
                changes_made.push(fix.description);
            }
        }

        // Determine status
        let is_escalating = changes_made.is_empty();
        let status = if is_escalating {
            "escalate".to_string()
        } else {
            "retry".to_string()
        };

        let message = if !is_escalating {
            format!(
                "Applied {} fixes. Please run backtest again (attempt {}/{})",
                changes_made.len(),
                attempt,
                max_attempts
            )
        } else {
            "No fixes could be generated - escalating to human review".to_string()
        };

        let result = RefineParserResult {
            refined_code,
            changes_made,
            attempt,
            status,
            message,
            escalation_reason: if is_escalating {
                Some("Unable to generate automated fixes for these errors".to_string())
            } else {
                None
            },
            suggested_manual_fixes: if is_escalating {
                Some(self.generate_manual_fixes(&analysis))
            } else {
                None
            },
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

    #[tokio::test]
    async fn test_refine_parser_null_errors() {
        let tool = RefineParserTool::new();

        let args = json!({
            "current_code": r#"
import polars as pl

def transform(file_path):
    df = pl.read_csv(file_path)
    return df
"#,
            "errors": [
                {
                    "file_path": "/path/to/file.csv",
                    "error_type": "null_value",
                    "message": "Null value in column 'amount'",
                    "column": "amount"
                }
            ],
            "constraints": {
                "columns": [
                    { "name": "amount", "expected_type": "float64", "nullable": false }
                ],
                "required_columns": ["amount"]
            },
            "attempt": 1
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let refine_result: RefineParserResult = serde_json::from_str(text).unwrap();
            assert_eq!(refine_result.status, "retry");
            assert!(!refine_result.changes_made.is_empty());
            assert!(refine_result.refined_code.contains("fill_null"));
        }
    }

    #[tokio::test]
    async fn test_refine_parser_type_mismatch() {
        let tool = RefineParserTool::new();

        let args = json!({
            "current_code": r#"
import polars as pl

def transform(file_path):
    df = pl.read_csv(file_path)
    df = df.with_columns(pl.col("value").cast(pl.Int64))
    return df
"#,
            "errors": [
                {
                    "file_path": "/path/to/file.csv",
                    "error_type": "type_mismatch",
                    "message": "Cannot cast 'N/A' to Int64",
                    "column": "value",
                    "value": "N/A"
                }
            ],
            "constraints": {
                "columns": [
                    { "name": "value", "expected_type": "int64", "nullable": true }
                ]
            },
            "attempt": 1
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let refine_result: RefineParserResult = serde_json::from_str(text).unwrap();
            assert_eq!(refine_result.status, "retry");
            assert!(
                refine_result.changes_made.iter().any(|c| c.contains("string") || c.contains("cast")),
                "Expected type-related change"
            );
        }
    }

    #[tokio::test]
    async fn test_refine_parser_escalates_after_max_attempts() {
        let tool = RefineParserTool::new();

        let args = json!({
            "current_code": "# parser code",
            "errors": [
                {
                    "file_path": "/path/to/file.csv",
                    "error_type": "unknown_error",
                    "message": "Something went wrong"
                }
            ],
            "attempt": 3,
            "max_attempts": 3
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let refine_result: RefineParserResult = serde_json::from_str(text).unwrap();
            assert_eq!(refine_result.status, "escalate");
            assert!(refine_result.escalation_reason.is_some());
            assert!(refine_result.suggested_manual_fixes.is_some());
        }
    }

    #[tokio::test]
    async fn test_refine_parser_escalates_on_same_errors() {
        let tool = RefineParserTool::new();

        let args = json!({
            "current_code": "# parser code",
            "errors": [
                {
                    "file_path": "/path/to/file.csv",
                    "error_type": "null_value",
                    "message": "Null in column",
                    "column": "amount"
                }
            ],
            "previous_error_types": ["null_value"],
            "attempt": 2,
            "max_attempts": 3
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let refine_result: RefineParserResult = serde_json::from_str(text).unwrap();
            assert_eq!(refine_result.status, "escalate");
            assert!(refine_result.escalation_reason.as_ref().unwrap().contains("progress"));
        }
    }

    #[tokio::test]
    async fn test_refine_parser_date_format() {
        let tool = RefineParserTool::new();

        let args = json!({
            "current_code": r#"
import polars as pl

def transform(file_path):
    df = pl.read_csv(file_path)
    return df
"#,
            "errors": [
                {
                    "file_path": "/path/to/file.csv",
                    "error_type": "parse_error",
                    "message": "Cannot parse date",
                    "column": "created_at",
                    "value": "12/25/2024"
                }
            ],
            "constraints": {
                "columns": [
                    { "name": "created_at", "expected_type": "date", "nullable": false, "format": "%Y-%m-%d" }
                ]
            },
            "attempt": 1
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let refine_result: RefineParserResult = serde_json::from_str(text).unwrap();
            assert_eq!(refine_result.status, "retry");
            assert!(
                refine_result.changes_made.iter().any(|c| c.to_lowercase().contains("date")),
                "Expected date-related change"
            );
        }
    }

    #[test]
    fn test_refine_parser_schema() {
        let tool = RefineParserTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        let required = schema.required.as_ref().unwrap();
        assert!(required.contains(&"current_code".to_string()));
        assert!(required.contains(&"errors".to_string()));
    }

    #[test]
    fn test_error_analysis() {
        let tool = RefineParserTool::new();

        let errors = vec![
            ParserError {
                file_path: "/test.csv".to_string(),
                line_number: Some(1),
                error_type: "null_value".to_string(),
                message: "Null in column".to_string(),
                value: None,
                column: Some("amount".to_string()),
            },
            ParserError {
                file_path: "/test.csv".to_string(),
                line_number: Some(2),
                error_type: "null_value".to_string(),
                message: "Null in column".to_string(),
                value: None,
                column: Some("amount".to_string()),
            },
        ];

        let analysis = tool.analyze_errors(&errors);

        assert_eq!(analysis.primary_type, Some("null_value".to_string()));
        assert!(analysis.by_column.contains_key("amount"));
        assert!(!analysis.patterns.is_empty());
        assert_eq!(analysis.patterns[0].fix_type, FixType::NullHandling);
    }

    #[test]
    fn test_generate_fix_null_handling() {
        let tool = RefineParserTool::new();

        let constraints = SchemaConstraints {
            columns: vec![ColumnConstraintDef {
                name: "amount".to_string(),
                expected_type: "float64".to_string(),
                nullable: false,
                format: None,
            }],
            nullable_columns: vec![],
            required_columns: vec!["amount".to_string()],
        };

        let fix = tool.generate_fix(&FixType::NullHandling, Some("amount"), &constraints);

        assert_eq!(fix.fix_type, FixType::NullHandling);
        assert_eq!(fix.column, Some("amount".to_string()));
        assert!(fix.code_change.contains("fill_null"));
    }

    #[test]
    fn test_apply_fix() {
        let tool = RefineParserTool::new();

        let code = r#"import polars as pl

def transform(file_path):
    df = pl.read_csv(file_path)
    return df"#;

        let fix = Fix {
            fix_type: FixType::NullHandling,
            column: Some("amount".to_string()),
            description: "Fill nulls".to_string(),
            code_change: "    df = df.fill_null(0)".to_string(),
        };

        let result = tool.apply_fix(code, &fix);

        assert!(result.contains("fill_null"));
        assert!(result.contains("pl.read_csv"));
    }
}
