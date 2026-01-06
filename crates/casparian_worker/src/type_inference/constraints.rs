//! Constraint types for type inference
//!
//! Constraints represent evidence about what a type CAN or CANNOT be.
//! The solver accumulates constraints and eliminates possibilities.

use super::DataType;

/// A constraint that eliminates or confirms type possibilities
#[derive(Debug, Clone)]
pub enum Constraint {
    /// This type is definitely eliminated for this column
    CannotBe {
        data_type: DataType,
        reason: EliminationReason,
    },
    /// This type is definitely required (rare - usually from schema hints)
    MustBe {
        data_type: DataType,
        reason: String,
    },
    /// A specific date format is eliminated
    DateFormatEliminated {
        format: String,
        reason: EliminationReason,
    },
    /// A specific time format is eliminated
    TimeFormatEliminated {
        format: String,
        reason: EliminationReason,
    },
}

/// Reason why a type or format was eliminated
#[derive(Debug, Clone)]
pub enum EliminationReason {
    /// Value contains characters that rule out this type
    InvalidCharacters { value: String, chars: String },
    /// Numeric value out of range for this interpretation
    OutOfRange {
        value: String,
        component: String,
        actual: i32,
        max: i32,
    },
    /// Date component impossible (e.g., month > 12)
    DateComponentInvalid {
        value: String,
        component: String,
        actual: i32,
    },
    /// Format pattern doesn't match
    PatternMismatch { value: String, expected: String },
    /// Parse failed with specific error
    ParseFailed { value: String, error: String },
    /// Contains decimal point (eliminates integer)
    ContainsDecimalPoint { value: String },
    /// Contains non-numeric prefix/suffix
    NonNumericAffixes { value: String, affix: String },
    /// Boolean interpretation failed
    NotBooleanValue { value: String },
    /// Value is empty or null
    EmptyValue,
    /// Custom reason
    Custom { value: String, reason: String },
}

impl EliminationReason {
    /// Get the value that caused this elimination (if any)
    pub fn value(&self) -> Option<&str> {
        match self {
            EliminationReason::InvalidCharacters { value, .. } => Some(value),
            EliminationReason::OutOfRange { value, .. } => Some(value),
            EliminationReason::DateComponentInvalid { value, .. } => Some(value),
            EliminationReason::PatternMismatch { value, .. } => Some(value),
            EliminationReason::ParseFailed { value, .. } => Some(value),
            EliminationReason::ContainsDecimalPoint { value } => Some(value),
            EliminationReason::NonNumericAffixes { value, .. } => Some(value),
            EliminationReason::NotBooleanValue { value } => Some(value),
            EliminationReason::EmptyValue => None,
            EliminationReason::Custom { value, .. } => Some(value),
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            EliminationReason::InvalidCharacters { chars, .. } => {
                format!("contains invalid characters: {}", chars)
            }
            EliminationReason::OutOfRange {
                component,
                actual,
                max,
                ..
            } => {
                format!("{} value {} exceeds maximum {}", component, actual, max)
            }
            EliminationReason::DateComponentInvalid {
                component, actual, ..
            } => {
                format!("{} value {} is invalid", component, actual)
            }
            EliminationReason::PatternMismatch { expected, .. } => {
                format!("doesn't match pattern: {}", expected)
            }
            EliminationReason::ParseFailed { error, .. } => {
                format!("parse failed: {}", error)
            }
            EliminationReason::ContainsDecimalPoint { .. } => "contains decimal point".to_string(),
            EliminationReason::NonNumericAffixes { affix, .. } => {
                format!("contains non-numeric affix: {}", affix)
            }
            EliminationReason::NotBooleanValue { value } => {
                format!("'{}' is not a boolean value", value)
            }
            EliminationReason::EmptyValue => "value is empty".to_string(),
            EliminationReason::Custom { reason, .. } => reason.clone(),
        }
    }
}

/// Evidence of an elimination (for debugging/explainability)
#[derive(Debug, Clone)]
pub struct EliminationEvidence {
    /// What was eliminated
    pub eliminated: EliminatedItem,
    /// Why it was eliminated
    pub reason: EliminationReason,
    /// Which row/value triggered the elimination (0-indexed)
    pub row_index: usize,
    /// The actual value that caused elimination
    pub value: String,
}

/// What was eliminated by a constraint
#[derive(Debug, Clone)]
pub enum EliminatedItem {
    /// A data type was eliminated
    Type(DataType),
    /// A date format was eliminated
    DateFormat(String),
    /// A time format was eliminated
    TimeFormat(String),
    /// A datetime format was eliminated
    DateTimeFormat(String),
}

impl std::fmt::Display for EliminatedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EliminatedItem::Type(dt) => write!(f, "type:{}", dt),
            EliminatedItem::DateFormat(fmt) => write!(f, "date_format:{}", fmt),
            EliminatedItem::TimeFormat(fmt) => write!(f, "time_format:{}", fmt),
            EliminatedItem::DateTimeFormat(fmt) => write!(f, "datetime_format:{}", fmt),
        }
    }
}

/// A contradiction where conflicting evidence exists
#[derive(Debug, Clone)]
pub struct Contradiction {
    /// Description of the contradiction
    pub message: String,
    /// Evidence that conflicts
    pub evidence: Vec<EliminationEvidence>,
}

impl Contradiction {
    /// Create a new contradiction
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            evidence: Vec::new(),
        }
    }

    /// Add evidence to the contradiction
    pub fn with_evidence(mut self, evidence: EliminationEvidence) -> Self {
        self.evidence.push(evidence);
        self
    }
}

impl std::fmt::Display for Contradiction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if !self.evidence.is_empty() {
            write!(f, " (evidence from {} rows)", self.evidence.len())?;
        }
        Ok(())
    }
}

/// Result of type inference for a column
#[derive(Debug, Clone)]
pub enum TypeInferenceResult {
    /// Type is resolved with certainty
    Resolved {
        data_type: DataType,
        /// For temporal types, the detected format
        format: Option<String>,
        /// How many values were processed to reach this conclusion
        values_processed: usize,
        /// Evidence that led to this resolution
        evidence: Vec<EliminationEvidence>,
    },
    /// Multiple types are still possible (data is ambiguous)
    Ambiguous {
        possible_types: Vec<DataType>,
        /// For each possible type, possible formats
        possible_formats: std::collections::HashMap<DataType, Vec<String>>,
        values_processed: usize,
    },
    /// Conflicting evidence found
    Contradiction(Contradiction),
    /// No valid type found (all types eliminated but String)
    NoValidType {
        /// Fallback to String
        fallback: DataType,
        /// Why each type was eliminated
        eliminations: Vec<EliminationEvidence>,
    },
}

impl TypeInferenceResult {
    /// Get the resolved data type (if resolved)
    pub fn data_type(&self) -> Option<DataType> {
        match self {
            TypeInferenceResult::Resolved { data_type, .. } => Some(*data_type),
            TypeInferenceResult::Ambiguous { possible_types, .. } => possible_types.first().copied(),
            TypeInferenceResult::NoValidType { fallback, .. } => Some(*fallback),
            TypeInferenceResult::Contradiction(_) => None,
        }
    }

    /// Check if the result is resolved
    pub fn is_resolved(&self) -> bool {
        matches!(self, TypeInferenceResult::Resolved { .. })
    }

    /// Get the format (for temporal types)
    pub fn format(&self) -> Option<&str> {
        match self {
            TypeInferenceResult::Resolved { format, .. } => format.as_deref(),
            _ => None,
        }
    }
}

impl std::fmt::Display for TypeInferenceResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeInferenceResult::Resolved {
                data_type, format, ..
            } => {
                if let Some(fmt) = format {
                    write!(f, "{}({})", data_type, fmt)
                } else {
                    write!(f, "{}", data_type)
                }
            }
            TypeInferenceResult::Ambiguous { possible_types, .. } => {
                let types: Vec<_> = possible_types.iter().map(|t| t.to_string()).collect();
                write!(f, "ambiguous[{}]", types.join(", "))
            }
            TypeInferenceResult::Contradiction(c) => {
                write!(f, "contradiction: {}", c)
            }
            TypeInferenceResult::NoValidType { fallback, .. } => {
                write!(f, "no_valid_type(fallback: {})", fallback)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elimination_reason_description() {
        let reason = EliminationReason::OutOfRange {
            value: "31/05/24".to_string(),
            component: "month".to_string(),
            actual: 31,
            max: 12,
        };
        assert!(reason.description().contains("month"));
        assert!(reason.description().contains("31"));
        assert!(reason.description().contains("12"));
    }

    #[test]
    fn test_type_inference_result_display() {
        let resolved = TypeInferenceResult::Resolved {
            data_type: DataType::Date,
            format: Some("DD/MM/YYYY".to_string()),
            values_processed: 100,
            evidence: vec![],
        };
        let display = format!("{}", resolved);
        assert!(display.contains("date"));
        assert!(display.contains("DD/MM/YYYY"));
    }

    #[test]
    fn test_contradiction() {
        let contradiction = Contradiction::new("Month cannot be both first and second position");
        assert!(contradiction.message.contains("Month"));
        assert!(contradiction.evidence.is_empty());
    }

    #[test]
    fn test_eliminated_item_display() {
        let item = EliminatedItem::DateFormat("MM/DD/YYYY".to_string());
        let display = format!("{}", item);
        assert!(display.contains("date_format"));
        assert!(display.contains("MM/DD/YYYY"));
    }
}
