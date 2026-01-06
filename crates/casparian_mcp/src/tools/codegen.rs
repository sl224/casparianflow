//! Code generation tools: generate_parser
//!
//! This module provides tools for generating parser code from schemas:
//!
//! - `generate_parser`: Generate Python parser code from discovered schema
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
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
}
