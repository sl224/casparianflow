//! Schema Amendment Workflow
//!
//! This module handles controlled evolution of schema contracts.
//! Once a schema is locked as a contract, it cannot change arbitrarily.
//! Amendments provide a formal process for schema evolution.
//!
//! # When Amendments Are Needed
//!
//! 1. **New Schema Variant**: A new file type appears that doesn't match existing schemas
//! 2. **Type Mismatch**: A column that was Int64 now contains strings
//! 3. **Nullability Change**: A required column now has null values
//! 4. **New Columns**: Files now have additional columns not in the contract
//!
//! # Amendment Workflow
//!
//! 1. System detects schema violation during processing
//! 2. Amendment proposal is created with details
//! 3. User reviews the proposal
//! 4. User chooses an action (approve, modify, reject, etc.)
//! 5. Contract is updated (if approved) with new version

use crate::{DataType, LockedColumn, LockedSchema, SchemaContract};
use crate::storage::{SchemaStorage, StorageError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur during schema amendment.
#[derive(Debug, Error)]
pub enum AmendmentError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Contract not found: {0}")]
    ContractNotFound(String),

    #[error("Amendment not found: {0}")]
    AmendmentNotFound(Uuid),

    #[error("Amendment already processed: {0}")]
    AlreadyProcessed(Uuid),

    #[error("Invalid amendment action: {0}")]
    InvalidAction(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Cannot apply change: {0}")]
    CannotApply(String),
}

/// A proposal to amend an existing schema contract.
///
/// When the system detects data that doesn't conform to the contract,
/// it creates an amendment proposal for user review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaAmendmentProposal {
    /// Unique identifier for this amendment proposal
    pub amendment_id: Uuid,

    /// The contract being amended
    pub contract_id: Uuid,

    /// Why the amendment is needed
    pub reason: AmendmentReason,

    /// Current schema (before amendment)
    pub current_schema: LockedSchema,

    /// Proposed schema (after amendment)
    pub proposed_schema: LockedSchema,

    /// Detailed list of changes
    pub changes: Vec<SchemaChange>,

    /// Number of files affected by this issue
    pub affected_files: usize,

    /// Sample values that caused the issue (for user review)
    pub sample_values: Vec<SampleValue>,

    /// When this proposal was created
    pub created_at: DateTime<Utc>,

    /// Current status of the proposal
    pub status: AmendmentStatus,

    /// Optional notes from the system
    pub system_notes: Option<String>,
}

impl SchemaAmendmentProposal {
    /// Create a new amendment proposal
    pub fn new(
        contract_id: Uuid,
        reason: AmendmentReason,
        current_schema: LockedSchema,
        proposed_schema: LockedSchema,
    ) -> Self {
        let changes = compute_schema_changes(&current_schema, &proposed_schema);

        Self {
            amendment_id: Uuid::new_v4(),
            contract_id,
            reason,
            current_schema,
            proposed_schema,
            changes,
            affected_files: 0,
            sample_values: Vec::new(),
            created_at: Utc::now(),
            status: AmendmentStatus::Pending,
            system_notes: None,
        }
    }

    /// Set the number of affected files
    pub fn with_affected_files(mut self, count: usize) -> Self {
        self.affected_files = count;
        self
    }

    /// Add sample values that caused the issue
    pub fn with_sample_values(mut self, samples: Vec<SampleValue>) -> Self {
        self.sample_values = samples;
        self
    }

    /// Add system notes
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.system_notes = Some(notes.into());
        self
    }

    /// Check if this proposal is still pending
    pub fn is_pending(&self) -> bool {
        matches!(self.status, AmendmentStatus::Pending)
    }
}

/// Why an amendment is being proposed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AmendmentReason {
    /// A new schema variant was detected (new file type)
    NewSchemaVariant {
        /// Description of the new variant
        variant_description: String,
        /// Number of files matching this variant
        file_count: usize,
        /// Optional pattern that identifies these files
        file_pattern: Option<String>,
    },

    /// A column has values that don't match the expected type
    TypeMismatch {
        /// Column name
        column: String,
        /// Expected type from contract
        expected_type: DataType,
        /// Sample actual values that don't match
        actual_values: Vec<String>,
    },

    /// A non-nullable column now has null values
    NullabilityChange {
        /// Column name
        column: String,
        /// Percentage of rows with null values
        null_percentage: f32,
    },

    /// New columns appeared in the data
    NewColumns {
        /// Names of new columns
        column_names: Vec<String>,
        /// Number of files with new columns
        file_count: usize,
    },

    /// Columns are missing from the data
    MissingColumns {
        /// Names of missing columns
        column_names: Vec<String>,
        /// Number of files missing these columns
        file_count: usize,
    },

    /// Column order changed
    ColumnOrderChanged {
        /// Expected order
        expected_order: Vec<String>,
        /// Actual order found
        actual_order: Vec<String>,
    },

    /// Format string doesn't match values
    FormatMismatch {
        /// Column name
        column: String,
        /// Expected format
        expected_format: String,
        /// Sample values that don't match
        actual_values: Vec<String>,
    },

    /// User requested manual amendment
    UserRequested {
        /// User who requested
        requested_by: String,
        /// Reason for request
        reason: String,
    },
}

impl std::fmt::Display for AmendmentReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AmendmentReason::NewSchemaVariant { variant_description, file_count, .. } => {
                write!(f, "New schema variant detected: {} ({} files)", variant_description, file_count)
            }
            AmendmentReason::TypeMismatch { column, expected_type, .. } => {
                write!(f, "Type mismatch in column '{}' (expected {})", column, expected_type)
            }
            AmendmentReason::NullabilityChange { column, null_percentage } => {
                write!(f, "Column '{}' has {:.1}% null values", column, null_percentage)
            }
            AmendmentReason::NewColumns { column_names, .. } => {
                write!(f, "New columns detected: {}", column_names.join(", "))
            }
            AmendmentReason::MissingColumns { column_names, .. } => {
                write!(f, "Missing columns: {}", column_names.join(", "))
            }
            AmendmentReason::ColumnOrderChanged { .. } => {
                write!(f, "Column order changed")
            }
            AmendmentReason::FormatMismatch { column, expected_format, .. } => {
                write!(f, "Format mismatch in column '{}' (expected '{}')", column, expected_format)
            }
            AmendmentReason::UserRequested { requested_by, reason } => {
                write!(f, "User '{}' requested: {}", requested_by, reason)
            }
        }
    }
}

/// A specific change to the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SchemaChange {
    /// Add a new column
    AddColumn {
        column: LockedColumn,
        /// Position to insert (None = append)
        position: Option<usize>,
    },

    /// Remove an existing column
    RemoveColumn {
        column_name: String,
    },

    /// Change a column's data type
    ChangeType {
        column_name: String,
        from: DataType,
        to: DataType,
    },

    /// Change a column's nullability
    ChangeNullability {
        column_name: String,
        nullable: bool,
    },

    /// Add or change a default value
    AddDefaultValue {
        column_name: String,
        default: String,
    },

    /// Remove a default value
    RemoveDefaultValue {
        column_name: String,
    },

    /// Change a column's format string
    ChangeFormat {
        column_name: String,
        from: Option<String>,
        to: Option<String>,
    },

    /// Rename a column
    RenameColumn {
        from: String,
        to: String,
    },

    /// Reorder columns
    ReorderColumns {
        new_order: Vec<String>,
    },
}

impl std::fmt::Display for SchemaChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaChange::AddColumn { column, .. } => {
                write!(f, "Add column '{}' ({})", column.name, column.data_type)
            }
            SchemaChange::RemoveColumn { column_name } => {
                write!(f, "Remove column '{}'", column_name)
            }
            SchemaChange::ChangeType { column_name, from, to } => {
                write!(f, "Change '{}' type: {} -> {}", column_name, from, to)
            }
            SchemaChange::ChangeNullability { column_name, nullable } => {
                write!(f, "Change '{}' nullability: {}", column_name, if *nullable { "nullable" } else { "required" })
            }
            SchemaChange::AddDefaultValue { column_name, default } => {
                write!(f, "Add default '{}' to '{}'", default, column_name)
            }
            SchemaChange::RemoveDefaultValue { column_name } => {
                write!(f, "Remove default from '{}'", column_name)
            }
            SchemaChange::ChangeFormat { column_name, from, to } => {
                write!(f, "Change '{}' format: {:?} -> {:?}", column_name, from, to)
            }
            SchemaChange::RenameColumn { from, to } => {
                write!(f, "Rename column '{}' -> '{}'", from, to)
            }
            SchemaChange::ReorderColumns { .. } => {
                write!(f, "Reorder columns")
            }
        }
    }
}

/// Sample value that caused a schema issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleValue {
    /// The file where this value was found
    pub file_path: String,

    /// Row number (if applicable)
    pub row: Option<usize>,

    /// Column name
    pub column: String,

    /// The actual value
    pub value: String,

    /// Why this value is problematic
    pub issue: String,
}

impl SampleValue {
    /// Create a new sample value
    pub fn new(
        file_path: impl Into<String>,
        column: impl Into<String>,
        value: impl Into<String>,
        issue: impl Into<String>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            row: None,
            column: column.into(),
            value: value.into(),
            issue: issue.into(),
        }
    }

    /// Set the row number
    pub fn with_row(mut self, row: usize) -> Self {
        self.row = Some(row);
        self
    }
}

/// Current status of an amendment proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmendmentStatus {
    /// Waiting for user action
    Pending,

    /// User approved the amendment
    Approved,

    /// User rejected the amendment
    Rejected,

    /// User chose to create a separate schema
    SeparatedSchema,

    /// User chose to exclude affected files
    FilesExcluded,

    /// Amendment was superseded by another
    Superseded,
}

/// Action to take on an amendment proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AmendmentAction {
    /// Approve the amendment as proposed
    ApproveAsProposed,

    /// Approve with user modifications
    ApproveWithModifications {
        /// Modified changes (replacing the proposed ones)
        changes: Vec<SchemaChange>,
    },

    /// Reject the amendment (keep current schema, fail non-conforming data)
    Reject {
        /// Reason for rejection
        reason: String,
    },

    /// Create a separate schema for the new data pattern
    CreateSeparateSchema {
        /// Name for the new schema
        name: String,
        /// Description
        description: Option<String>,
    },

    /// Exclude the affected files from processing
    ExcludeAffectedFiles,

    /// Defer decision (keep proposal pending)
    Defer {
        /// Notes about why deferring
        notes: String,
    },
}

/// Result of applying an amendment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendmentResult {
    /// The amended contract (if action resulted in contract change)
    pub contract: Option<SchemaContract>,

    /// New contract created (if action was CreateSeparateSchema)
    pub new_contract: Option<SchemaContract>,

    /// Files excluded (if action was ExcludeAffectedFiles)
    pub excluded_files: Vec<String>,

    /// Final status of the amendment
    pub status: AmendmentStatus,

    /// Timestamp of the action
    pub processed_at: DateTime<Utc>,

    /// User who processed the amendment
    pub processed_by: String,
}

/// Propose an amendment to an existing contract.
///
/// This creates an amendment proposal based on detected issues.
pub fn propose_amendment(
    _storage: &SchemaStorage,
    contract: &SchemaContract,
    reason: AmendmentReason,
    proposed_schema: LockedSchema,
) -> Result<SchemaAmendmentProposal, AmendmentError> {
    // Get the current schema from the contract
    let current_schema = contract.schemas.first()
        .ok_or_else(|| AmendmentError::Validation("Contract has no schemas".into()))?
        .clone();

    // Create the proposal
    let proposal = SchemaAmendmentProposal::new(
        contract.contract_id,
        reason,
        current_schema,
        proposed_schema,
    );

    Ok(proposal)
}

/// Propose an amendment for a type mismatch.
pub fn propose_type_mismatch_amendment(
    contract: &SchemaContract,
    column: &str,
    expected_type: DataType,
    actual_values: Vec<String>,
    proposed_type: DataType,
) -> Result<SchemaAmendmentProposal, AmendmentError> {
    let current_schema = contract.schemas.first()
        .ok_or_else(|| AmendmentError::Validation("Contract has no schemas".into()))?;

    // Create proposed schema with the type change
    let mut new_columns = current_schema.columns.clone();
    for col in &mut new_columns {
        if col.name == column {
            col.data_type = proposed_type;
            break;
        }
    }

    let proposed_schema = LockedSchema::new(&current_schema.name, new_columns);

    let reason = AmendmentReason::TypeMismatch {
        column: column.to_string(),
        expected_type,
        actual_values,
    };

    propose_amendment_internal(contract, reason, current_schema.clone(), proposed_schema)
}

/// Propose an amendment for nullability change.
pub fn propose_nullability_amendment(
    contract: &SchemaContract,
    column: &str,
    null_percentage: f32,
) -> Result<SchemaAmendmentProposal, AmendmentError> {
    let current_schema = contract.schemas.first()
        .ok_or_else(|| AmendmentError::Validation("Contract has no schemas".into()))?;

    // Create proposed schema with nullable column
    let mut new_columns = current_schema.columns.clone();
    for col in &mut new_columns {
        if col.name == column {
            col.nullable = true;
            break;
        }
    }

    let proposed_schema = LockedSchema::new(&current_schema.name, new_columns);

    let reason = AmendmentReason::NullabilityChange {
        column: column.to_string(),
        null_percentage,
    };

    propose_amendment_internal(contract, reason, current_schema.clone(), proposed_schema)
}

/// Propose an amendment for new columns.
pub fn propose_new_columns_amendment(
    contract: &SchemaContract,
    new_columns: Vec<LockedColumn>,
    file_count: usize,
) -> Result<SchemaAmendmentProposal, AmendmentError> {
    let current_schema = contract.schemas.first()
        .ok_or_else(|| AmendmentError::Validation("Contract has no schemas".into()))?;

    // Create proposed schema with new columns appended
    let mut all_columns = current_schema.columns.clone();
    let column_names: Vec<String> = new_columns.iter().map(|c| c.name.clone()).collect();
    all_columns.extend(new_columns);

    let proposed_schema = LockedSchema::new(&current_schema.name, all_columns);

    let reason = AmendmentReason::NewColumns {
        column_names,
        file_count,
    };

    propose_amendment_internal(contract, reason, current_schema.clone(), proposed_schema)
}

/// Internal helper to create amendment proposal.
fn propose_amendment_internal(
    contract: &SchemaContract,
    reason: AmendmentReason,
    current_schema: LockedSchema,
    proposed_schema: LockedSchema,
) -> Result<SchemaAmendmentProposal, AmendmentError> {
    Ok(SchemaAmendmentProposal::new(
        contract.contract_id,
        reason,
        current_schema,
        proposed_schema,
    ))
}

/// Approve an amendment and update the contract.
pub async fn approve_amendment(
    storage: &SchemaStorage,
    proposal: &SchemaAmendmentProposal,
    action: AmendmentAction,
    processed_by: impl Into<String>,
) -> Result<AmendmentResult, AmendmentError> {
    if !proposal.is_pending() {
        return Err(AmendmentError::AlreadyProcessed(proposal.amendment_id));
    }

    let processed_by = processed_by.into();
    let processed_at = Utc::now();

    match action {
        AmendmentAction::ApproveAsProposed => {
            // Get current contract
            let current_contract = storage.get_contract(&proposal.contract_id).await?
                .ok_or_else(|| AmendmentError::ContractNotFound(proposal.contract_id.to_string()))?;

            // Create new version with proposed schema
            let mut new_schemas = current_contract.schemas.clone();
            if !new_schemas.is_empty() {
                new_schemas[0] = proposal.proposed_schema.clone();
            }

            let mut new_contract = SchemaContract::with_schemas(
                &current_contract.scope_id,
                new_schemas,
                &processed_by,
            );
            new_contract.version = current_contract.version + 1;

            storage.save_contract(&new_contract).await?;

            Ok(AmendmentResult {
                contract: Some(new_contract),
                new_contract: None,
                excluded_files: Vec::new(),
                status: AmendmentStatus::Approved,
                processed_at,
                processed_by,
            })
        }

        AmendmentAction::ApproveWithModifications { changes } => {
            // Apply custom changes
            let current_contract = storage.get_contract(&proposal.contract_id).await?
                .ok_or_else(|| AmendmentError::ContractNotFound(proposal.contract_id.to_string()))?;

            let current_schema = current_contract.schemas.first()
                .ok_or_else(|| AmendmentError::Validation("Contract has no schemas".into()))?;

            let modified_schema = apply_schema_changes(current_schema, &changes)?;

            let mut new_schemas = current_contract.schemas.clone();
            if !new_schemas.is_empty() {
                new_schemas[0] = modified_schema;
            }

            let mut new_contract = SchemaContract::with_schemas(
                &current_contract.scope_id,
                new_schemas,
                &processed_by,
            );
            new_contract.version = current_contract.version + 1;

            storage.save_contract(&new_contract).await?;

            Ok(AmendmentResult {
                contract: Some(new_contract),
                new_contract: None,
                excluded_files: Vec::new(),
                status: AmendmentStatus::Approved,
                processed_at,
                processed_by,
            })
        }

        AmendmentAction::Reject { reason: _ } => {
            Ok(AmendmentResult {
                contract: None,
                new_contract: None,
                excluded_files: Vec::new(),
                status: AmendmentStatus::Rejected,
                processed_at,
                processed_by,
            })
        }

        AmendmentAction::CreateSeparateSchema { name, description } => {
            // Create a new contract for the new schema variant
            let proposed = &proposal.proposed_schema;
            let source_pattern = proposed.source_pattern.clone();
            let mut new_schema = LockedSchema::new(&name, proposed.columns.clone());
            if let Some(pattern) = source_pattern {
                new_schema = new_schema.with_source_pattern(pattern);
            }

            let mut new_contract = SchemaContract::new(
                format!("{}_{}", proposal.contract_id, name),
                new_schema,
                &processed_by,
            );
            if let Some(desc) = description {
                new_contract = new_contract.with_description(desc);
            }

            storage.save_contract(&new_contract).await?;

            Ok(AmendmentResult {
                contract: None,
                new_contract: Some(new_contract),
                excluded_files: Vec::new(),
                status: AmendmentStatus::SeparatedSchema,
                processed_at,
                processed_by,
            })
        }

        AmendmentAction::ExcludeAffectedFiles => {
            // In a real implementation, this would record the excluded files
            let excluded = proposal.sample_values.iter()
                .map(|s| s.file_path.clone())
                .collect::<Vec<_>>();

            Ok(AmendmentResult {
                contract: None,
                new_contract: None,
                excluded_files: excluded,
                status: AmendmentStatus::FilesExcluded,
                processed_at,
                processed_by,
            })
        }

        AmendmentAction::Defer { notes: _ } => {
            // Keep proposal pending - no changes
            Ok(AmendmentResult {
                contract: None,
                new_contract: None,
                excluded_files: Vec::new(),
                status: AmendmentStatus::Pending,
                processed_at,
                processed_by,
            })
        }
    }
}

/// Compute the changes between two schemas.
fn compute_schema_changes(from: &LockedSchema, to: &LockedSchema) -> Vec<SchemaChange> {
    let mut changes = Vec::new();

    let from_cols: std::collections::HashMap<&str, &LockedColumn> =
        from.columns.iter().map(|c| (c.name.as_str(), c)).collect();
    let to_cols: std::collections::HashMap<&str, &LockedColumn> =
        to.columns.iter().map(|c| (c.name.as_str(), c)).collect();

    // Find removed columns
    for name in from_cols.keys() {
        if !to_cols.contains_key(name) {
            changes.push(SchemaChange::RemoveColumn {
                column_name: (*name).to_string(),
            });
        }
    }

    // Find added columns
    for (name, col) in &to_cols {
        if !from_cols.contains_key(name) {
            changes.push(SchemaChange::AddColumn {
                column: (*col).clone(),
                position: None,
            });
        }
    }

    // Find changed columns
    for (name, to_col) in &to_cols {
        if let Some(from_col) = from_cols.get(name) {
            if from_col.data_type != to_col.data_type {
                changes.push(SchemaChange::ChangeType {
                    column_name: (*name).to_string(),
                    from: from_col.data_type.clone(),
                    to: to_col.data_type.clone(),
                });
            }

            if from_col.nullable != to_col.nullable {
                changes.push(SchemaChange::ChangeNullability {
                    column_name: (*name).to_string(),
                    nullable: to_col.nullable,
                });
            }

            if from_col.format != to_col.format {
                changes.push(SchemaChange::ChangeFormat {
                    column_name: (*name).to_string(),
                    from: from_col.format.clone(),
                    to: to_col.format.clone(),
                });
            }
        }
    }

    changes
}

/// Apply a list of schema changes to produce a new schema.
fn apply_schema_changes(
    schema: &LockedSchema,
    changes: &[SchemaChange],
) -> Result<LockedSchema, AmendmentError> {
    let mut columns = schema.columns.clone();

    for change in changes {
        match change {
            SchemaChange::AddColumn { column, position } => {
                match position {
                    Some(pos) if *pos <= columns.len() => {
                        columns.insert(*pos, column.clone());
                    }
                    _ => {
                        columns.push(column.clone());
                    }
                }
            }

            SchemaChange::RemoveColumn { column_name } => {
                columns.retain(|c| c.name != *column_name);
            }

            SchemaChange::ChangeType { column_name, to, .. } => {
                for col in &mut columns {
                    if col.name == *column_name {
                        col.data_type = to.clone();
                        break;
                    }
                }
            }

            SchemaChange::ChangeNullability { column_name, nullable } => {
                for col in &mut columns {
                    if col.name == *column_name {
                        col.nullable = *nullable;
                        break;
                    }
                }
            }

            SchemaChange::AddDefaultValue { .. } => {
                // Default values are handled at runtime, not stored in LockedColumn
                // This is a no-op for schema structure
            }

            SchemaChange::RemoveDefaultValue { .. } => {
                // Same as above - no-op for schema structure
            }

            SchemaChange::ChangeFormat { column_name, to, .. } => {
                for col in &mut columns {
                    if col.name == *column_name {
                        col.format = to.clone();
                        break;
                    }
                }
            }

            SchemaChange::RenameColumn { from, to } => {
                for col in &mut columns {
                    if col.name == *from {
                        col.name = to.clone();
                        break;
                    }
                }
            }

            SchemaChange::ReorderColumns { new_order } => {
                let mut reordered = Vec::with_capacity(columns.len());
                for name in new_order {
                    if let Some(col) = columns.iter().find(|c| c.name == *name) {
                        reordered.push(col.clone());
                    } else {
                        return Err(AmendmentError::CannotApply(
                            format!("Column '{}' not found for reordering", name)
                        ));
                    }
                }
                columns = reordered;
            }
        }
    }

    Ok(LockedSchema::new(&schema.name, columns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SchemaStorage;

    async fn create_test_storage() -> SchemaStorage {
        SchemaStorage::in_memory().await.unwrap()
    }

    async fn create_test_contract(storage: &SchemaStorage) -> SchemaContract {
        let schema = LockedSchema::new(
            "test_table",
            vec![
                LockedColumn::required("id", DataType::Int64),
                LockedColumn::required("name", DataType::String),
                LockedColumn::optional("value", DataType::Float64),
            ],
        );

        let contract = SchemaContract::new("test_scope", schema, "admin");
        storage.save_contract(&contract).await.unwrap();
        contract
    }

    #[test]
    fn test_create_amendment_proposal() {
        let current = LockedSchema::new(
            "test",
            vec![LockedColumn::required("id", DataType::Int64)],
        );

        let proposed = LockedSchema::new(
            "test",
            vec![
                LockedColumn::required("id", DataType::Int64),
                LockedColumn::optional("new_col", DataType::String),
            ],
        );

        let proposal = SchemaAmendmentProposal::new(
            Uuid::new_v4(),
            AmendmentReason::NewColumns {
                column_names: vec!["new_col".to_string()],
                file_count: 5,
            },
            current,
            proposed,
        )
        .with_affected_files(5);

        assert!(proposal.is_pending());
        assert_eq!(proposal.affected_files, 5);
        assert!(!proposal.changes.is_empty());
    }

    #[tokio::test]
    async fn test_propose_type_mismatch() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let proposal = propose_type_mismatch_amendment(
            &contract,
            "value",
            DataType::Float64,
            vec!["abc".to_string(), "def".to_string()],
            DataType::String,
        )
        .unwrap();

        assert!(matches!(proposal.reason, AmendmentReason::TypeMismatch { .. }));
        assert!(proposal.changes.iter().any(|c| matches!(c, SchemaChange::ChangeType { .. })));
    }

    #[tokio::test]
    async fn test_propose_nullability_change() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let proposal = propose_nullability_amendment(&contract, "name", 15.5).unwrap();

        assert!(matches!(proposal.reason, AmendmentReason::NullabilityChange { .. }));
        assert!(proposal.changes.iter().any(|c| matches!(c, SchemaChange::ChangeNullability { .. })));
    }

    #[tokio::test]
    async fn test_propose_new_columns() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let new_cols = vec![
            LockedColumn::optional("extra1", DataType::String),
            LockedColumn::optional("extra2", DataType::Int64),
        ];

        let proposal = propose_new_columns_amendment(&contract, new_cols, 10).unwrap();

        assert!(matches!(proposal.reason, AmendmentReason::NewColumns { .. }));
        let add_count = proposal.changes.iter()
            .filter(|c| matches!(c, SchemaChange::AddColumn { .. }))
            .count();
        assert_eq!(add_count, 2);
    }

    #[tokio::test]
    async fn test_approve_amendment_as_proposed() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let proposed = LockedSchema::new(
            "test_table",
            vec![
                LockedColumn::required("id", DataType::Int64),
                LockedColumn::optional("name", DataType::String), // Changed to nullable
                LockedColumn::optional("value", DataType::Float64),
            ],
        );

        let proposal = SchemaAmendmentProposal::new(
            contract.contract_id,
            AmendmentReason::NullabilityChange {
                column: "name".to_string(),
                null_percentage: 10.0,
            },
            contract.schemas[0].clone(),
            proposed,
        );

        let result = approve_amendment(
            &storage,
            &proposal,
            AmendmentAction::ApproveAsProposed,
            "reviewer",
        )
        .await
        .unwrap();

        assert_eq!(result.status, AmendmentStatus::Approved);
        assert!(result.contract.is_some());

        let updated = result.contract.unwrap();
        assert_eq!(updated.version, 2);
        assert!(updated.schemas[0].columns[1].nullable);
    }

    #[tokio::test]
    async fn test_approve_with_modifications() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let proposal = SchemaAmendmentProposal::new(
            contract.contract_id,
            AmendmentReason::UserRequested {
                requested_by: "user".to_string(),
                reason: "Need to add column".to_string(),
            },
            contract.schemas[0].clone(),
            contract.schemas[0].clone(), // Same schema, we'll modify via action
        );

        let result = approve_amendment(
            &storage,
            &proposal,
            AmendmentAction::ApproveWithModifications {
                changes: vec![
                    SchemaChange::AddColumn {
                        column: LockedColumn::optional("new_col", DataType::String),
                        position: None,
                    },
                ],
            },
            "reviewer",
        )
        .await
        .unwrap();

        assert_eq!(result.status, AmendmentStatus::Approved);
        let updated = result.contract.unwrap();
        assert_eq!(updated.schemas[0].columns.len(), 4);
        assert_eq!(updated.schemas[0].columns[3].name, "new_col");
    }

    #[tokio::test]
    async fn test_reject_amendment() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let proposal = SchemaAmendmentProposal::new(
            contract.contract_id,
            AmendmentReason::TypeMismatch {
                column: "id".to_string(),
                expected_type: DataType::Int64,
                actual_values: vec!["abc".to_string()],
            },
            contract.schemas[0].clone(),
            contract.schemas[0].clone(),
        );

        let result = approve_amendment(
            &storage,
            &proposal,
            AmendmentAction::Reject {
                reason: "Bad data, should be fixed at source".to_string(),
            },
            "reviewer",
        )
        .await
        .unwrap();

        assert_eq!(result.status, AmendmentStatus::Rejected);
        assert!(result.contract.is_none());
    }

    #[tokio::test]
    async fn test_create_separate_schema() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let proposed = LockedSchema::new(
            "new_variant",
            vec![
                LockedColumn::required("id", DataType::Int64),
                LockedColumn::required("different_field", DataType::String),
            ],
        );

        let proposal = SchemaAmendmentProposal::new(
            contract.contract_id,
            AmendmentReason::NewSchemaVariant {
                variant_description: "New file format".to_string(),
                file_count: 20,
                file_pattern: Some("*_v2.csv".to_string()),
            },
            contract.schemas[0].clone(),
            proposed,
        );

        let result = approve_amendment(
            &storage,
            &proposal,
            AmendmentAction::CreateSeparateSchema {
                name: "v2_data".to_string(),
                description: Some("Version 2 format files".to_string()),
            },
            "reviewer",
        )
        .await
        .unwrap();

        assert_eq!(result.status, AmendmentStatus::SeparatedSchema);
        assert!(result.new_contract.is_some());
        assert!(result.contract.is_none());

        let new_contract = result.new_contract.unwrap();
        assert_eq!(new_contract.schemas[0].name, "v2_data");
    }

    #[tokio::test]
    async fn test_exclude_affected_files() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let mut proposal = SchemaAmendmentProposal::new(
            contract.contract_id,
            AmendmentReason::TypeMismatch {
                column: "id".to_string(),
                expected_type: DataType::Int64,
                actual_values: vec!["bad".to_string()],
            },
            contract.schemas[0].clone(),
            contract.schemas[0].clone(),
        );

        proposal.sample_values = vec![
            SampleValue::new("bad1.csv", "id", "abc", "Not an integer"),
            SampleValue::new("bad2.csv", "id", "def", "Not an integer"),
        ];

        let result = approve_amendment(
            &storage,
            &proposal,
            AmendmentAction::ExcludeAffectedFiles,
            "reviewer",
        )
        .await
        .unwrap();

        assert_eq!(result.status, AmendmentStatus::FilesExcluded);
        assert_eq!(result.excluded_files.len(), 2);
        assert!(result.excluded_files.contains(&"bad1.csv".to_string()));
    }

    #[tokio::test]
    async fn test_defer_amendment() {
        let storage = create_test_storage().await;
        let contract = create_test_contract(&storage).await;

        let proposal = SchemaAmendmentProposal::new(
            contract.contract_id,
            AmendmentReason::UserRequested {
                requested_by: "user".to_string(),
                reason: "test".to_string(),
            },
            contract.schemas[0].clone(),
            contract.schemas[0].clone(),
        );

        let result = approve_amendment(
            &storage,
            &proposal,
            AmendmentAction::Defer {
                notes: "Need to investigate further".to_string(),
            },
            "reviewer",
        )
        .await
        .unwrap();

        assert_eq!(result.status, AmendmentStatus::Pending);
    }

    #[test]
    fn test_compute_schema_changes() {
        let from = LockedSchema::new(
            "test",
            vec![
                LockedColumn::required("a", DataType::Int64),
                LockedColumn::required("b", DataType::String),
            ],
        );

        let to = LockedSchema::new(
            "test",
            vec![
                LockedColumn::optional("a", DataType::Float64), // Type + nullable changed
                LockedColumn::required("c", DataType::String),  // b removed, c added
            ],
        );

        let changes = compute_schema_changes(&from, &to);

        assert!(changes.iter().any(|c| matches!(c, SchemaChange::RemoveColumn { column_name } if column_name == "b")));
        assert!(changes.iter().any(|c| matches!(c, SchemaChange::AddColumn { column, .. } if column.name == "c")));
        assert!(changes.iter().any(|c| matches!(c, SchemaChange::ChangeType { column_name, from: DataType::Int64, to: DataType::Float64 } if column_name == "a")));
        assert!(changes.iter().any(|c| matches!(c, SchemaChange::ChangeNullability { column_name, nullable: true } if column_name == "a")));
    }

    #[test]
    fn test_apply_schema_changes() {
        let schema = LockedSchema::new(
            "test",
            vec![
                LockedColumn::required("a", DataType::Int64),
                LockedColumn::required("b", DataType::String),
            ],
        );

        let changes = vec![
            SchemaChange::AddColumn {
                column: LockedColumn::optional("c", DataType::Float64),
                position: None,
            },
            SchemaChange::ChangeNullability {
                column_name: "a".to_string(),
                nullable: true,
            },
            SchemaChange::RenameColumn {
                from: "b".to_string(),
                to: "renamed_b".to_string(),
            },
        ];

        let result = apply_schema_changes(&schema, &changes).unwrap();

        assert_eq!(result.columns.len(), 3);
        assert!(result.columns[0].nullable);
        assert_eq!(result.columns[1].name, "renamed_b");
        assert_eq!(result.columns[2].name, "c");
    }

    #[test]
    fn test_amendment_reason_display() {
        let reason = AmendmentReason::TypeMismatch {
            column: "amount".to_string(),
            expected_type: DataType::Float64,
            actual_values: vec!["N/A".to_string()],
        };

        let display = reason.to_string();
        assert!(display.contains("amount"));
        assert!(display.contains("Float64"));
    }

    #[test]
    fn test_schema_change_display() {
        let change = SchemaChange::ChangeType {
            column_name: "value".to_string(),
            from: DataType::Int64,
            to: DataType::String,
        };

        let display = change.to_string();
        assert!(display.contains("value"));
        assert!(display.contains("Int64"));
        assert!(display.contains("String"));
    }

    #[test]
    fn test_sample_value() {
        let sample = SampleValue::new("data.csv", "amount", "N/A", "Expected numeric")
            .with_row(42);

        assert_eq!(sample.file_path, "data.csv");
        assert_eq!(sample.row, Some(42));
        assert_eq!(sample.value, "N/A");
    }
}
