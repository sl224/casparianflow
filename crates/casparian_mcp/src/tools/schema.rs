//! Schema tools: discover_schemas, approve_schemas, propose_amendment
//!
//! These tools handle schema discovery, approval, and amendment:
//!
//! - `discover_schemas`: Analyze files to discover their schema structure
//! - `approve_schemas`: Approve discovered schemas and create contracts
//! - `propose_amendment`: Propose changes to existing schema contracts

use crate::types::{
    BulkApprovalOption, DecisionOption, HumanDecision, Tool, ToolError, ToolInputSchema, ToolResult,
    WorkflowMetadata,
};
use async_trait::async_trait;
use casparian_schema::{
    approval::{ApprovedColumn, ApprovedSchemaVariant, SchemaApprovalRequest},
    DataType, SchemaStorage,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use uuid::Uuid;

// =============================================================================
// Discover Schemas Tool
// =============================================================================

/// A discovered column with inferred type information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredColumn {
    /// Column name
    pub name: String,
    /// Inferred data type
    pub data_type: String,
    /// Sample values from the column
    pub sample_values: Vec<String>,
    /// Percentage of null/empty values
    pub null_percentage: f32,
    /// Whether the type inference is ambiguous
    pub is_ambiguous: bool,
    /// Possible alternative types if ambiguous
    pub alternative_types: Vec<String>,
}

/// A discovered schema from a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSchema {
    /// Unique identifier for this discovery
    pub discovery_id: String,
    /// Source file path
    pub source_file: String,
    /// Schema name (derived from file name)
    pub name: String,
    /// Discovered columns
    pub columns: Vec<DiscoveredColumn>,
    /// Number of rows analyzed
    pub rows_analyzed: usize,
    /// File format detected
    pub format: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Any warnings during discovery
    pub warnings: Vec<String>,
}

/// Result of schema discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverSchemasResult {
    /// Number of files analyzed
    pub files_analyzed: usize,
    /// Discovered schemas
    pub schemas: Vec<DiscoveredSchema>,
    /// Files that could not be analyzed
    pub failed_files: Vec<String>,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Analyze files to discover their schema structure
///
/// This tool examines files to infer their schema, including column names,
/// data types, and nullability.
pub struct DiscoverSchemasTool;

impl DiscoverSchemasTool {
    pub fn new() -> Self {
        Self
    }

    /// Infer data type from a sample of values
    fn infer_type(&self, values: &[&str]) -> (String, bool, Vec<String>) {
        if values.is_empty() {
            return ("string".to_string(), false, vec![]);
        }

        let mut type_counts: HashMap<&str, usize> = HashMap::new();

        for value in values {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }

            // Check types in order of specificity
            if value.parse::<i64>().is_ok() {
                *type_counts.entry("int64").or_insert(0) += 1;
            } else if value.parse::<f64>().is_ok() {
                *type_counts.entry("float64").or_insert(0) += 1;
            } else if Self::is_boolean(value) {
                *type_counts.entry("boolean").or_insert(0) += 1;
            } else if Self::is_date(value) {
                *type_counts.entry("date").or_insert(0) += 1;
            } else if Self::is_timestamp(value) {
                *type_counts.entry("timestamp").or_insert(0) += 1;
            } else {
                *type_counts.entry("string").or_insert(0) += 1;
            }
        }

        if type_counts.is_empty() {
            return ("string".to_string(), false, vec![]);
        }

        // Find most common type
        let mut sorted: Vec<_> = type_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        let best_type = sorted[0].0.to_string();
        let is_ambiguous = sorted.len() > 1 && sorted[1].1 > &(values.len() / 10);
        let alternatives: Vec<String> = sorted
            .iter()
            .skip(1)
            .filter(|(_, count)| **count > values.len() / 20)
            .map(|(t, _)| t.to_string())
            .collect();

        (best_type, is_ambiguous, alternatives)
    }

    fn is_boolean(value: &str) -> bool {
        matches!(
            value.to_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "t" | "f" | "y" | "n" | "1" | "0"
        )
    }

    fn is_date(value: &str) -> bool {
        // Simple date patterns
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
            || chrono::NaiveDate::parse_from_str(value, "%m/%d/%Y").is_ok()
            || chrono::NaiveDate::parse_from_str(value, "%d/%m/%Y").is_ok()
    }

    fn is_timestamp(value: &str) -> bool {
        chrono::DateTime::parse_from_rfc3339(value).is_ok()
            || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok()
    }

    /// Analyze a CSV file
    fn analyze_csv(&self, path: &Path, max_rows: usize) -> Result<DiscoveredSchema, ToolError> {
        let file = File::open(path).map_err(|e| {
            ToolError::ExecutionFailed(format!("Failed to open file {}: {}", path.display(), e))
        })?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Read header
        let header = lines
            .next()
            .ok_or_else(|| ToolError::ExecutionFailed("File is empty".into()))?
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read header: {}", e)))?;

        let column_names: Vec<String> = header.split(',').map(|s| s.trim().to_string()).collect();
        let num_columns = column_names.len();

        // Collect values for each column
        let mut column_values: Vec<Vec<String>> = vec![Vec::new(); num_columns];
        let mut null_counts: Vec<usize> = vec![0; num_columns];
        let mut rows_analyzed = 0;

        for line_result in lines.take(max_rows) {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => continue,
            };

            let values: Vec<&str> = line.split(',').collect();
            rows_analyzed += 1;

            for (i, value) in values.iter().enumerate() {
                if i >= num_columns {
                    break;
                }
                let value = value.trim();
                if value.is_empty() {
                    null_counts[i] += 1;
                } else {
                    column_values[i].push(value.to_string());
                }
            }
        }

        // Build columns
        let mut columns = Vec::new();
        let mut warnings = Vec::new();

        for (i, name) in column_names.iter().enumerate() {
            let values_refs: Vec<&str> = column_values[i].iter().map(|s| s.as_str()).collect();
            let (data_type, is_ambiguous, alternatives) = self.infer_type(&values_refs);

            let null_percentage = if rows_analyzed > 0 {
                (null_counts[i] as f32 / rows_analyzed as f32) * 100.0
            } else {
                0.0
            };

            if is_ambiguous {
                warnings.push(format!(
                    "Column '{}' has ambiguous type: {} (could also be: {})",
                    name,
                    data_type,
                    alternatives.join(", ")
                ));
            }

            if null_percentage > 50.0 {
                warnings.push(format!(
                    "Column '{}' has {:.1}% null values",
                    name, null_percentage
                ));
            }

            columns.push(DiscoveredColumn {
                name: name.clone(),
                data_type,
                sample_values: column_values[i].iter().take(5).cloned().collect(),
                null_percentage,
                is_ambiguous,
                alternative_types: alternatives,
            });
        }

        // Calculate confidence
        let ambiguous_count = columns.iter().filter(|c| c.is_ambiguous).count();
        let confidence = if columns.is_empty() {
            0.0
        } else {
            1.0 - (ambiguous_count as f32 / columns.len() as f32) * 0.5
        };

        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(DiscoveredSchema {
            discovery_id: Uuid::new_v4().to_string(),
            source_file: path.to_string_lossy().to_string(),
            name,
            columns,
            rows_analyzed,
            format: "csv".to_string(),
            confidence,
            warnings,
        })
    }
}

impl Default for DiscoverSchemasTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for DiscoverSchemasTool {
    fn name(&self) -> &str {
        "discover_schemas"
    }

    fn description(&self) -> &str {
        "Analyze files to discover their schema structure. Infers column names, data types, and nullability from file contents."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of file paths to analyze"
                },
                "scope_id": {
                    "type": "string",
                    "description": "Optional scope ID to associate discoveries with"
                },
                "max_rows": {
                    "type": "integer",
                    "description": "Maximum rows to analyze per file (default: 1000)",
                    "default": 1000
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

        let max_rows = args
            .get("max_rows")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000) as usize;

        let mut schemas = Vec::new();
        let mut failed_files = Vec::new();

        for file in &files {
            let path = Path::new(file);

            if !path.exists() {
                failed_files.push(format!("{}: File not found", file));
                continue;
            }

            // Determine file type by extension
            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            match extension.as_str() {
                "csv" | "tsv" | "txt" => match self.analyze_csv(path, max_rows) {
                    Ok(schema) => schemas.push(schema),
                    Err(e) => failed_files.push(format!("{}: {}", file, e)),
                },
                _ => {
                    failed_files.push(format!("{}: Unsupported file type '{}'", file, extension));
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        // Build workflow metadata with decisions for ambiguous columns
        let mut workflow = WorkflowMetadata::schema_approval_needed();

        // Add decisions for ambiguous columns
        for schema in &schemas {
            for col in &schema.columns {
                if col.is_ambiguous {
                    let mut options = vec![DecisionOption::new(
                        &col.data_type,
                        &col.data_type,
                        format!("Use {} (inferred)", col.data_type),
                    )];

                    for alt in &col.alternative_types {
                        options.push(DecisionOption::new(
                            alt,
                            alt,
                            format!("Use {} instead", alt),
                        ));
                    }

                    let decision = HumanDecision::new(format!(
                        "Column '{}' type is ambiguous. Choose: {} or {}?",
                        col.name,
                        col.data_type,
                        col.alternative_types.join("/")
                    ))
                    .with_options(options)
                    .with_default(&col.data_type)
                    .with_context(format!("schema: {}, column: {}", schema.name, col.name));

                    workflow = workflow.with_decision(decision);
                }
            }

            // Add bulk approval option if schema has multiple similar columns
            if schema.columns.len() > 3 {
                let type_counts: std::collections::HashMap<&str, usize> = schema
                    .columns
                    .iter()
                    .fold(std::collections::HashMap::new(), |mut acc, c| {
                        *acc.entry(c.data_type.as_str()).or_insert(0) += 1;
                        acc
                    });

                let most_common = type_counts.iter().max_by_key(|(_, count)| *count);
                if let Some((dtype, count)) = most_common {
                    if *count > 2 {
                        workflow = workflow.with_bulk_approval(BulkApprovalOption::new(
                            format!("{}_{}", schema.name, dtype),
                            *count,
                            format!("{} columns of type {} in schema '{}'", count, dtype, schema.name),
                        ));
                    }
                }
            }
        }

        let result = DiscoverSchemasResult {
            files_analyzed: schemas.len(),
            schemas,
            failed_files,
            duration_ms,
            workflow,
        };

        ToolResult::json(&result)
    }
}

// =============================================================================
// Approve Schemas Tool
// =============================================================================

/// Input for approving a schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveSchemaInput {
    /// Discovery ID to approve
    pub discovery_id: String,
    /// Schema name (can be modified from discovery)
    pub name: String,
    /// Output table name
    pub output_table_name: String,
    /// Columns to approve (can be modified)
    pub columns: Vec<ApproveColumnInput>,
    /// Description
    pub description: Option<String>,
}

/// Column approval input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveColumnInput {
    /// Column name
    pub name: String,
    /// Data type to use
    pub data_type: String,
    /// Whether nulls are allowed
    pub nullable: bool,
    /// Optional format string
    pub format: Option<String>,
    /// Optional rename
    pub rename_to: Option<String>,
}

/// Result of schema approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveSchemaResultOutput {
    /// The created contract
    pub contract_id: String,
    /// Scope ID
    pub scope_id: String,
    /// Version number
    pub version: u32,
    /// Number of schemas approved
    pub schemas_approved: usize,
    /// Any warnings generated
    pub warnings: Vec<String>,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Approve discovered schemas and create contracts
///
/// This tool converts discovered schemas into locked contracts that
/// define the expected structure of data.
pub struct ApproveSchemasTool;

impl ApproveSchemasTool {
    pub fn new() -> Self {
        Self
    }

    fn string_to_data_type(s: &str) -> DataType {
        match s.to_lowercase().as_str() {
            "int64" | "integer" | "int" => DataType::Int64,
            "float64" | "float" | "double" | "number" => DataType::Float64,
            "boolean" | "bool" => DataType::Boolean,
            "date" => DataType::Date,
            "timestamp" | "datetime" => DataType::Timestamp,
            "binary" | "bytes" => DataType::Binary,
            _ => DataType::String,
        }
    }
}

impl Default for ApproveSchemasTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ApproveSchemasTool {
    fn name(&self) -> &str {
        "approve_schemas"
    }

    fn description(&self) -> &str {
        "Approve discovered schemas and create contracts. Once approved, schemas become locked contracts that the parser must conform to."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "scope_id": {
                    "type": "string",
                    "description": "Scope ID to approve schemas for"
                },
                "schemas": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "discovery_id": { "type": "string" },
                            "name": { "type": "string" },
                            "output_table_name": { "type": "string" },
                            "columns": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "name": { "type": "string" },
                                        "data_type": { "type": "string" },
                                        "nullable": { "type": "boolean" },
                                        "format": { "type": "string" },
                                        "rename_to": { "type": "string" }
                                    }
                                }
                            },
                            "description": { "type": "string" }
                        }
                    },
                    "description": "Schemas to approve"
                },
                "approved_by": {
                    "type": "string",
                    "description": "User who is approving (default: 'claude-code')"
                }
            }),
            vec!["scope_id".to_string(), "schemas".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Extract parameters
        let scope_id = args
            .get("scope_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'scope_id' parameter".into()))?;

        let schemas_value = args
            .get("schemas")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'schemas' parameter".into()))?;

        let approved_by = args
            .get("approved_by")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-code");

        // Parse schema inputs
        let schema_inputs: Vec<ApproveSchemaInput> = serde_json::from_value(Value::Array(schemas_value.clone()))
            .map_err(|e| ToolError::InvalidParams(format!("Invalid schema format: {}", e)))?;

        if schema_inputs.is_empty() {
            return Err(ToolError::InvalidParams("'schemas' array cannot be empty".into()));
        }

        // Create storage (in-memory for now - would connect to actual DB in production)
        let storage = SchemaStorage::in_memory()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to create storage: {}", e)))?;

        // Build approved schema variants
        let mut approved_variants = Vec::new();
        let mut warnings = Vec::new();

        for input in &schema_inputs {
            let mut columns = Vec::new();

            for col in &input.columns {
                let data_type = Self::string_to_data_type(&col.data_type);
                let mut approved_col = if col.nullable {
                    ApprovedColumn::optional(&col.name, data_type)
                } else {
                    ApprovedColumn::required(&col.name, data_type)
                };

                if let Some(ref fmt) = col.format {
                    approved_col = approved_col.with_format(fmt);
                }
                if let Some(ref rename) = col.rename_to {
                    approved_col = approved_col.rename_to(rename);
                    warnings.push(format!("Column '{}' will be renamed to '{}'", col.name, rename));
                }

                columns.push(approved_col);
            }

            let mut variant = ApprovedSchemaVariant::new(&input.name, &input.output_table_name)
                .with_columns(columns);

            if let Some(ref desc) = input.description {
                variant = variant.with_description(desc);
            }

            approved_variants.push(variant);
        }

        // Create approval request
        let discovery_id = Uuid::new_v4();
        let request = SchemaApprovalRequest::new(discovery_id, approved_by)
            .with_schemas(approved_variants);

        // Approve and create contract
        let approval_result = casparian_schema::approval::approve_schema(&storage, request)
            .map_err(|e| ToolError::ExecutionFailed(format!("Approval failed: {}", e)))?;

        // Collect all warnings
        for w in &approval_result.warnings {
            warnings.push(w.message.clone());
        }

        // Build workflow metadata - schema is now approved, move to backtest phase
        let workflow = WorkflowMetadata::schema_approved();

        let result = ApproveSchemaResultOutput {
            contract_id: approval_result.contract.contract_id.to_string(),
            scope_id: scope_id.to_string(),
            version: approval_result.contract.version,
            schemas_approved: approval_result.contract.schemas.len(),
            warnings,
            workflow,
        };

        ToolResult::json(&result)
    }
}

// =============================================================================
// Propose Amendment Tool
// =============================================================================

/// Input for proposing an amendment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendmentProposalInput {
    /// Contract ID to amend
    pub contract_id: String,
    /// Type of amendment
    pub amendment_type: String,
    /// Details specific to the amendment type
    pub details: Value,
}

/// Result of proposing an amendment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeAmendmentResultOutput {
    /// Amendment ID
    pub amendment_id: String,
    /// Contract ID being amended
    pub contract_id: String,
    /// Reason for amendment
    pub reason: String,
    /// Proposed changes
    pub changes: Vec<String>,
    /// Status
    pub status: String,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Propose changes to existing schema contracts
///
/// This tool creates amendment proposals when data doesn't match the
/// existing contract. Amendments must be approved before taking effect.
pub struct ProposeAmendmentTool;

impl ProposeAmendmentTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProposeAmendmentTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ProposeAmendmentTool {
    fn name(&self) -> &str {
        "propose_amendment"
    }

    fn description(&self) -> &str {
        "Propose changes to existing schema contracts. Use when data doesn't match the current contract."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "contract_id": {
                    "type": "string",
                    "description": "ID of the contract to amend"
                },
                "amendment_type": {
                    "type": "string",
                    "enum": ["type_mismatch", "nullability_change", "new_columns", "user_requested"],
                    "description": "Type of amendment to propose"
                },
                "column": {
                    "type": "string",
                    "description": "Column name (for type_mismatch and nullability_change)"
                },
                "proposed_type": {
                    "type": "string",
                    "description": "New data type to propose (for type_mismatch)"
                },
                "new_columns": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "data_type": { "type": "string" },
                            "nullable": { "type": "boolean" }
                        }
                    },
                    "description": "New columns to add (for new_columns type)"
                },
                "reason": {
                    "type": "string",
                    "description": "Human-readable reason for the amendment"
                },
                "sample_values": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Sample values that caused the issue"
                }
            }),
            vec!["contract_id".to_string(), "amendment_type".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Extract parameters
        let contract_id_str = args
            .get("contract_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'contract_id' parameter".into()))?;

        let contract_id = Uuid::parse_str(contract_id_str)
            .map_err(|e| ToolError::InvalidParams(format!("Invalid contract_id: {}", e)))?;

        let amendment_type = args
            .get("amendment_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'amendment_type' parameter".into()))?;

        // Build the amendment reason based on type
        let (reason, changes) = match amendment_type {
            "type_mismatch" => {
                let column = args
                    .get("column")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidParams("'column' required for type_mismatch".into()))?;

                let proposed_type = args
                    .get("proposed_type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidParams("'proposed_type' required for type_mismatch".into()))?;

                let sample_values: Vec<String> = args
                    .get("sample_values")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                let reason = format!(
                    "Type mismatch in column '{}': proposing change to {}",
                    column, proposed_type
                );

                let change = format!(
                    "Change column '{}' type to {} (samples: {})",
                    column,
                    proposed_type,
                    sample_values.join(", ")
                );

                (reason, vec![change])
            }
            "nullability_change" => {
                let column = args
                    .get("column")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidParams("'column' required for nullability_change".into()))?;

                let reason = format!("Nullability change for column '{}'", column);
                let change = format!("Make column '{}' nullable", column);

                (reason, vec![change])
            }
            "new_columns" => {
                let new_cols = args
                    .get("new_columns")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| ToolError::InvalidParams("'new_columns' required for new_columns type".into()))?;

                let col_names: Vec<String> = new_cols
                    .iter()
                    .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect();

                let reason = format!("New columns detected: {}", col_names.join(", "));
                let changes: Vec<String> = col_names
                    .iter()
                    .map(|n| format!("Add column '{}'", n))
                    .collect();

                (reason, changes)
            }
            "user_requested" => {
                let reason = args
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("User requested amendment")
                    .to_string();

                (reason.clone(), vec![reason])
            }
            _ => {
                return Err(ToolError::InvalidParams(format!(
                    "Unknown amendment_type: {}",
                    amendment_type
                )));
            }
        };

        // Create amendment ID
        let amendment_id = Uuid::new_v4();

        // Build workflow metadata - amendment proposed, needs approval
        let decision = HumanDecision::new(format!("Approve amendment: {}?", reason))
            .with_options(vec![
                DecisionOption::new("approve", "Approve", "Accept the proposed changes"),
                DecisionOption::new("reject", "Reject", "Reject the proposed changes"),
                DecisionOption::new("modify", "Modify", "Modify the proposal before approving"),
            ])
            .with_context(format!("contract: {}", contract_id));

        let workflow = WorkflowMetadata::schema_approval_needed()
            .with_decision(decision);

        let result = ProposeAmendmentResultOutput {
            amendment_id: amendment_id.to_string(),
            contract_id: contract_id.to_string(),
            reason,
            changes,
            status: "pending".to_string(),
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
    async fn test_discover_schemas_csv() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test.csv");

        fs::write(
            &csv_path,
            "id,name,value,active\n1,Alice,10.5,true\n2,Bob,20.3,false\n3,Carol,30.1,true",
        )
        .unwrap();

        let tool = DiscoverSchemasTool::new();

        let args = json!({
            "files": [csv_path.to_string_lossy().to_string()],
            "max_rows": 100
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let discover_result: DiscoverSchemasResult = serde_json::from_str(text).unwrap();
            assert_eq!(discover_result.files_analyzed, 1);
            assert_eq!(discover_result.schemas.len(), 1);

            let schema = &discover_result.schemas[0];
            assert_eq!(schema.columns.len(), 4);
            assert_eq!(schema.columns[0].name, "id");
            assert_eq!(schema.columns[1].name, "name");
        }
    }

    #[tokio::test]
    async fn test_discover_schemas_missing_file() {
        let tool = DiscoverSchemasTool::new();

        let args = json!({
            "files": ["/nonexistent/file.csv"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let discover_result: DiscoverSchemasResult = serde_json::from_str(text).unwrap();
            assert_eq!(discover_result.files_analyzed, 0);
            assert!(!discover_result.failed_files.is_empty());
        }
    }

    #[tokio::test]
    async fn test_approve_schemas_basic() {
        let tool = ApproveSchemasTool::new();

        let args = json!({
            "scope_id": "test-scope-123",
            "schemas": [{
                "discovery_id": "disc-1",
                "name": "transactions",
                "output_table_name": "transactions",
                "columns": [
                    { "name": "id", "data_type": "int64", "nullable": false },
                    { "name": "amount", "data_type": "float64", "nullable": true }
                ],
                "description": "Transaction data"
            }]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let approve_result: ApproveSchemaResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(approve_result.schemas_approved, 1);
            assert_eq!(approve_result.version, 1);
        }
    }

    #[tokio::test]
    async fn test_propose_amendment_type_mismatch() {
        let tool = ProposeAmendmentTool::new();

        let args = json!({
            "contract_id": "550e8400-e29b-41d4-a716-446655440000",
            "amendment_type": "type_mismatch",
            "column": "amount",
            "proposed_type": "string",
            "sample_values": ["N/A", "unknown"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let amend_result: ProposeAmendmentResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(amend_result.status, "pending");
            assert!(amend_result.reason.contains("amount"));
        }
    }

    #[tokio::test]
    async fn test_propose_amendment_new_columns() {
        let tool = ProposeAmendmentTool::new();

        let args = json!({
            "contract_id": "550e8400-e29b-41d4-a716-446655440000",
            "amendment_type": "new_columns",
            "new_columns": [
                { "name": "created_at", "data_type": "timestamp", "nullable": true },
                { "name": "updated_at", "data_type": "timestamp", "nullable": true }
            ]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let amend_result: ProposeAmendmentResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(amend_result.changes.len(), 2);
        }
    }

    #[test]
    fn test_discover_schemas_schema() {
        let tool = DiscoverSchemasTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.as_ref().unwrap().contains(&"files".to_string()));
    }

    #[test]
    fn test_approve_schemas_schema() {
        let tool = ApproveSchemasTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        let required = schema.required.as_ref().unwrap();
        assert!(required.contains(&"scope_id".to_string()));
        assert!(required.contains(&"schemas".to_string()));
    }

    #[test]
    fn test_propose_amendment_schema() {
        let tool = ProposeAmendmentTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        let required = schema.required.as_ref().unwrap();
        assert!(required.contains(&"contract_id".to_string()));
        assert!(required.contains(&"amendment_type".to_string()));
    }
}
