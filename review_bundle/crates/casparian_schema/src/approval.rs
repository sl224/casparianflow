//! Schema Approval Workflow
//!
//! This module handles the transition from discovery (intent) to contract (commitment).
//! When a user approves a schema, they are creating a binding contract that the parser
//! must conform to exactly.
//!
//! # Workflow
//!
//! 1. Schema discovery produces proposed variants
//! 2. User reviews and potentially modifies columns/types
//! 3. User answers any ambiguity questions
//! 4. User optionally excludes problematic files
//! 5. User clicks "Approve" -> SchemaContract is created
//!
//! After approval, any deviation from the contract is a FAILURE.

use crate::ids::{DiscoveryId, SchemaTimestamp, SchemaVariantId};
use crate::storage::{SchemaStorage, StorageError};
use crate::{DataType, LockedColumn, LockedSchema, SchemaContract};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Errors that can occur during schema approval.
#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Discovery not found: {0}")]
    DiscoveryNotFound(String),

    #[error("Discovery already processed: {0}")]
    AlreadyProcessed(String),

    #[error("No schemas approved")]
    NoSchemasApproved,

    #[error("Multiple output tables are not supported in v1 scope_id derivation")]
    MultipleOutputsNotSupported,

    #[error("Invalid column configuration: {0}")]
    InvalidColumn(String),

    #[error("Unanswered required question: {0}")]
    UnansweredQuestion(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

/// A request to approve a schema discovery and create a contract.
///
/// This is the bridge between "intent" (discovery) and "contract" (locked schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaApprovalRequest {
    /// ID of the discovery result being approved
    pub discovery_id: DiscoveryId,

    /// Parser identifier (stable across versions)
    #[serde(default)]
    pub parser_id: String,

    /// Parser version for this approval
    #[serde(default)]
    pub parser_version: String,

    /// Advisory hash of parser logic/config (optional)
    pub logic_hash: Option<String>,

    /// Feature flag: allow List/Struct types in approved schemas
    #[serde(default)]
    pub allow_nested_types: bool,

    /// The approved schema variants (user may have modified columns/types)
    pub approved_schemas: Vec<ApprovedSchemaVariant>,

    /// Answers to any ambiguity questions from discovery
    pub question_answers: Vec<QuestionAnswer>,

    /// Files to exclude from processing (bad data, outliers, etc.)
    pub excluded_files: Vec<String>,

    /// User who is approving
    pub approved_by: String,

    /// Optional description of why this approval was made
    pub approval_notes: Option<String>,
}

impl SchemaApprovalRequest {
    /// Create a new approval request
    pub fn new(
        discovery_id: DiscoveryId,
        parser_id: impl Into<String>,
        parser_version: impl Into<String>,
        approved_by: impl Into<String>,
    ) -> Self {
        Self {
            discovery_id,
            parser_id: parser_id.into(),
            parser_version: parser_version.into(),
            logic_hash: None,
            allow_nested_types: false,
            approved_schemas: Vec::new(),
            question_answers: Vec::new(),
            excluded_files: Vec::new(),
            approved_by: approved_by.into(),
            approval_notes: None,
        }
    }

    /// Add an approved schema variant
    pub fn with_schema(mut self, schema: ApprovedSchemaVariant) -> Self {
        self.approved_schemas.push(schema);
        self
    }

    /// Add multiple approved schema variants
    pub fn with_schemas(mut self, schemas: Vec<ApprovedSchemaVariant>) -> Self {
        self.approved_schemas = schemas;
        self
    }

    /// Add an answer to an ambiguity question
    pub fn with_answer(mut self, answer: QuestionAnswer) -> Self {
        self.question_answers.push(answer);
        self
    }

    /// Add files to exclude
    pub fn exclude_files(mut self, files: Vec<String>) -> Self {
        self.excluded_files = files;
        self
    }

    /// Add approval notes
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.approval_notes = Some(notes.into());
        self
    }

    /// Set an advisory logic hash for the parser
    pub fn with_logic_hash(mut self, logic_hash: impl Into<String>) -> Self {
        self.logic_hash = Some(logic_hash.into());
        self
    }

    /// Allow nested List/Struct types in approved schemas
    pub fn with_allow_nested_types(mut self, allow: bool) -> Self {
        self.allow_nested_types = allow;
        self
    }
}

/// An approved schema variant with user modifications.
///
/// This represents what the user actually approved, which may differ
/// from what was originally proposed during discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovedSchemaVariant {
    /// ID of this variant (for tracking)
    pub variant_id: SchemaVariantId,

    /// Human-readable name for this schema
    pub name: String,

    /// The approved columns (user may have modified types, nullability, etc.)
    pub columns: Vec<ApprovedColumn>,

    /// Name of the output table (user may rename)
    pub output_table_name: String,

    /// Pattern for files this schema applies to
    pub source_pattern: Option<String>,

    /// Description of what this schema represents
    pub description: Option<String>,
}

impl ApprovedSchemaVariant {
    /// Create a new approved schema variant
    pub fn new(name: impl Into<String>, output_table_name: impl Into<String>) -> Self {
        Self {
            variant_id: SchemaVariantId::new(),
            name: name.into(),
            columns: Vec::new(),
            output_table_name: output_table_name.into(),
            source_pattern: None,
            description: None,
        }
    }

    /// Add columns
    pub fn with_columns(mut self, columns: Vec<ApprovedColumn>) -> Self {
        self.columns = columns;
        self
    }

    /// Set source pattern
    pub fn with_source_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.source_pattern = Some(pattern.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Convert to a LockedSchema for the contract
    pub fn to_locked_schema(&self) -> LockedSchema {
        let columns = self.columns.iter().map(|c| c.to_locked_column()).collect();
        let mut schema = LockedSchema::new(&self.output_table_name, columns);
        if let Some(ref pattern) = self.source_pattern {
            schema = schema.with_source_pattern(pattern);
        }
        schema
    }
}

/// A column definition approved by the user.
///
/// The user may have modified the discovered type, nullability,
/// format, or even renamed the column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovedColumn {
    /// Original column name from discovery
    pub name: String,

    /// Data type (user may have changed from discovery)
    pub data_type: DataType,

    /// Whether null values are allowed
    pub nullable: bool,

    /// Optional format string (for dates, etc.)
    pub format: Option<String>,

    /// Optional: rename this column in output
    pub rename_to: Option<String>,

    /// Optional: default value for nulls (user may set)
    pub default_value: Option<String>,

    /// Optional: description for documentation
    pub description: Option<String>,
}

impl ApprovedColumn {
    /// Create a new required column
    pub fn required(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable: false,
            format: None,
            rename_to: None,
            default_value: None,
            description: None,
        }
    }

    /// Create a new optional (nullable) column
    pub fn optional(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable: true,
            format: None,
            rename_to: None,
            default_value: None,
            description: None,
        }
    }

    /// Set format string
    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Rename column in output
    pub fn rename_to(mut self, new_name: impl Into<String>) -> Self {
        self.rename_to = Some(new_name.into());
        self
    }

    /// Set default value for nulls
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default_value = Some(default.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Convert to a LockedColumn for the contract
    pub fn to_locked_column(&self) -> LockedColumn {
        let name = self.rename_to.clone().unwrap_or_else(|| self.name.clone());
        let mut col = if self.nullable {
            LockedColumn::optional(&name, self.data_type.clone())
        } else {
            LockedColumn::required(&name, self.data_type.clone())
        };

        if let Some(ref fmt) = self.format {
            col = col.with_format(fmt);
        }
        if let Some(ref desc) = self.description {
            col = col.with_description(desc);
        }
        col
    }
}

/// An answer to an ambiguity question from schema discovery.
///
/// During discovery, the system may encounter ambiguous situations:
/// - A column has mixed types (strings and numbers)
/// - Date formats vary across files
/// - Some files have extra columns
///
/// The user must answer these questions to resolve ambiguity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnswer {
    /// ID of the question being answered
    pub question_id: String,

    /// The user's answer
    pub answer: AmbiguityResolution,

    /// Optional explanation from user
    pub explanation: Option<String>,
}

impl QuestionAnswer {
    /// Create a new question answer
    pub fn new(question_id: impl Into<String>, answer: AmbiguityResolution) -> Self {
        Self {
            question_id: question_id.into(),
            answer,
            explanation: None,
        }
    }

    /// Add explanation
    pub fn with_explanation(mut self, explanation: impl Into<String>) -> Self {
        self.explanation = Some(explanation.into());
        self
    }
}

/// How to resolve an ambiguity in schema discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AmbiguityResolution {
    /// Use the specified type for a mixed-type column
    UseType { column: String, data_type: DataType },

    /// Use the specified format for dates/timestamps
    UseFormat { column: String, format: String },

    /// Make the column nullable to handle missing values
    MakeNullable { column: String },

    /// Use a default value for missing/invalid values
    UseDefault {
        column: String,
        default_value: String,
    },

    /// Exclude files that don't match the majority pattern
    ExcludeNonConforming { file_pattern: String },

    /// Split into separate schemas (different file types)
    CreateSeparateSchema { variant_name: String },

    /// Custom resolution with user-provided logic
    Custom { description: String, action: String },
}

/// Result of a successful schema approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResult {
    /// The created contract
    pub contract: SchemaContract,

    /// Files that were excluded
    pub excluded_files: Vec<String>,

    /// Any warnings generated during approval
    pub warnings: Vec<ApprovalWarning>,

    /// When the approval happened
    pub approved_at: SchemaTimestamp,
}

/// Warning generated during approval (non-fatal issues).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalWarning {
    /// Type of warning
    pub warning_type: WarningType,

    /// Human-readable message
    pub message: String,

    /// Affected column (if applicable)
    pub column: Option<String>,
}

/// Types of warnings that can occur during approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningType {
    /// Column has high null percentage
    HighNullPercentage,

    /// Column name was renamed
    ColumnRenamed,

    /// Type was coerced (e.g., int to string)
    TypeCoerced,

    /// Files were excluded
    FilesExcluded,

    /// Default value will be applied
    DefaultApplied,

    /// Format string may not match all values
    FormatMayFail,
}

/// Derive a v1 scope_id from parser identity and output table name.
pub fn derive_scope_id(parser_id: &str, parser_version: &str, output_table_name: &str) -> String {
    let input = format!("{}:{}:{}", parser_id, parser_version, output_table_name);
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Approve a schema discovery and create a contract.
///
/// This is the main entry point for schema approval. It:
/// 1. Validates the approval request
/// 2. Applies user modifications
/// 3. Creates the locked schema contract
/// 4. Updates storage
pub fn approve_schema(
    storage: &SchemaStorage,
    request: SchemaApprovalRequest,
) -> Result<ApprovalResult, ApprovalError> {
    // Validate request
    if request.approved_schemas.is_empty() {
        return Err(ApprovalError::NoSchemasApproved);
    }

    if request.parser_id.trim().is_empty() {
        return Err(ApprovalError::Validation("parser_id is required".into()));
    }
    if request.parser_version.trim().is_empty() {
        return Err(ApprovalError::Validation(
            "parser_version is required".into(),
        ));
    }

    // Validate each schema variant
    for schema in &request.approved_schemas {
        validate_approved_schema(schema, request.allow_nested_types)?;
    }

    let output_table_name = request.approved_schemas[0].output_table_name.as_str();
    if request
        .approved_schemas
        .iter()
        .any(|schema| schema.output_table_name != output_table_name)
    {
        // v1 scope_id derivation assumes a single output table name.
        // If multiple outputs are present, reject rather than guess identity.
        return Err(ApprovalError::MultipleOutputsNotSupported);
    }

    // Convert approved variants to locked schemas
    let locked_schemas: Vec<LockedSchema> = request
        .approved_schemas
        .iter()
        .map(|v| v.to_locked_schema())
        .collect();

    // Generate warnings
    let warnings = generate_approval_warnings(&request);

    // Create the contract
    let scope_id = derive_scope_id(
        &request.parser_id,
        &request.parser_version,
        output_table_name,
    );
    let mut contract =
        SchemaContract::with_schemas(&scope_id, locked_schemas, &request.approved_by);
    contract.logic_hash = request.logic_hash.clone();

    // Save to storage
    storage.save_contract(&contract)?;

    Ok(ApprovalResult {
        contract,
        excluded_files: request.excluded_files,
        warnings,
        approved_at: SchemaTimestamp::now(),
    })
}

/// Validate an approved schema variant.
fn validate_approved_schema(
    schema: &ApprovedSchemaVariant,
    allow_nested_types: bool,
) -> Result<(), ApprovalError> {
    if schema.name.is_empty() {
        return Err(ApprovalError::InvalidColumn(
            "Schema name cannot be empty".into(),
        ));
    }

    if schema.output_table_name.is_empty() {
        return Err(ApprovalError::InvalidColumn(
            "Output table name cannot be empty".into(),
        ));
    }

    if schema.columns.is_empty() {
        return Err(ApprovalError::InvalidColumn(format!(
            "Schema '{}' must have at least one column",
            schema.name
        )));
    }

    // Validate each column
    for col in &schema.columns {
        if col.name.is_empty() {
            return Err(ApprovalError::InvalidColumn(
                "Column name cannot be empty".into(),
            ));
        }

        if !allow_nested_types
            && matches!(
                col.data_type,
                DataType::List { .. } | DataType::Struct { .. }
            )
        {
            return Err(ApprovalError::Validation(format!(
                "Nested List/Struct types require allow_nested_types: column '{}'",
                col.name
            )));
        }

        // Check for duplicate column names (including renames)
        let output_name = col.rename_to.as_ref().unwrap_or(&col.name);
        let duplicates: Vec<_> = schema
            .columns
            .iter()
            .filter(|c| {
                let name = c.rename_to.as_ref().unwrap_or(&c.name);
                name == output_name && !std::ptr::eq(*c, col)
            })
            .collect();

        if !duplicates.is_empty() {
            return Err(ApprovalError::InvalidColumn(format!(
                "Duplicate output column name: '{}'",
                output_name
            )));
        }
    }

    Ok(())
}

/// Generate warnings for an approval request.
fn generate_approval_warnings(request: &SchemaApprovalRequest) -> Vec<ApprovalWarning> {
    let mut warnings = Vec::new();

    // Warn about excluded files
    if !request.excluded_files.is_empty() {
        warnings.push(ApprovalWarning {
            warning_type: WarningType::FilesExcluded,
            message: format!(
                "{} files excluded from processing",
                request.excluded_files.len()
            ),
            column: None,
        });
    }

    // Warn about renamed columns
    for schema in &request.approved_schemas {
        for col in &schema.columns {
            if col.rename_to.is_some() {
                warnings.push(ApprovalWarning {
                    warning_type: WarningType::ColumnRenamed,
                    message: format!(
                        "Column '{}' will be renamed to '{}'",
                        col.name,
                        col.rename_to.as_ref().unwrap()
                    ),
                    column: Some(col.name.clone()),
                });
            }

            if col.default_value.is_some() {
                warnings.push(ApprovalWarning {
                    warning_type: WarningType::DefaultApplied,
                    message: format!(
                        "Default value '{}' will be used for nulls in column '{}'",
                        col.default_value.as_ref().unwrap(),
                        col.name
                    ),
                    column: Some(col.name.clone()),
                });
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SchemaStorage;

    fn create_test_storage() -> SchemaStorage {
        SchemaStorage::in_memory().unwrap()
    }

    fn new_request(approved_by: &str) -> SchemaApprovalRequest {
        SchemaApprovalRequest::new(DiscoveryId::new(), "parser-123", "1.0.0", approved_by)
    }

    #[test]
    fn test_create_approval_request() {
        let request = new_request("user123")
            .with_schema(
                ApprovedSchemaVariant::new("transactions", "transactions").with_columns(vec![
                    ApprovedColumn::required("id", DataType::Int64),
                    ApprovedColumn::required("amount", DataType::Float64),
                ]),
            )
            .with_notes("Approved after review");

        assert_eq!(request.approved_by, "user123");
        assert_eq!(request.parser_id, "parser-123");
        assert_eq!(request.parser_version, "1.0.0");
        assert_eq!(request.approved_schemas.len(), 1);
        assert_eq!(
            request.approval_notes,
            Some("Approved after review".to_string())
        );
    }

    #[test]
    fn test_approved_column_to_locked() {
        let col = ApprovedColumn::required("old_name", DataType::String)
            .rename_to("new_name")
            .with_format("%Y-%m-%d")
            .with_description("A date column");

        let locked = col.to_locked_column();
        assert_eq!(locked.name, "new_name");
        assert_eq!(locked.data_type, DataType::String);
        assert!(!locked.nullable);
        assert_eq!(locked.format, Some("%Y-%m-%d".to_string()));
        assert_eq!(locked.description, Some("A date column".to_string()));
    }

    #[test]
    fn test_approve_schema_success() {
        let storage = create_test_storage();

        let request = new_request("approver").with_schema(
            ApprovedSchemaVariant::new("test_schema", "test_output").with_columns(vec![
                ApprovedColumn::required("id", DataType::Int64),
                ApprovedColumn::optional("name", DataType::String),
            ]),
        );

        let result = approve_schema(&storage, request).unwrap();
        assert_eq!(result.contract.approved_by, "approver");
        assert_eq!(result.contract.schemas.len(), 1);
        assert_eq!(result.contract.schemas[0].name, "test_output");
        assert_eq!(result.contract.schemas[0].columns.len(), 2);
    }

    #[test]
    fn test_scope_id_and_logic_hash() {
        let storage = create_test_storage();

        let parser_id = "parser-alpha";
        let parser_version = "2024.05";
        let output_table_name = "events";
        let logic_hash = "deadbeef";

        let request =
            SchemaApprovalRequest::new(DiscoveryId::new(), parser_id, parser_version, "approver")
                .with_logic_hash(logic_hash)
                .with_schema(
                    ApprovedSchemaVariant::new("events", output_table_name)
                        .with_columns(vec![ApprovedColumn::required("id", DataType::Int64)]),
                );

        let result = approve_schema(&storage, request).unwrap();
        let expected_scope_id = derive_scope_id(parser_id, parser_version, output_table_name);
        assert_eq!(result.contract.scope_id, expected_scope_id);
        assert_eq!(result.contract.logic_hash.as_deref(), Some(logic_hash));
    }

    #[test]
    fn test_approve_schema_no_schemas() {
        let storage = create_test_storage();

        let request = new_request("user");

        let err = approve_schema(&storage, request).unwrap_err();
        assert!(matches!(err, ApprovalError::NoSchemasApproved));
    }

    #[test]
    fn test_approve_schema_empty_name() {
        let storage = create_test_storage();

        let request = new_request("user").with_schema(
            ApprovedSchemaVariant::new("", "output")
                .with_columns(vec![ApprovedColumn::required("id", DataType::Int64)]),
        );

        let err = approve_schema(&storage, request).unwrap_err();
        assert!(matches!(err, ApprovalError::InvalidColumn(_)));
    }

    #[test]
    fn test_approve_schema_no_columns() {
        let storage = create_test_storage();

        let request =
            new_request("user").with_schema(ApprovedSchemaVariant::new("test", "test_output"));

        let err = approve_schema(&storage, request).unwrap_err();
        assert!(matches!(err, ApprovalError::InvalidColumn(_)));
    }

    #[test]
    fn test_nested_types_rejected_by_default() {
        let storage = create_test_storage();

        let request = new_request("user").with_schema(
            ApprovedSchemaVariant::new("nested", "nested_output").with_columns(vec![
                ApprovedColumn::required(
                    "items",
                    DataType::List {
                        item: Box::new(DataType::String),
                    },
                ),
            ]),
        );

        let err = approve_schema(&storage, request).unwrap_err();
        assert!(matches!(err, ApprovalError::Validation(_)));
    }

    #[test]
    fn test_nested_types_allowed_with_flag() {
        let storage = create_test_storage();

        let request = new_request("user")
            .with_allow_nested_types(true)
            .with_schema(
                ApprovedSchemaVariant::new("nested", "nested_output").with_columns(vec![
                    ApprovedColumn::required(
                        "items",
                        DataType::List {
                            item: Box::new(DataType::String),
                        },
                    ),
                ]),
            );

        let result = approve_schema(&storage, request).unwrap();
        assert_eq!(result.contract.schemas.len(), 1);
    }

    #[test]
    fn test_approval_warnings_for_exclusions() {
        let storage = create_test_storage();

        let request = new_request("user")
            .with_schema(
                ApprovedSchemaVariant::new("test", "test_output")
                    .with_columns(vec![ApprovedColumn::required("id", DataType::Int64)]),
            )
            .exclude_files(vec!["bad1.csv".into(), "bad2.csv".into()]);

        let result = approve_schema(&storage, request).unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.warning_type == WarningType::FilesExcluded));
    }

    #[test]
    fn test_approval_warnings_for_rename() {
        let storage = create_test_storage();

        let request = new_request("user").with_schema(
            ApprovedSchemaVariant::new("test", "test_output").with_columns(vec![
                ApprovedColumn::required("old", DataType::Int64).rename_to("new"),
            ]),
        );

        let result = approve_schema(&storage, request).unwrap();
        assert!(result
            .warnings
            .iter()
            .any(|w| w.warning_type == WarningType::ColumnRenamed));
    }

    #[test]
    fn test_question_answer() {
        let answer = QuestionAnswer::new(
            "q1",
            AmbiguityResolution::UseType {
                column: "amount".to_string(),
                data_type: DataType::Float64,
            },
        )
        .with_explanation("User confirmed this should be float");

        assert_eq!(answer.question_id, "q1");
        assert!(answer.explanation.is_some());
    }

    #[test]
    fn test_multiple_output_tables_rejected() {
        let storage = create_test_storage();

        let request = new_request("user").with_schemas(vec![
            ApprovedSchemaVariant::new("schema_a", "output_a")
                .with_columns(vec![ApprovedColumn::required("id", DataType::Int64)])
                .with_source_pattern("*_a.csv"),
            ApprovedSchemaVariant::new("schema_b", "output_b")
                .with_columns(vec![
                    ApprovedColumn::required("id", DataType::Int64),
                    ApprovedColumn::optional("extra", DataType::String),
                ])
                .with_source_pattern("*_b.csv"),
        ]);

        let err = approve_schema(&storage, request).unwrap_err();
        assert!(matches!(err, ApprovalError::MultipleOutputsNotSupported));
    }

    #[test]
    fn test_duplicate_column_name_rejected() {
        let storage = create_test_storage();

        let request = new_request("user").with_schema(
            ApprovedSchemaVariant::new("test", "test_output").with_columns(vec![
                ApprovedColumn::required("id", DataType::Int64),
                ApprovedColumn::required("other", DataType::String).rename_to("id"),
            ]),
        );

        let err = approve_schema(&storage, request).unwrap_err();
        assert!(matches!(err, ApprovalError::InvalidColumn(_)));
    }
}
