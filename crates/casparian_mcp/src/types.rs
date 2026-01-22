//! Shared Type Definitions for MCP Tools
//!
//! These definitions are referenced throughout the tool specifications.
//! All types are designed for JSON serialization in the MCP protocol.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// PluginRef - Parser/Plugin Identity
// ============================================================================

/// Standardized parser/plugin identity for future-proofing.
///
/// Can reference either:
/// - A registered plugin by ID (with optional version)
/// - A local file path (dev mode only)
///
/// # Examples
///
/// ```json
/// { "plugin": "evtx_native", "version": "0.1.0" }
/// { "plugin": "fix_parser" }
/// { "path": "./parsers/my_parser.py" }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PluginRef {
    /// Reference to a registered plugin
    Registered {
        /// Plugin ID
        plugin: String,
        /// Optional semver version constraint
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
    /// Reference to a local file (dev mode only)
    Path {
        /// Local file path
        path: PathBuf,
    },
}

impl PluginRef {
    /// Create a reference to a registered plugin
    pub fn registered(plugin: impl Into<String>) -> Self {
        Self::Registered {
            plugin: plugin.into(),
            version: None,
        }
    }

    /// Create a reference to a registered plugin with version
    pub fn registered_version(plugin: impl Into<String>, version: impl Into<String>) -> Self {
        Self::Registered {
            plugin: plugin.into(),
            version: Some(version.into()),
        }
    }

    /// Create a reference to a local file
    pub fn path(path: impl Into<PathBuf>) -> Self {
        Self::Path { path: path.into() }
    }

    /// Get the plugin name/identifier for display
    pub fn display_name(&self) -> String {
        match self {
            Self::Registered { plugin, version } => match version {
                Some(v) => format!("{}@{}", plugin, v),
                None => plugin.clone(),
            },
            Self::Path { path } => path.display().to_string(),
        }
    }
}

// ============================================================================
// DataType - Schema Column Types
// ============================================================================

/// Data type for schema columns.
///
/// Supports both simple types (string) and complex types (object with params).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum DataType {
    /// Simple type: "string", "int64", "float64", "boolean", "date", "binary"
    Simple(SimpleDataType),
    /// Complex type with parameters
    Complex(ComplexDataType),
}

/// Simple data types (represented as strings in JSON)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SimpleDataType {
    String,
    Int64,
    Float64,
    Boolean,
    Date,
    Binary,
}

/// Complex data types with parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComplexDataType {
    /// Fixed-point decimal with precision and scale
    Decimal {
        precision: u8,
        scale: u8,
    },
    /// Timestamp with timezone
    TimestampTz {
        #[serde(skip_serializing_if = "Option::is_none")]
        timezone: Option<String>,
    },
}

// ============================================================================
// SchemaDefinition - Per-Output Schema
// ============================================================================

/// Schema definition for a single output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDefinition {
    /// Output name this schema applies to
    pub output_name: String,

    /// Validation mode
    #[serde(default)]
    pub mode: SchemaMode,

    /// Column definitions
    pub columns: Vec<ColumnDefinition>,
}

/// Schema validation mode
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaMode {
    /// All columns must match exactly
    #[default]
    Strict,
    /// Extra columns in output are allowed
    AllowExtra,
    /// Optional columns can be missing
    AllowMissingOptional,
}

/// Column definition within a schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// Column name
    pub name: String,

    /// Data type
    #[serde(rename = "type")]
    pub data_type: DataType,

    /// Whether null values are allowed
    #[serde(default = "default_true")]
    pub nullable: bool,

    /// Optional format string (for dates/timestamps)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Map of output_name -> SchemaDefinition for multi-output parsers
pub type SchemasMap = HashMap<String, SchemaDefinition>;

// ============================================================================
// RedactionPolicy - Controls Sensitive Data Exposure
// ============================================================================

/// Controls sensitive data exposure in tool outputs.
///
/// DFIR data often contains sensitive content; redaction is enabled by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPolicy {
    /// Redaction mode
    #[serde(default)]
    pub mode: RedactionMode,

    /// Maximum number of sample values to include
    #[serde(default = "default_max_sample_count")]
    pub max_sample_count: usize,

    /// Maximum length of sample values (characters)
    #[serde(default = "default_max_value_length")]
    pub max_value_length: usize,

    /// Length of hash prefix (for hash mode)
    #[serde(default = "default_hash_prefix_length")]
    pub hash_prefix_length: usize,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            mode: RedactionMode::default(),
            max_sample_count: default_max_sample_count(),
            max_value_length: default_max_value_length(),
            hash_prefix_length: default_hash_prefix_length(),
        }
    }
}

fn default_max_sample_count() -> usize {
    5
}
fn default_max_value_length() -> usize {
    100
}
fn default_hash_prefix_length() -> usize {
    8
}

/// Redaction mode
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RedactionMode {
    /// No redaction - raw values (requires explicit opt-in)
    None,
    /// Truncate values to max_value_length
    Truncate,
    /// Hash values (SHA256 prefix) - default for security
    #[default]
    Hash,
}

impl RedactionPolicy {
    /// Apply redaction to a string value
    pub fn redact(&self, value: &str) -> String {
        match self.mode {
            RedactionMode::None => value.to_string(),
            RedactionMode::Truncate => {
                if value.len() <= self.max_value_length {
                    value.to_string()
                } else {
                    format!("{}...", &value[..self.max_value_length])
                }
            }
            RedactionMode::Hash => {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(value.as_bytes());
                let hash = hasher.finalize();
                let hex = hex::encode(hash);
                format!("[hash:{}]", &hex[..self.hash_prefix_length])
            }
        }
    }

    /// Redact a list of sample values
    pub fn redact_samples(&self, samples: &[String]) -> Vec<String> {
        samples
            .iter()
            .take(self.max_sample_count)
            .map(|s| self.redact(s))
            .collect()
    }
}

// ============================================================================
// ViolationContext - Machine-Readable Error Context for AI Learning
// ============================================================================

/// Machine-readable error context for AI learning during backtest.
///
/// Provides structured information about schema violations that AI agents
/// can use to improve parser/schema definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationContext {
    /// Output name where violation occurred
    pub output_name: String,

    /// Column name (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,

    /// Type of violation
    pub violation_type: ViolationType,

    /// Number of occurrences
    pub count: u64,

    /// Percentage of total rows affected
    pub pct_of_rows: f64,

    /// Sample values (redacted per policy)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<String>,

    /// Top-K value distribution (keys redacted per policy)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub value_distribution: HashMap<String, u64>,

    /// Suggested fix (if determinable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<SuggestedFix>,
}

/// Types of schema violations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    /// Value doesn't match expected type
    TypeMismatch,
    /// Null value in non-nullable column
    NullNotAllowed,
    /// Value doesn't match expected format (e.g., date format)
    FormatMismatch,
    /// Column name doesn't match schema
    ColumnNameMismatch,
    /// Wrong number of columns
    ColumnCountMismatch,
}

/// Suggested fix for a violation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SuggestedFix {
    /// Change column type
    ChangeType {
        from: DataType,
        to: DataType,
    },
    /// Make column nullable
    MakeNullable,
    /// Change format string
    ChangeFormat {
        suggested: String,
    },
    /// Add missing column
    AddColumn {
        name: String,
        #[serde(rename = "type")]
        data_type: DataType,
    },
    /// Remove extra column
    RemoveColumn {
        name: String,
    },
}

// ============================================================================
// ApprovalSummary - Human-Readable Approval Context
// ============================================================================

/// Summary information for approval requests.
///
/// Displayed to humans when reviewing pending approvals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalSummary {
    /// Human-readable description
    pub description: String,

    /// Number of files to be processed
    pub file_count: usize,

    /// Estimated rows to be produced (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_rows: Option<u64>,

    /// Target path for output
    pub target_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_ref_registered() {
        let pr = PluginRef::registered("evtx_native");
        let json = serde_json::to_string(&pr).unwrap();
        assert!(json.contains("evtx_native"));
    }

    #[test]
    fn test_plugin_ref_path() {
        let pr = PluginRef::path("./parsers/test.py");
        let json = serde_json::to_string(&pr).unwrap();
        assert!(json.contains("parsers/test.py"));
    }

    #[test]
    fn test_redaction_hash() {
        let policy = RedactionPolicy::default();
        let redacted = policy.redact("sensitive-data-12345");
        assert!(redacted.starts_with("[hash:"));
        assert_eq!(redacted.len(), 8 + 8); // "[hash:" + 8 chars + "]"
    }

    #[test]
    fn test_redaction_truncate() {
        let policy = RedactionPolicy {
            mode: RedactionMode::Truncate,
            max_value_length: 10,
            ..Default::default()
        };
        let redacted = policy.redact("this is a very long string");
        assert_eq!(redacted, "this is a ...");
    }

    #[test]
    fn test_redaction_none() {
        let policy = RedactionPolicy {
            mode: RedactionMode::None,
            ..Default::default()
        };
        let redacted = policy.redact("sensitive");
        assert_eq!(redacted, "sensitive");
    }

    #[test]
    fn test_schema_definition_serialization() {
        let schema = SchemaDefinition {
            output_name: "events".to_string(),
            mode: SchemaMode::Strict,
            columns: vec![ColumnDefinition {
                name: "id".to_string(),
                data_type: DataType::Simple(SimpleDataType::Int64),
                nullable: false,
                format: None,
            }],
        };

        let json = serde_json::to_string_pretty(&schema).unwrap();
        assert!(json.contains("events"));
        assert!(json.contains("int64"));
    }
}
