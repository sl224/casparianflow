//! Code generation tools: generate_parser and refine_parser
//!
//! This module provides tools for parser code generation and iterative refinement:
//!
//! - `generate_parser`: Generate Python parser code from discovered schema
//! - `refine_parser`: Takes failed parser code + errors + constraints and produces an improved version
//!
//! Generated parsers follow the Bridge Protocol:
//! - Define `TOPIC` and `SINK` constants for output routing
//! - Implement `parse(file_path: str)` function returning polars DataFrame
//! - Support multi-output via `list[Output]` return type
//!
//! See `crates/casparian_worker/shim/casparian_types.py` for Output contract.

use crate::types::{Tool, ToolError, ToolInputSchema, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

// =============================================================================
// Generate Parser Tool
// =============================================================================

/// Column definition for parser generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    /// Column name
    pub name: String,
    /// Data type (int64, float64, string, boolean, date, timestamp)
    pub data_type: String,
    /// Whether nulls are allowed
    #[serde(default)]
    pub nullable: bool,
    /// Date/time format string (e.g., "%Y-%m-%d")
    pub format: Option<String>,
    /// Rename column to this name in output
    pub rename_to: Option<String>,
}

/// Schema definition for parser generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDef {
    /// Schema name (used for function naming)
    pub name: String,
    /// Column definitions
    pub columns: Vec<ColumnDef>,
    /// Description (becomes docstring)
    pub description: Option<String>,
}

/// Parser generation options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserOptions {
    /// File format: csv, json, tsv, parquet
    #[serde(default = "default_format")]
    pub file_format: String,
    /// CSV delimiter (default: ",")
    pub delimiter: Option<String>,
    /// Number of header rows to skip
    #[serde(default)]
    pub skip_rows: usize,
    /// Values to treat as null
    pub null_values: Option<Vec<String>>,
    /// Date format for parsing (default: "%Y-%m-%d")
    pub date_format: Option<String>,
    /// Timestamp format for parsing
    pub timestamp_format: Option<String>,
    /// Whether to include error handling
    #[serde(default = "default_true")]
    pub include_error_handling: bool,
    /// Whether to include type validation
    #[serde(default = "default_true")]
    pub include_validation: bool,
    /// Output sink type: parquet, sqlite, csv
    #[serde(default = "default_sink")]
    pub sink_type: String,
    /// For sqlite: custom table name
    pub table_name: Option<String>,
    /// Additional imports to include
    pub extra_imports: Option<Vec<String>>,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            file_format: "csv".to_string(),
            delimiter: None,
            skip_rows: 0,
            null_values: None,
            date_format: None,
            timestamp_format: None,
            include_error_handling: true,
            include_validation: true,
            sink_type: "sqlite".to_string(),
            table_name: None,
            extra_imports: None,
        }
    }
}

fn default_sink() -> String {
    "sqlite".to_string()
}

fn default_format() -> String {
    "csv".to_string()
}

fn default_true() -> bool {
    true
}

/// Result of parser generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateParserResult {
    /// Generated Python parser code
    pub parser_code: String,
    /// Suggested parser file name
    pub parser_name: String,
    /// Estimated complexity: simple, medium, complex
    pub complexity: String,
    /// Lines of code generated
    pub lines_of_code: usize,
    /// Columns with special handling
    pub special_columns: Vec<String>,
    /// Any warnings during generation
    pub warnings: Vec<String>,
    /// Suggested test cases
    pub suggested_tests: Vec<String>,
}

/// Generate Python parser code from discovered schema
///
/// This tool creates polars-based Python parser code that:
/// - Reads the specified file format
/// - Converts columns to the correct types
/// - Handles null values appropriately
/// - Validates data against the schema
pub struct GenerateParserTool;

impl GenerateParserTool {
    pub fn new() -> Self {
        Self
    }

    /// Map schema data type to polars type
    #[allow(dead_code)] // Used only in tests
    fn polars_type(data_type: &str) -> &'static str {
        match data_type.to_lowercase().as_str() {
            "int64" | "integer" | "int" => "pl.Int64",
            "int32" => "pl.Int32",
            "float64" | "float" | "double" | "number" => "pl.Float64",
            "float32" => "pl.Float32",
            "boolean" | "bool" => "pl.Boolean",
            "string" | "str" | "text" | "varchar" => "pl.Utf8",
            "date" => "pl.Date",
            "timestamp" | "datetime" => "pl.Datetime",
            "time" => "pl.Time",
            "binary" | "bytes" => "pl.Binary",
            _ => "pl.Utf8", // Default to string
        }
    }

    /// Generate type conversion code for a column
    fn generate_type_conversion(col: &ColumnDef, options: &ParserOptions) -> Option<String> {
        let name = &col.name;
        let target_name = col.rename_to.as_ref().unwrap_or(name);
        let dtype = col.data_type.to_lowercase();

        match dtype.as_str() {
            "int64" | "integer" | "int" => Some(format!(
                "pl.col(\"{}\").cast(pl.Int64).alias(\"{}\")",
                name, target_name
            )),
            "int32" => Some(format!(
                "pl.col(\"{}\").cast(pl.Int32).alias(\"{}\")",
                name, target_name
            )),
            "float64" | "float" | "double" | "number" => Some(format!(
                "pl.col(\"{}\").cast(pl.Float64).alias(\"{}\")",
                name, target_name
            )),
            "float32" => Some(format!(
                "pl.col(\"{}\").cast(pl.Float32).alias(\"{}\")",
                name, target_name
            )),
            "boolean" | "bool" => Some(format!(
                "pl.col(\"{}\").cast(pl.Boolean).alias(\"{}\")",
                name, target_name
            )),
            "date" => {
                let fmt = col
                    .format
                    .as_ref()
                    .or(options.date_format.as_ref())
                    .map(|f| f.as_str())
                    .unwrap_or("%Y-%m-%d");
                Some(format!(
                    "pl.col(\"{}\").str.strptime(pl.Date, \"{}\").alias(\"{}\")",
                    name, fmt, target_name
                ))
            }
            "timestamp" | "datetime" => {
                let fmt = col
                    .format
                    .as_ref()
                    .or(options.timestamp_format.as_ref())
                    .map(|f| f.as_str())
                    .unwrap_or("%Y-%m-%dT%H:%M:%S");
                Some(format!(
                    "pl.col(\"{}\").str.strptime(pl.Datetime, \"{}\").alias(\"{}\")",
                    name, fmt, target_name
                ))
            }
            "string" | "str" | "text" | "varchar" => {
                if col.rename_to.is_some() {
                    Some(format!(
                        "pl.col(\"{}\").alias(\"{}\")",
                        name, target_name
                    ))
                } else {
                    None // No conversion needed
                }
            }
            _ => None,
        }
    }

    /// Generate the parser code following Bridge Protocol
    ///
    /// Generated parsers must:
    /// 1. Define TOPIC and SINK constants for output routing
    /// 2. Implement parse(file_path: str) -> pl.DataFrame
    /// 3. Use polars for data processing
    fn generate_code(&self, schema: &SchemaDef, options: &ParserOptions) -> GenerateParserResult {
        let mut warnings = Vec::new();
        let mut special_columns = Vec::new();
        let mut lines = Vec::new();

        let topic_name = schema.name.to_lowercase().replace(' ', "_").replace('-', "_");
        let parser_name = format!("{}_parser", topic_name);

        // Module docstring
        lines.push(format!(
            "\"\"\"Parser: {}\n\nGenerated by Casparian MCP.\nBridge Protocol compatible - defines parse() function.\n\"\"\"",
            schema.name
        ));
        lines.push(String::new());

        // Imports
        lines.push("import polars as pl".to_string());

        if let Some(ref extra) = options.extra_imports {
            for imp in extra {
                lines.push(format!("import {}", imp));
            }
        }

        lines.push(String::new());

        // Bridge Protocol: TOPIC and SINK constants
        lines.push("# Bridge Protocol: Output routing configuration".to_string());
        lines.push(format!("TOPIC = \"{}\"", topic_name));
        lines.push(format!("SINK = \"{}\"", options.sink_type));
        lines.push(String::new());

        // Function signature - Bridge Protocol requires parse(file_path: str)
        lines.push("def parse(file_path: str) -> pl.DataFrame:".to_string());

        // Docstring
        let desc = schema
            .description
            .as_ref()
            .map(|d| d.as_str())
            .unwrap_or("Parse file and return structured DataFrame.");
        lines.push(format!("    \"\"\""));
        lines.push(format!("    {}", desc));
        lines.push(String::new());
        lines.push("    Args:".to_string());
        lines.push("        file_path: Path to input file".to_string());
        lines.push(String::new());
        lines.push("    Returns:".to_string());
        lines.push("        pl.DataFrame: Parsed and typed data".to_string());
        lines.push(String::new());
        lines.push("    Raises:".to_string());
        lines.push("        ValueError: If required columns have null values".to_string());
        lines.push("        pl.exceptions.ComputeError: If type conversion fails".to_string());
        lines.push("    \"\"\"".to_string());

        let indent = "    ";

        // Read file based on format
        self.add_read_code(&mut lines, options, indent);

        // Type conversions
        self.add_conversion_code(&mut lines, schema, options, indent, &mut special_columns);

        // Validation
        if options.include_validation {
            self.add_validation_code(&mut lines, schema, indent, &mut warnings);
        }

        lines.push(format!("{}return df", indent));
        lines.push(String::new());

        // Main block for standalone testing (not used by bridge)
        lines.push("# Standalone testing (not used when run via Bridge)".to_string());
        lines.push("if __name__ == \"__main__\":".to_string());
        lines.push("    import sys".to_string());
        lines.push("    if len(sys.argv) > 1:".to_string());
        lines.push("        df = parse(sys.argv[1])".to_string());
        lines.push("        print(f\"Parsed {len(df)} rows\")".to_string());
        lines.push("        print(df.head())".to_string());
        lines.push("        print(f\"\\nSchema: {df.schema}\")".to_string());
        lines.push("    else:".to_string());
        lines.push(format!("        print(\"Usage: python {}.py <file_path>\")", parser_name));

        let parser_code = lines.join("\n");
        let loc = parser_code.lines().count();

        // Determine complexity
        let complexity = if special_columns.len() > 3 || schema.columns.len() > 10 {
            "complex"
        } else if special_columns.is_empty() && schema.columns.len() <= 5 {
            "simple"
        } else {
            "medium"
        };

        // Generate suggested tests
        let suggested_tests = vec![
            format!("Test parse() with valid {} file", options.file_format),
            "Test with missing columns".to_string(),
            "Test with null values in required columns".to_string(),
            "Test with invalid type values".to_string(),
            format!("Verify output has TOPIC='{}' and SINK='{}'", topic_name, options.sink_type),
        ];

        GenerateParserResult {
            parser_code,
            parser_name,
            complexity: complexity.to_string(),
            lines_of_code: loc,
            special_columns,
            warnings,
            suggested_tests,
        }
    }

    fn add_read_code(&self, lines: &mut Vec<String>, options: &ParserOptions, indent: &str) {
        match options.file_format.to_lowercase().as_str() {
            "csv" => {
                let mut read_opts = vec!["file_path".to_string()];

                if let Some(ref delim) = options.delimiter {
                    read_opts.push(format!("separator=\"{}\"", delim));
                }

                if options.skip_rows > 0 {
                    read_opts.push(format!("skip_rows={}", options.skip_rows));
                }

                if let Some(ref nulls) = options.null_values {
                    let null_list: Vec<String> = nulls.iter().map(|n| format!("\"{}\"", n)).collect();
                    read_opts.push(format!("null_values=[{}]", null_list.join(", ")));
                }

                lines.push(format!("{}df = pl.read_csv({})", indent, read_opts.join(", ")));
            }
            "tsv" => {
                let mut read_opts = vec!["file_path".to_string(), "separator=\"\\t\"".to_string()];

                if options.skip_rows > 0 {
                    read_opts.push(format!("skip_rows={}", options.skip_rows));
                }

                lines.push(format!("{}df = pl.read_csv({})", indent, read_opts.join(", ")));
            }
            "json" => {
                lines.push(format!("{}df = pl.read_json(file_path)", indent));
            }
            "ndjson" | "jsonl" => {
                lines.push(format!("{}df = pl.read_ndjson(file_path)", indent));
            }
            "parquet" => {
                lines.push(format!("{}df = pl.read_parquet(file_path)", indent));
            }
            _ => {
                // Default to CSV
                lines.push(format!("{}df = pl.read_csv(file_path)", indent));
            }
        }

        lines.push(String::new());
    }

    fn add_conversion_code(
        &self,
        lines: &mut Vec<String>,
        schema: &SchemaDef,
        options: &ParserOptions,
        indent: &str,
        special_columns: &mut Vec<String>,
    ) {
        let conversions: Vec<String> = schema
            .columns
            .iter()
            .filter_map(|col| {
                let conv = Self::generate_type_conversion(col, options);
                if conv.is_some() {
                    let dtype = col.data_type.to_lowercase();
                    if dtype == "date" || dtype == "timestamp" || dtype == "datetime" {
                        special_columns.push(format!("{} ({})", col.name, dtype));
                    }
                }
                conv
            })
            .collect();

        if !conversions.is_empty() {
            lines.push(format!("{}# Type conversions", indent));
            lines.push(format!("{}df = df.with_columns([", indent));

            for (i, conv) in conversions.iter().enumerate() {
                let comma = if i < conversions.len() - 1 { "," } else { "" };
                lines.push(format!("{}    {}{}", indent, conv, comma));
            }

            lines.push(format!("{}])", indent));
            lines.push(String::new());
        }
    }

    fn add_validation_code(
        &self,
        lines: &mut Vec<String>,
        schema: &SchemaDef,
        indent: &str,
        warnings: &mut Vec<String>,
    ) {
        // Check for required columns (non-nullable)
        let required_cols: Vec<&str> = schema
            .columns
            .iter()
            .filter(|c| !c.nullable)
            .map(|c| c.name.as_str())
            .collect();

        if !required_cols.is_empty() {
            lines.push(format!("{}# Validate required columns have no nulls", indent));
            lines.push(format!(
                "{}required_columns = [{}]",
                indent,
                required_cols
                    .iter()
                    .map(|c| format!("\"{}\"", c))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            lines.push(format!("{}for col in required_columns:", indent));
            lines.push(format!(
                "{}    null_count = df.select(pl.col(col).is_null().sum()).item()",
                indent
            ));
            lines.push(format!("{}    if null_count > 0:", indent));
            lines.push(format!(
                "{}        raise ValueError(f\"Column '{{col}}' has {{null_count}} null values but is required\")",
                indent
            ));
            lines.push(String::new());
        } else {
            warnings.push("No non-nullable columns defined - validation will be minimal".to_string());
        }
    }
}

impl Default for GenerateParserTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GenerateParserTool {
    fn name(&self) -> &str {
        "generate_parser"
    }

    fn description(&self) -> &str {
        "Generate Python parser code from a discovered schema. Creates polars-based code that reads files, converts types, and validates data."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "schema": {
                    "type": "object",
                    "description": "Schema definition (from discover_schemas output)",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Schema name (used for function naming)"
                        },
                        "columns": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "data_type": {
                                        "type": "string",
                                        "enum": ["int64", "int32", "float64", "float32", "string", "boolean", "date", "timestamp", "binary"]
                                    },
                                    "nullable": { "type": "boolean", "default": false },
                                    "format": { "type": "string", "description": "Date/time format string" },
                                    "rename_to": { "type": "string", "description": "Rename column in output" }
                                },
                                "required": ["name", "data_type"]
                            },
                            "description": "Column definitions"
                        },
                        "description": {
                            "type": "string",
                            "description": "Schema description (becomes docstring)"
                        }
                    },
                    "required": ["name", "columns"]
                },
                "options": {
                    "type": "object",
                    "description": "Parser generation options",
                    "properties": {
                        "file_format": {
                            "type": "string",
                            "enum": ["csv", "tsv", "json", "ndjson", "parquet"],
                            "default": "csv",
                            "description": "Input file format"
                        },
                        "delimiter": {
                            "type": "string",
                            "description": "CSV delimiter (default: comma)"
                        },
                        "skip_rows": {
                            "type": "integer",
                            "default": 0,
                            "description": "Number of header rows to skip"
                        },
                        "null_values": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Values to treat as null (e.g., ['NA', 'N/A', ''])"
                        },
                        "date_format": {
                            "type": "string",
                            "default": "%Y-%m-%d",
                            "description": "Default date parsing format"
                        },
                        "timestamp_format": {
                            "type": "string",
                            "default": "%Y-%m-%dT%H:%M:%S",
                            "description": "Default timestamp parsing format"
                        },
                        "include_error_handling": {
                            "type": "boolean",
                            "default": true,
                            "description": "Include try/except error handling"
                        },
                        "include_validation": {
                            "type": "boolean",
                            "default": true,
                            "description": "Include null validation for required columns"
                        },
                        "sink_type": {
                            "type": "string",
                            "enum": ["parquet", "sqlite", "csv"],
                            "default": "parquet",
                            "description": "Output sink type (Bridge Protocol SINK constant)"
                        },
                        "table_name": {
                            "type": "string",
                            "description": "For sqlite sink: custom table name"
                        },
                        "extra_imports": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Additional imports to include"
                        }
                    }
                }
            }),
            vec!["schema".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Extract schema
        let schema_value = args
            .get("schema")
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'schema' parameter".into()))?;

        let schema: SchemaDef = serde_json::from_value(schema_value.clone()).map_err(|e| {
            ToolError::InvalidParams(format!("Invalid schema format: {}", e))
        })?;

        if schema.columns.is_empty() {
            return Err(ToolError::InvalidParams(
                "Schema must have at least one column".into(),
            ));
        }

        // Extract options
        let options: ParserOptions = args
            .get("options")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Generate the parser
        let result = self.generate_code(&schema, &options);

        ToolResult::json(&result)
    }
}

// =============================================================================
// Refine Parser Tool
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
// Internal Types for Refine Parser
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
    _fix_type: FixType,
    /// Description of what the fix does
    description: String,
    /// Code to add/modify
    code_change: String,
}

// =============================================================================
// RefineParserTool Implementation
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
                        columns: vec![col.clone()],
                        fix_type: FixType::KeepAsString,
                        confidence: 0.8,
                    });
                } else {
                    patterns.push(ErrorPattern {
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
                        _fix_type: FixType::NullHandling,
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
                        _fix_type: FixType::NullHandling,
                        description: format!("Column '{}' allows nulls - no fill needed", col_name),
                        code_change: format!(
                            r#"    # Column allows nulls - ensure proper null handling
    # No fill_null needed for nullable column '{}'"#,
                            col_name
                        ),
                    }
                } else {
                    Fix {
                        _fix_type: FixType::NullHandling,
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
                    _fix_type: FixType::TypeCast,
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
                _fix_type: FixType::ErrorHandling,
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
                    _fix_type: FixType::DateFormat,
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
                _fix_type: FixType::ValueCleaning,
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
                _fix_type: FixType::KeepAsString,
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

    // -------------------------------------------------------------------------
    // Generate Parser Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_generate_simple_csv_parser_bridge_protocol() {
        let tool = GenerateParserTool::new();

        let args = json!({
            "schema": {
                "name": "sample_data",
                "columns": [
                    { "name": "id", "data_type": "int64", "nullable": false },
                    { "name": "name", "data_type": "string", "nullable": false },
                    { "name": "value", "data_type": "float64", "nullable": true }
                ],
                "description": "Sample data parser"
            }
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let gen_result: GenerateParserResult = serde_json::from_str(text).unwrap();

            // Debug output
            eprintln!("Generated code:\n{}", gen_result.parser_code);

            // Verify Bridge Protocol compliance
            assert!(gen_result.parser_code.contains("TOPIC = \"sample_data\""),
                "Missing TOPIC constant. Code: {}", gen_result.parser_code);
            assert!(gen_result.parser_code.contains("SINK = \"sqlite\""),
                "Missing SINK constant (default sqlite). Code: {}", gen_result.parser_code);
            assert!(gen_result.parser_code.contains("def parse(file_path: str)"),
                "Missing parse() function with correct signature");

            // Verify polars usage
            assert!(gen_result.parser_code.contains("import polars as pl"));
            assert!(gen_result.parser_code.contains("pl.Int64"));
            assert!(gen_result.parser_code.contains("pl.Float64"));

            assert_eq!(gen_result.parser_name, "sample_data_parser");
        }
    }

    #[tokio::test]
    async fn test_generate_parser_with_sqlite_sink() {
        let tool = GenerateParserTool::new();

        let args = json!({
            "schema": {
                "name": "transactions",
                "columns": [
                    { "name": "id", "data_type": "int64" },
                    { "name": "amount", "data_type": "float64" }
                ]
            },
            "options": {
                "sink_type": "sqlite"
            }
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let gen_result: GenerateParserResult = serde_json::from_str(text).unwrap();
            assert!(gen_result.parser_code.contains("SINK = \"sqlite\""),
                "Should use sqlite sink");
            assert!(gen_result.parser_code.contains("TOPIC = \"transactions\""));
        }
    }

    #[tokio::test]
    async fn test_generate_parser_with_dates() {
        let tool = GenerateParserTool::new();

        let args = json!({
            "schema": {
                "name": "sales",
                "columns": [
                    { "name": "date", "data_type": "date", "format": "%Y-%m-%d" },
                    { "name": "amount", "data_type": "float64" },
                    { "name": "created_at", "data_type": "timestamp" }
                ]
            },
            "options": {
                "file_format": "csv"
            }
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let gen_result: GenerateParserResult = serde_json::from_str(text).unwrap();
            assert!(gen_result.parser_code.contains("strptime"));
            assert!(gen_result.parser_code.contains("pl.Date"));
            assert!(gen_result.parser_code.contains("pl.Datetime"));
            assert_eq!(gen_result.special_columns.len(), 2); // date and timestamp

            // Verify Bridge Protocol
            assert!(gen_result.parser_code.contains("TOPIC = \"sales\""));
            assert!(gen_result.parser_code.contains("def parse(file_path: str)"));
        }
    }

    #[tokio::test]
    async fn test_generate_json_parser() {
        let tool = GenerateParserTool::new();

        let args = json!({
            "schema": {
                "name": "events",
                "columns": [
                    { "name": "event_id", "data_type": "string" },
                    { "name": "count", "data_type": "int64" }
                ]
            },
            "options": {
                "file_format": "json"
            }
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let gen_result: GenerateParserResult = serde_json::from_str(text).unwrap();
            assert!(gen_result.parser_code.contains("pl.read_json"));
            assert!(gen_result.parser_code.contains("TOPIC = \"events\""));
        }
    }

    #[tokio::test]
    async fn test_generate_parser_with_null_values() {
        let tool = GenerateParserTool::new();

        let args = json!({
            "schema": {
                "name": "data",
                "columns": [
                    { "name": "value", "data_type": "float64" }
                ]
            },
            "options": {
                "null_values": ["NA", "N/A", ""]
            }
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let gen_result: GenerateParserResult = serde_json::from_str(text).unwrap();
            assert!(gen_result.parser_code.contains("null_values"));
            assert!(gen_result.parser_code.contains("\"NA\""));
        }
    }

    #[tokio::test]
    async fn test_generate_parser_without_validation() {
        let tool = GenerateParserTool::new();

        let args = json!({
            "schema": {
                "name": "simple",
                "columns": [
                    { "name": "id", "data_type": "int64" }
                ]
            },
            "options": {
                "include_validation": false
            }
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let gen_result: GenerateParserResult = serde_json::from_str(text).unwrap();
            // Should still have Bridge Protocol constants
            assert!(gen_result.parser_code.contains("TOPIC = \"simple\""));
            assert!(gen_result.parser_code.contains("SINK = \"sqlite\""));
            assert!(gen_result.parser_code.contains("def parse(file_path: str)"));
        }
    }

    #[tokio::test]
    async fn test_generate_parser_with_column_rename() {
        let tool = GenerateParserTool::new();

        let args = json!({
            "schema": {
                "name": "renamed",
                "columns": [
                    { "name": "old_name", "data_type": "string", "rename_to": "new_name" }
                ]
            }
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let gen_result: GenerateParserResult = serde_json::from_str(text).unwrap();
            assert!(gen_result.parser_code.contains("alias(\"new_name\")"));
        }
    }

    #[tokio::test]
    async fn test_generate_parser_missing_schema() {
        let tool = GenerateParserTool::new();

        let args = json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_generate_parser_empty_columns() {
        let tool = GenerateParserTool::new();

        let args = json!({
            "schema": {
                "name": "empty",
                "columns": []
            }
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_parser_schema() {
        let tool = GenerateParserTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.as_ref().unwrap().contains(&"schema".to_string()));
    }

    #[test]
    fn test_polars_type_mapping() {
        assert_eq!(GenerateParserTool::polars_type("int64"), "pl.Int64");
        assert_eq!(GenerateParserTool::polars_type("float64"), "pl.Float64");
        assert_eq!(GenerateParserTool::polars_type("string"), "pl.Utf8");
        assert_eq!(GenerateParserTool::polars_type("boolean"), "pl.Boolean");
        assert_eq!(GenerateParserTool::polars_type("date"), "pl.Date");
        assert_eq!(GenerateParserTool::polars_type("timestamp"), "pl.Datetime");
        assert_eq!(GenerateParserTool::polars_type("unknown"), "pl.Utf8"); // Default
    }

    #[tokio::test]
    async fn test_bridge_protocol_topic_naming() {
        let tool = GenerateParserTool::new();

        // Test that names with spaces/dashes become valid topic names
        let args = json!({
            "schema": {
                "name": "Sales Data 2024",
                "columns": [
                    { "name": "id", "data_type": "int64" }
                ]
            }
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let gen_result: GenerateParserResult = serde_json::from_str(text).unwrap();
            // Spaces should become underscores, lowercase
            assert!(gen_result.parser_code.contains("TOPIC = \"sales_data_2024\""));
        }
    }

    // -------------------------------------------------------------------------
    // Refine Parser Tests
    // -------------------------------------------------------------------------

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

        assert_eq!(fix._fix_type, FixType::NullHandling);
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
            _fix_type: FixType::NullHandling,
            description: "Fill nulls".to_string(),
            code_change: "    df = df.fill_null(0)".to_string(),
        };

        let result = tool.apply_fix(code, &fix);

        assert!(result.contains("fill_null"));
        assert!(result.contains("pl.read_csv"));
    }
}
