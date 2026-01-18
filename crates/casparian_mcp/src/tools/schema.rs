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
// Constraint Visibility Types
// =============================================================================

/// A type that was eliminated during inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EliminatedType {
    /// Name of the type that was eliminated (e.g., "int64", "date")
    pub type_name: String,
    /// Human-readable reason why this type was eliminated
    pub reason: String,
    /// Sample values that caused this type to be eliminated
    pub counter_examples: Vec<String>,
}

/// Evidence for type inference decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeEvidence {
    /// Sample values that informed this decision
    pub sample_values: Vec<String>,
    /// Types that were eliminated and why
    pub eliminated_types: Vec<EliminatedType>,
    /// Percentage of values that match this type (0.0 - 100.0)
    pub match_percentage: f32,
}

/// A possible alternative type with confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeType {
    /// The alternative type name
    pub type_name: String,
    /// Confidence in this alternative (0.0 - 1.0)
    pub confidence: f32,
    /// What would change if this type were chosen
    pub what_would_change: String,
}

/// Constraint reasoning for a column - explains WHY types were inferred
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnConstraint {
    /// The resolved type
    pub resolved_type: String,
    /// Confidence in this decision (0.0 - 1.0)
    pub confidence: f32,
    /// Evidence that led to this decision
    pub evidence: TypeEvidence,
    /// Assumptions made (e.g., "no nulls", "decimal format")
    pub assumptions: Vec<String>,
    /// Whether this needs human decision
    pub needs_human_decision: bool,
    /// Question to ask human if ambiguous
    pub human_question: Option<String>,
    /// Possible alternative types if ambiguous
    pub alternatives: Vec<AlternativeType>,
}

impl Default for ColumnConstraint {
    fn default() -> Self {
        Self {
            resolved_type: "string".to_string(),
            confidence: 1.0,
            evidence: TypeEvidence {
                sample_values: vec![],
                eliminated_types: vec![],
                match_percentage: 100.0,
            },
            assumptions: vec![],
            needs_human_decision: false,
            human_question: None,
            alternatives: vec![],
        }
    }
}

/// A group of schemas with similar structure (for bulk approval)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaGroup {
    /// Unique identifier for this group
    pub group_id: String,
    /// Human-readable name for the group
    pub name: String,
    /// Schema discovery IDs in this group
    pub schema_ids: Vec<String>,
    /// Columns common to all schemas in this group
    pub common_columns: Vec<String>,
    /// A representative sample file
    pub sample_file: String,
}

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
    /// Full constraint reasoning - explains WHY this type was inferred
    pub constraint: ColumnConstraint,
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
    /// Groups of similar schemas for bulk approval workflow
    pub schema_groups: Vec<SchemaGroup>,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Analyze files to discover their schema structure
///
/// This tool examines files to infer their schema, including column names,
/// data types, and nullability.
pub struct DiscoverSchemasTool;

/// Result of type inference with full constraint reasoning
#[derive(Debug, Clone)]
struct TypeInferenceResult {
    /// The inferred type
    data_type: String,
    /// Whether the inference is ambiguous
    is_ambiguous: bool,
    /// Alternative types
    alternatives: Vec<String>,
    /// Full constraint reasoning
    constraint: ColumnConstraint,
}

impl DiscoverSchemasTool {
    pub fn new() -> Self {
        Self
    }

    /// Infer data type from a sample of values with full constraint reasoning
    fn infer_type_with_constraints(&self, column_name: &str, values: &[&str]) -> TypeInferenceResult {
        if values.is_empty() {
            return TypeInferenceResult {
                data_type: "string".to_string(),
                is_ambiguous: false,
                alternatives: vec![],
                constraint: ColumnConstraint {
                    resolved_type: "string".to_string(),
                    confidence: 1.0,
                    evidence: TypeEvidence {
                        sample_values: vec![],
                        eliminated_types: vec![],
                        match_percentage: 100.0,
                    },
                    assumptions: vec!["No non-null values to analyze".to_string()],
                    needs_human_decision: false,
                    human_question: None,
                    alternatives: vec![],
                },
            };
        }

        // Track per-type statistics and counter-examples
        let mut type_counts: HashMap<&str, usize> = HashMap::new();
        let mut type_failures: HashMap<&str, Vec<String>> = HashMap::new();
        let candidate_types = ["int64", "float64", "boolean", "date", "timestamp"];

        for type_name in &candidate_types {
            type_failures.insert(type_name, Vec::new());
        }

        let mut date_format_counts: HashMap<&str, usize> = HashMap::new();
        let mut ambiguous_date_values: Vec<String> = Vec::new();

        for value in values {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }

            // Check each type and track why it fails
            let is_int = value.parse::<i64>().is_ok();
            let is_float = value.parse::<f64>().is_ok();
            let is_bool = Self::is_boolean(value);
            let (is_date, date_format) = Self::check_date_with_format(value);
            let is_timestamp = Self::is_timestamp(value);

            // Track successful parses
            if is_int {
                *type_counts.entry("int64").or_insert(0) += 1;
            } else if !is_float {
                // Only record failure if not even a float
                type_failures.get_mut("int64").unwrap().push(value.to_string());
            }

            if is_float && !is_int {
                *type_counts.entry("float64").or_insert(0) += 1;
            } else if !is_int && !is_float {
                type_failures.get_mut("float64").unwrap().push(value.to_string());
            }

            if is_bool {
                *type_counts.entry("boolean").or_insert(0) += 1;
            } else {
                type_failures.get_mut("boolean").unwrap().push(value.to_string());
            }

            if is_date {
                *type_counts.entry("date").or_insert(0) += 1;
                if let Some(fmt) = date_format {
                    *date_format_counts.entry(fmt).or_insert(0) += 1;
                }
                // Check for ambiguous date formats (could be MM/DD or DD/MM)
                if Self::is_ambiguous_date_format(value) {
                    ambiguous_date_values.push(value.to_string());
                }
            } else {
                type_failures.get_mut("date").unwrap().push(value.to_string());
            }

            if is_timestamp {
                *type_counts.entry("timestamp").or_insert(0) += 1;
            } else {
                type_failures.get_mut("timestamp").unwrap().push(value.to_string());
            }

            // If nothing matches, it's a string
            if !is_int && !is_float && !is_bool && !is_date && !is_timestamp {
                *type_counts.entry("string").or_insert(0) += 1;
            }
        }

        // Build eliminated types with reasons
        let total_values = values.iter().filter(|v| !v.trim().is_empty()).count();
        let mut eliminated_types = Vec::new();

        for type_name in &candidate_types {
            let count = type_counts.get(type_name).copied().unwrap_or(0);
            let failures = type_failures.get(type_name).unwrap();

            if count == 0 && !failures.is_empty() {
                // Type was completely eliminated
                let reason = Self::describe_elimination_reason(type_name, failures);
                eliminated_types.push(EliminatedType {
                    type_name: type_name.to_string(),
                    reason,
                    counter_examples: failures.iter().take(3).cloned().collect(),
                });
            } else if count > 0 && count < total_values && !failures.is_empty() {
                // Partial match - type possible but not for all values
                let match_pct = (count as f32 / total_values as f32) * 100.0;
                eliminated_types.push(EliminatedType {
                    type_name: type_name.to_string(),
                    reason: format!(
                        "Only {:.1}% of values match this type ({} of {} values)",
                        match_pct, count, total_values
                    ),
                    counter_examples: failures.iter().take(3).cloned().collect(),
                });
            }
        }

        // Determine best type
        let mut sorted: Vec<_> = type_counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        let (best_type, match_percentage) = if sorted.is_empty() || *sorted[0].1 == 0 {
            ("string".to_string(), 100.0)
        } else {
            let best = sorted[0].0.to_string();
            let pct = (*sorted[0].1 as f32 / total_values as f32) * 100.0;
            (best, pct)
        };

        // Calculate confidence and check for ambiguity
        let is_ambiguous = sorted.len() > 1 && sorted[1].1 > &(total_values / 10);

        let alternatives_vec: Vec<String> = sorted
            .iter()
            .skip(1)
            .filter(|(_, count)| **count > total_values / 20)
            .map(|(t, _)| t.to_string())
            .collect();

        // Build alternative types with confidence and what would change
        let mut alternative_types = Vec::new();
        for (type_name, count) in sorted.iter().skip(1) {
            if **count > total_values / 20 {
                let alt_confidence = **count as f32 / total_values as f32;
                alternative_types.push(AlternativeType {
                    type_name: type_name.to_string(),
                    confidence: alt_confidence,
                    what_would_change: Self::describe_type_change(&best_type, type_name),
                });
            }
        }

        // Calculate confidence
        let confidence = if is_ambiguous {
            match_percentage / 100.0 * 0.7  // Reduce confidence for ambiguous cases
        } else {
            match_percentage / 100.0
        };

        // Build assumptions
        let mut assumptions = Vec::new();
        if match_percentage < 100.0 {
            assumptions.push(format!(
                "Assuming {:.1}% non-matching values are data errors",
                100.0 - match_percentage
            ));
        }
        if best_type == "date" && !ambiguous_date_values.is_empty() {
            assumptions.push("Date format assumed based on value patterns".to_string());
        }

        // Determine if human decision is needed
        let needs_human_decision = is_ambiguous
            || (best_type == "date" && !ambiguous_date_values.is_empty())
            || match_percentage < 90.0;

        // Generate human question if needed
        let human_question = if best_type == "date" && !ambiguous_date_values.is_empty() {
            Some(format!(
                "Column '{}' has values like '{}'. Is this MM/DD (US format) or DD/MM (European format)?",
                column_name,
                ambiguous_date_values.first().unwrap_or(&"unknown".to_string())
            ))
        } else if is_ambiguous {
            Some(format!(
                "Column '{}' could be {} or {}. Sample values: {}. Which type should be used?",
                column_name,
                best_type,
                alternatives_vec.join(", "),
                values.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            ))
        } else if match_percentage < 90.0 {
            Some(format!(
                "Column '{}' has {:.1}% values that don't match type {}. Should these be treated as errors or should the type be string?",
                column_name,
                100.0 - match_percentage,
                best_type
            ))
        } else {
            None
        };

        TypeInferenceResult {
            data_type: best_type.clone(),
            is_ambiguous,
            alternatives: alternatives_vec,
            constraint: ColumnConstraint {
                resolved_type: best_type,
                confidence,
                evidence: TypeEvidence {
                    sample_values: values.iter().take(5).map(|s| s.to_string()).collect(),
                    eliminated_types,
                    match_percentage,
                },
                assumptions,
                needs_human_decision,
                human_question,
                alternatives: alternative_types,
            },
        }
    }

    /// Describe why a type was eliminated
    fn describe_elimination_reason(type_name: &str, failures: &[String]) -> String {
        let sample = failures.first().map(|s| s.as_str()).unwrap_or("(none)");
        match type_name {
            "int64" => format!("Values contain non-integer characters (e.g., '{}')", sample),
            "float64" => format!("Values contain non-numeric characters (e.g., '{}')", sample),
            "boolean" => format!("Values are not boolean-like (e.g., '{}')", sample),
            "date" => format!("Values don't match any supported date format (e.g., '{}')", sample),
            "timestamp" => format!("Values don't match any supported timestamp format (e.g., '{}')", sample),
            _ => format!("Values don't match type {} (e.g., '{}')", type_name, sample),
        }
    }

    /// Describe what would change if a different type were chosen
    fn describe_type_change(from_type: &str, to_type: &str) -> String {
        match (from_type, to_type) {
            ("int64", "float64") => "Values would be stored as decimals; integer arithmetic preserved".to_string(),
            ("int64", "string") => "Numeric operations would not be available; lexicographic sorting".to_string(),
            ("float64", "string") => "Numeric operations would not be available; values stored as text".to_string(),
            ("date", "string") => "Date operations not available; values stored as plain text".to_string(),
            ("date", "timestamp") => "Values would include time component (midnight assumed)".to_string(),
            ("boolean", "string") => "Boolean logic not available; values stored as text".to_string(),
            ("boolean", "int64") => "true/false would become 1/0; numeric operations available".to_string(),
            _ => format!("Values would be interpreted as {} instead of {}", to_type, from_type),
        }
    }

    fn is_boolean(value: &str) -> bool {
        matches!(
            value.to_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "t" | "f" | "y" | "n" | "1" | "0"
        )
    }

    /// Check if a value is a date and return the detected format
    fn check_date_with_format(value: &str) -> (bool, Option<&'static str>) {
        if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() {
            return (true, Some("YYYY-MM-DD"));
        }
        if chrono::NaiveDate::parse_from_str(value, "%m/%d/%Y").is_ok() {
            return (true, Some("MM/DD/YYYY"));
        }
        if chrono::NaiveDate::parse_from_str(value, "%d/%m/%Y").is_ok() {
            return (true, Some("DD/MM/YYYY"));
        }
        if chrono::NaiveDate::parse_from_str(value, "%d-%m-%Y").is_ok() {
            return (true, Some("DD-MM-YYYY"));
        }
        if chrono::NaiveDate::parse_from_str(value, "%m-%d-%Y").is_ok() {
            return (true, Some("MM-DD-YYYY"));
        }
        (false, None)
    }

    /// Check if a date value has an ambiguous format (could be MM/DD or DD/MM)
    fn is_ambiguous_date_format(value: &str) -> bool {
        // Parse the value to extract components
        let parts: Vec<&str> = if value.contains('/') {
            value.split('/').collect()
        } else if value.contains('-') {
            value.split('-').collect()
        } else {
            return false;
        };

        if parts.len() < 3 {
            return false;
        }

        // Try to parse the first two components as numbers
        let first: i32 = parts[0].parse().unwrap_or(0);
        let second: i32 = parts[1].parse().unwrap_or(0);

        // If both are <= 12, it's ambiguous (could be month or day)
        first > 0 && first <= 12 && second > 0 && second <= 12 && first != second
    }

    fn is_timestamp(value: &str) -> bool {
        chrono::DateTime::parse_from_rfc3339(value).is_ok()
            || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok()
            || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").is_ok()
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

        // Build columns with constraint reasoning
        let mut columns = Vec::new();
        let mut warnings = Vec::new();

        for (i, name) in column_names.iter().enumerate() {
            let values_refs: Vec<&str> = column_values[i].iter().map(|s| s.as_str()).collect();
            let inference_result = self.infer_type_with_constraints(name, &values_refs);

            let null_percentage = if rows_analyzed > 0 {
                (null_counts[i] as f32 / rows_analyzed as f32) * 100.0
            } else {
                0.0
            };

            if inference_result.is_ambiguous {
                warnings.push(format!(
                    "Column '{}' has ambiguous type: {} (could also be: {})",
                    name,
                    inference_result.data_type,
                    inference_result.alternatives.join(", ")
                ));
            }

            if null_percentage > 50.0 {
                warnings.push(format!(
                    "Column '{}' has {:.1}% null values",
                    name, null_percentage
                ));
            }

            // Add warning for columns needing human decision
            if inference_result.constraint.needs_human_decision {
                if let Some(ref question) = inference_result.constraint.human_question {
                    warnings.push(format!("HUMAN DECISION NEEDED: {}", question));
                }
            }

            columns.push(DiscoveredColumn {
                name: name.clone(),
                data_type: inference_result.data_type,
                sample_values: column_values[i].iter().take(5).cloned().collect(),
                null_percentage,
                is_ambiguous: inference_result.is_ambiguous,
                alternative_types: inference_result.alternatives,
                constraint: inference_result.constraint,
            });
        }

        // Calculate confidence (average of column confidences)
        let avg_confidence = if columns.is_empty() {
            0.0
        } else {
            columns.iter().map(|c| c.constraint.confidence).sum::<f32>() / columns.len() as f32
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
            confidence: avg_confidence,
            warnings,
        })
    }

    /// Group schemas by similarity for bulk approval workflow
    fn group_schemas(schemas: &[DiscoveredSchema]) -> Vec<SchemaGroup> {
        if schemas.is_empty() {
            return vec![];
        }

        // Simple grouping: schemas with same column names (order matters)
        let mut groups: HashMap<String, Vec<&DiscoveredSchema>> = HashMap::new();

        for schema in schemas {
            let column_key: String = schema
                .columns
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>()
                .join(",");
            groups.entry(column_key).or_default().push(schema);
        }

        // Convert to SchemaGroup structs
        groups
            .into_iter()
            .enumerate()
            .map(|(idx, (column_key, schemas_in_group))| {
                let common_columns: Vec<String> = column_key.split(',').map(String::from).collect();
                let sample_file = schemas_in_group
                    .first()
                    .map(|s| s.source_file.clone())
                    .unwrap_or_default();

                SchemaGroup {
                    group_id: Uuid::new_v4().to_string(),
                    name: format!("Group {} ({} columns)", idx + 1, common_columns.len()),
                    schema_ids: schemas_in_group.iter().map(|s| s.discovery_id.clone()).collect(),
                    common_columns,
                    sample_file,
                }
            })
            .collect()
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

        // Group schemas by similarity for bulk approval workflow
        let schema_groups = Self::group_schemas(&schemas);

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
            schema_groups,
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
                    "description": "Parser ID (legacy scope_id input; used if parser_id is omitted)"
                },
                "parser_id": {
                    "type": "string",
                    "description": "Parser identifier (defaults to scope_id if omitted)"
                },
                "parser_version": {
                    "type": "string",
                    "description": "Parser version string (defaults to 'unknown' if omitted)"
                },
                "logic_hash": {
                    "type": "string",
                    "description": "Advisory hash of parser logic/config"
                },
                "allow_nested_types": {
                    "type": "boolean",
                    "description": "Allow List/Struct types in approved schemas (default: false)"
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
        let scope_id_input = args
            .get("scope_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'scope_id' parameter".into()))?;

        let parser_id = args
            .get("parser_id")
            .and_then(|v| v.as_str())
            .unwrap_or(scope_id_input);

        let parser_version = args
            .get("parser_version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let logic_hash = args
            .get("logic_hash")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string());

        let allow_nested_types = args
            .get("allow_nested_types")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

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
        let storage = SchemaStorage::in_memory().await
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
        let mut request = SchemaApprovalRequest::new(
            discovery_id,
            parser_id,
            parser_version,
            approved_by,
        )
        .with_schemas(approved_variants)
        .with_allow_nested_types(allow_nested_types);

        if let Some(hash) = logic_hash {
            request = request.with_logic_hash(hash);
        }

        // Approve and create contract
        let approval_result = casparian_schema::approval::approve_schema(&storage, request).await
            .map_err(|e| ToolError::ExecutionFailed(format!("Approval failed: {}", e)))?;

        // Collect all warnings
        for w in &approval_result.warnings {
            warnings.push(w.message.clone());
        }

        // Build workflow metadata - schema is now approved, move to backtest phase
        let workflow = WorkflowMetadata::schema_approved();

        let result = ApproveSchemaResultOutput {
            contract_id: approval_result.contract.contract_id.to_string(),
            scope_id: approval_result.contract.scope_id.clone(),
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

    // =========================================================================
    // Constraint Visibility Tests
    // =========================================================================

    #[tokio::test]
    async fn test_constraint_visibility_int_column() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("integers.csv");

        // Use larger numbers that are clearly not booleans
        fs::write(
            &csv_path,
            "id,count\n10,100\n20,200\n30,300\n40,400\n50,500",
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
            let schema = &discover_result.schemas[0];

            // Check constraint reasoning for int column
            let id_col = &schema.columns[0];
            assert_eq!(id_col.data_type, "int64");
            // High confidence for clean integer data
            assert!(id_col.constraint.confidence > 0.9);

            // Verify eliminated types are recorded
            let eliminated_types: Vec<&str> = id_col.constraint.evidence.eliminated_types
                .iter()
                .map(|e| e.type_name.as_str())
                .collect();
            // Date, timestamp, and boolean should be eliminated for these integer values
            assert!(eliminated_types.contains(&"date") || eliminated_types.contains(&"timestamp") || eliminated_types.contains(&"boolean"));
        }
    }

    #[tokio::test]
    async fn test_constraint_visibility_ambiguous_date() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("dates.csv");

        // 03/04/2024 is ambiguous: March 4 or April 3?
        fs::write(
            &csv_path,
            "date_field\n03/04/2024\n05/06/2024\n01/02/2024",
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
            let schema = &discover_result.schemas[0];

            // Ambiguous dates should trigger human decision
            let date_col = &schema.columns[0];
            assert_eq!(date_col.data_type, "date");
            assert!(date_col.constraint.needs_human_decision);
            assert!(date_col.constraint.human_question.is_some());

            // Question should mention MM/DD vs DD/MM
            let question = date_col.constraint.human_question.as_ref().unwrap();
            assert!(question.contains("MM/DD") || question.contains("DD/MM"));
        }
    }

    #[tokio::test]
    async fn test_constraint_visibility_mixed_types() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("mixed.csv");

        // Column with mix of integers and strings
        fs::write(
            &csv_path,
            "value\n1\n2\nN/A\n4\nunknown",
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
            let schema = &discover_result.schemas[0];

            let value_col = &schema.columns[0];

            // Should note the mixed types
            assert!(value_col.constraint.evidence.match_percentage < 100.0);

            // Should have counter-examples in eliminated types
            let has_counter_examples = value_col.constraint.evidence.eliminated_types
                .iter()
                .any(|e| !e.counter_examples.is_empty());
            assert!(has_counter_examples || value_col.constraint.alternatives.len() > 0);
        }
    }

    #[tokio::test]
    async fn test_schema_grouping() {
        let temp_dir = TempDir::new().unwrap();

        // Create two files with same columns
        let csv1_path = temp_dir.path().join("orders_jan.csv");
        let csv2_path = temp_dir.path().join("orders_feb.csv");
        let csv3_path = temp_dir.path().join("products.csv");

        fs::write(
            &csv1_path,
            "order_id,customer,amount\n1,Alice,100\n2,Bob,200",
        )
        .unwrap();
        fs::write(
            &csv2_path,
            "order_id,customer,amount\n3,Carol,300\n4,Dave,400",
        )
        .unwrap();
        fs::write(
            &csv3_path,
            "product_id,name,price\n1,Widget,10\n2,Gadget,20",
        )
        .unwrap();

        let tool = DiscoverSchemasTool::new();
        let args = json!({
            "files": [
                csv1_path.to_string_lossy().to_string(),
                csv2_path.to_string_lossy().to_string(),
                csv3_path.to_string_lossy().to_string()
            ],
            "max_rows": 100
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let discover_result: DiscoverSchemasResult = serde_json::from_str(text).unwrap();

            // Should have 3 schemas
            assert_eq!(discover_result.schemas.len(), 3);

            // Should have 2 groups (orders and products)
            assert_eq!(discover_result.schema_groups.len(), 2);

            // Find the orders group (should have 2 schemas)
            let orders_group = discover_result.schema_groups.iter()
                .find(|g| g.schema_ids.len() == 2);
            assert!(orders_group.is_some());

            let orders_group = orders_group.unwrap();
            assert_eq!(orders_group.common_columns, vec!["order_id", "customer", "amount"]);
        }
    }

    #[test]
    fn test_type_evidence_serialization() {
        let evidence = TypeEvidence {
            sample_values: vec!["1".to_string(), "2".to_string()],
            eliminated_types: vec![
                EliminatedType {
                    type_name: "date".to_string(),
                    reason: "Values don't match date format".to_string(),
                    counter_examples: vec!["1".to_string()],
                }
            ],
            match_percentage: 100.0,
        };

        let json = serde_json::to_string(&evidence).unwrap();
        assert!(json.contains("sample_values"));
        assert!(json.contains("eliminated_types"));
        assert!(json.contains("match_percentage"));

        // Deserialize back
        let parsed: TypeEvidence = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.sample_values.len(), 2);
        assert_eq!(parsed.eliminated_types.len(), 1);
    }

    #[test]
    fn test_column_constraint_default() {
        let constraint = ColumnConstraint::default();
        assert_eq!(constraint.resolved_type, "string");
        assert_eq!(constraint.confidence, 1.0);
        assert!(!constraint.needs_human_decision);
        assert!(constraint.human_question.is_none());
    }
}
