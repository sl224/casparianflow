//! Schema Contract Types
//!
//! Schema = Intent. Once approved, schema is a CONTRACT.
//! Parser must conform. Violations are failures, not fallbacks.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A schema contract - the binding agreement between user intent and parser output.
///
/// Once a contract is created (user clicks "Approve Schema"), the parser
/// MUST produce data conforming to these schemas. Any deviation is a failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaContract {
    /// Unique identifier for this contract
    pub contract_id: Uuid,

    /// The scope this contract applies to (e.g., parser_id, pipeline_id, tag)
    pub scope_id: String,

    /// Human-readable description of the scope (e.g., "CSV files matching RFC_DB")
    pub scope_description: Option<String>,

    /// Advisory hash of parser logic/config used when approving
    pub logic_hash: Option<String>,

    /// When this contract was approved by the user
    pub approved_at: DateTime<Utc>,

    /// Who approved this contract (user_id or "system")
    pub approved_by: String,

    /// The locked schemas in this contract
    pub schemas: Vec<LockedSchema>,

    /// Contract version (incremented on re-approval)
    pub version: u32,
}

impl SchemaContract {
    /// Create a new schema contract with a single schema
    pub fn new(scope_id: impl Into<String>, schema: LockedSchema, approved_by: impl Into<String>) -> Self {
        Self {
            contract_id: Uuid::new_v4(),
            scope_id: scope_id.into(),
            scope_description: None,
            logic_hash: None,
            approved_at: Utc::now(),
            approved_by: approved_by.into(),
            schemas: vec![schema],
            version: 1,
        }
    }

    /// Create a contract with multiple schemas
    pub fn with_schemas(
        scope_id: impl Into<String>,
        schemas: Vec<LockedSchema>,
        approved_by: impl Into<String>,
    ) -> Self {
        Self {
            contract_id: Uuid::new_v4(),
            scope_id: scope_id.into(),
            scope_description: None,
            logic_hash: None,
            approved_at: Utc::now(),
            approved_by: approved_by.into(),
            schemas,
            version: 1,
        }
    }

    /// Add description to the contract
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.scope_description = Some(description.into());
        self
    }

    /// Attach an advisory logic hash
    pub fn with_logic_hash(mut self, logic_hash: Option<String>) -> Self {
        self.logic_hash = logic_hash;
        self
    }
}

/// A locked schema - immutable definition of expected data structure.
///
/// Once locked, this schema CANNOT change without creating a new contract version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedSchema {
    /// Unique identifier for this schema
    pub schema_id: Uuid,

    /// Human-readable name (e.g., "transactions", "users")
    pub name: String,

    /// The columns in this schema, in order
    pub columns: Vec<LockedColumn>,

    /// Pattern that identifies files this schema applies to (e.g., "*.csv", "RFC_DB_*")
    pub source_pattern: Option<String>,

    /// SHA-256 hash of the schema definition (for quick comparison)
    pub content_hash: String,
}

impl LockedSchema {
    /// Create a new locked schema
    pub fn new(name: impl Into<String>, columns: Vec<LockedColumn>) -> Self {
        let name = name.into();
        let content_hash = Self::compute_hash(&name, &columns);
        Self {
            schema_id: Uuid::new_v4(),
            name,
            columns,
            source_pattern: None,
            content_hash,
        }
    }

    /// Set the source pattern
    pub fn with_source_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.source_pattern = Some(pattern.into());
        self
    }

    /// Compute a content hash for the schema
    fn compute_hash(name: &str, columns: &[LockedColumn]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        for col in columns {
            col.name.hash(&mut hasher);
            format!("{:?}", col.data_type).hash(&mut hasher);
            col.nullable.hash(&mut hasher);
            col.format.hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }

    /// Check if data columns match this schema
    pub fn validate_columns(&self, column_names: &[&str]) -> Result<(), SchemaViolation> {
        if column_names.len() != self.columns.len() {
            return Err(SchemaViolation {
                file_path: None,
                row: None,
                column: None,
                expected: format!("{} columns", self.columns.len()),
                got: format!("{} columns", column_names.len()),
                violation_type: ViolationType::ColumnCountMismatch,
            });
        }

        for (i, (expected, got)) in self.columns.iter().zip(column_names.iter()).enumerate() {
            if expected.name != *got {
                return Err(SchemaViolation {
                    file_path: None,
                    row: None,
                    column: Some(i),
                    expected: expected.name.clone(),
                    got: (*got).to_string(),
                    violation_type: ViolationType::ColumnNameMismatch,
                });
            }
        }

        Ok(())
    }
}

/// A locked column definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockedColumn {
    /// Column name (must match exactly)
    pub name: String,

    /// Expected data type
    pub data_type: DataType,

    /// Whether null/empty values are allowed
    pub nullable: bool,

    /// Optional format string (e.g., "%Y-%m-%d" for dates)
    pub format: Option<String>,

    /// Optional description for documentation
    pub description: Option<String>,
}

impl LockedColumn {
    /// Create a new required (non-nullable) column
    pub fn required(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable: false,
            format: None,
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
            description: None,
        }
    }

    /// Set format string (for dates, timestamps, etc.)
    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Canonical data type used for schema contracts (shared across crates).
pub use casparian_protocol::DataType;

/// A schema contract violation - parser output doesn't match contract.
///
/// This represents a FAILURE, not a warning. The job should fail when
/// violations are detected (unless explicitly in "discovery mode").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaViolation {
    /// Path to the file being processed (if applicable)
    pub file_path: Option<String>,

    /// Row number where violation occurred (0-indexed, if applicable)
    pub row: Option<usize>,

    /// Column index where violation occurred (if applicable)
    pub column: Option<usize>,

    /// What was expected
    pub expected: String,

    /// What was actually received
    pub got: String,

    /// Type of violation
    pub violation_type: ViolationType,
}

impl SchemaViolation {
    /// Create a type mismatch violation
    pub fn type_mismatch(
        column: usize,
        expected: DataType,
        got: impl Into<String>,
    ) -> Self {
        Self {
            file_path: None,
            row: None,
            column: Some(column),
            expected: expected.to_string(),
            got: got.into(),
            violation_type: ViolationType::TypeMismatch,
        }
    }

    /// Create a null not allowed violation
    pub fn null_not_allowed(column: usize, column_name: impl Into<String>) -> Self {
        Self {
            file_path: None,
            row: None,
            column: Some(column),
            expected: "non-null value".to_string(),
            got: format!("null in column '{}'", column_name.into()),
            violation_type: ViolationType::NullNotAllowed,
        }
    }

    /// Create a format mismatch violation
    pub fn format_mismatch(
        column: usize,
        expected_format: impl Into<String>,
        got: impl Into<String>,
    ) -> Self {
        Self {
            file_path: None,
            row: None,
            column: Some(column),
            expected: expected_format.into(),
            got: got.into(),
            violation_type: ViolationType::FormatMismatch,
        }
    }

    /// Set the file path context
    pub fn with_file(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Set the row context
    pub fn with_row(mut self, row: usize) -> Self {
        self.row = Some(row);
        self
    }
}

impl std::fmt::Display for SchemaViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: expected '{}', got '{}'", self.violation_type, self.expected, self.got)?;
        if let Some(col) = self.column {
            write!(f, " (column {})", col)?;
        }
        if let Some(row) = self.row {
            write!(f, " at row {}", row)?;
        }
        if let Some(ref path) = self.file_path {
            write!(f, " in '{}'", path)?;
        }
        Ok(())
    }
}

impl std::error::Error for SchemaViolation {}

/// Types of schema violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    /// Value doesn't match expected type (e.g., "abc" for Int64)
    TypeMismatch,

    /// Null/empty value in non-nullable column
    NullNotAllowed,

    /// Value doesn't match expected format (e.g., wrong date format)
    FormatMismatch,

    /// Column name doesn't match schema
    ColumnNameMismatch,

    /// Number of columns doesn't match schema
    ColumnCountMismatch,

    /// Schema not found for the scope
    SchemaNotFound,
}

impl std::fmt::Display for ViolationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViolationType::TypeMismatch => write!(f, "Type mismatch"),
            ViolationType::NullNotAllowed => write!(f, "Null not allowed"),
            ViolationType::FormatMismatch => write!(f, "Format mismatch"),
            ViolationType::ColumnNameMismatch => write!(f, "Column name mismatch"),
            ViolationType::ColumnCountMismatch => write!(f, "Column count mismatch"),
            ViolationType::SchemaNotFound => write!(f, "Schema not found"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_schema() {
        let schema = LockedSchema::new(
            "transactions",
            vec![
                LockedColumn::required("id", DataType::Int64),
                LockedColumn::required("amount", DataType::Float64),
                LockedColumn::optional("description", DataType::String),
                LockedColumn::required("date", DataType::Date).with_format("%Y-%m-%d"),
            ],
        );

        assert_eq!(schema.name, "transactions");
        assert_eq!(schema.columns.len(), 4);
        assert!(!schema.content_hash.is_empty());
    }

    #[test]
    fn test_create_contract() {
        let schema = LockedSchema::new(
            "users",
            vec![
                LockedColumn::required("user_id", DataType::Int64),
                LockedColumn::required("email", DataType::String),
            ],
        );

        let contract = SchemaContract::new("parser_123", schema, "user_456")
            .with_description("User data from CRM export");

        assert_eq!(contract.scope_id, "parser_123");
        assert_eq!(contract.approved_by, "user_456");
        assert_eq!(contract.version, 1);
        assert_eq!(contract.schemas.len(), 1);
    }

    #[test]
    fn test_validate_columns() {
        let schema = LockedSchema::new(
            "data",
            vec![
                LockedColumn::required("a", DataType::String),
                LockedColumn::required("b", DataType::Int64),
            ],
        );

        // Valid columns
        assert!(schema.validate_columns(&["a", "b"]).is_ok());

        // Wrong column count
        let err = schema.validate_columns(&["a"]).unwrap_err();
        assert_eq!(err.violation_type, ViolationType::ColumnCountMismatch);

        // Wrong column name
        let err = schema.validate_columns(&["a", "c"]).unwrap_err();
        assert_eq!(err.violation_type, ViolationType::ColumnNameMismatch);
    }

    #[test]
    fn test_data_type_validation() {
        assert!(DataType::Int64.validate_string("123"));
        assert!(DataType::Int64.validate_string("-456"));
        assert!(!DataType::Int64.validate_string("12.5"));
        assert!(!DataType::Int64.validate_string("abc"));

        assert!(DataType::Float64.validate_string("12.5"));
        assert!(DataType::Float64.validate_string("-3.14"));
        assert!(!DataType::Float64.validate_string("abc"));

        assert!(DataType::Boolean.validate_string("true"));
        assert!(DataType::Boolean.validate_string("FALSE"));
        assert!(DataType::Boolean.validate_string("1"));
        assert!(!DataType::Boolean.validate_string("maybe"));

        assert!(DataType::Date.validate_string("2024-01-15"));
        assert!(DataType::Date.validate_string("01/15/2024"));
        assert!(!DataType::Date.validate_string("not a date"));
    }

    #[test]
    fn test_violation_display() {
        let v = SchemaViolation::type_mismatch(2, DataType::Int64, "abc")
            .with_file("data.csv")
            .with_row(42);

        let msg = v.to_string();
        assert!(msg.contains("TypeMismatch"));
        assert!(msg.contains("Int64"));
        assert!(msg.contains("abc"));
        assert!(msg.contains("column 2"));
        assert!(msg.contains("row 42"));
        assert!(msg.contains("data.csv"));
    }
}
