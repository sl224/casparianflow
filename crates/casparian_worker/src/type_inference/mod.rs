//! Constraint-based type inference engine
//!
//! Uses ALL values to eliminate possibilities (not sampling).
//! Each value adds constraints. Intersection = proven type.
//!
//! # Key Insight
//!
//! Traditional type inference uses voting: "70% of values look like dates, so it's a date."
//! This approach uses elimination: "This value CANNOT be month (31 > 12), so DD/MM/YY is proven."
//!
//! # Example
//!
//! Given a column with values: ["15/06/24", "31/05/24", "01/12/24"]
//!
//! - "15/06/24" could be DD/MM/YY or MM/DD/YY (15 could be day, 06 could be month or day)
//! - "31/05/24" PROVES it's DD/MM/YY because 31 cannot be a month
//! - After seeing "31/05/24", the format is resolved with certainty
//!
//! # Algorithm
//!
//! 1. Start with all possible types and formats
//! 2. For each value, eliminate impossible interpretations
//! 3. Continue until either:
//!    - Only one possibility remains (resolved)
//!    - All possibilities eliminated (contradiction/no valid type)
//!    - End of data reached with multiple possibilities (ambiguous)

pub mod constraints;
pub mod date_formats;
pub mod solver;
pub mod streaming;

pub use constraints::{
    Constraint, Contradiction, EliminationEvidence, EliminationReason, TypeInferenceResult,
};
pub use date_formats::{ParsedDate, DATE_FORMATS};
pub use solver::ConstraintSolver;
pub use streaming::infer_types_streaming;

/// Type inference engine data types.
///
/// Uses inference-friendly names (Integer, Float, DateTime) rather than
/// Arrow-compatible names (Int64, Float64, Timestamp) used in protocol.
/// This makes the inference engine's output more readable.
///
/// The inference engine works with these types, then converts to
/// `casparian_protocol::DataType` for output via `From` impl.
///
/// # Name Mapping
///
/// | Inference | Protocol | Arrow |
/// |-----------|----------|-------|
/// | Integer | Int64 | Int64 |
/// | Float | Float64 | Float64 |
/// | DateTime | Timestamp | Timestamp |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataType {
    /// Null/empty value
    Null,
    /// Boolean (true/false, yes/no, 1/0)
    Boolean,
    /// 64-bit signed integer
    Integer,
    /// 64-bit floating point
    Float,
    /// Date (no time component)
    Date,
    /// DateTime (date + time)
    DateTime,
    /// Time (no date component)
    Time,
    /// Duration/interval
    Duration,
    /// UTF-8 string (fallback)
    String,
}

impl DataType {
    /// Returns all possible data types (for initialization)
    pub fn all() -> Vec<DataType> {
        vec![
            DataType::Null,
            DataType::Boolean,
            DataType::Integer,
            DataType::Float,
            DataType::Date,
            DataType::DateTime,
            DataType::Time,
            DataType::Duration,
            DataType::String,
        ]
    }

    /// Returns numeric types
    pub fn numeric() -> Vec<DataType> {
        vec![DataType::Integer, DataType::Float]
    }

    /// Returns temporal types
    pub fn temporal() -> Vec<DataType> {
        vec![DataType::Date, DataType::DateTime, DataType::Time]
    }

    /// Returns true if this type is numeric
    pub fn is_numeric(&self) -> bool {
        matches!(self, DataType::Integer | DataType::Float)
    }

    /// Returns true if this type is temporal
    pub fn is_temporal(&self) -> bool {
        matches!(self, DataType::Date | DataType::DateTime | DataType::Time)
    }
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataType::Null => write!(f, "null"),
            DataType::Boolean => write!(f, "boolean"),
            DataType::Integer => write!(f, "integer"),
            DataType::Float => write!(f, "float"),
            DataType::Date => write!(f, "date"),
            DataType::DateTime => write!(f, "datetime"),
            DataType::Time => write!(f, "time"),
            DataType::Duration => write!(f, "duration"),
            DataType::String => write!(f, "string"),
        }
    }
}

// ============================================================================
// Conversions to/from canonical casparian_protocol::DataType
// ============================================================================

impl From<DataType> for casparian_protocol::DataType {
    fn from(dt: DataType) -> Self {
        match dt {
            DataType::Null => casparian_protocol::DataType::Null,
            DataType::Boolean => casparian_protocol::DataType::Boolean,
            DataType::Integer => casparian_protocol::DataType::Int64,
            DataType::Float => casparian_protocol::DataType::Float64,
            DataType::Date => casparian_protocol::DataType::Date,
            DataType::DateTime => casparian_protocol::DataType::Timestamp,
            DataType::Time => casparian_protocol::DataType::Time,
            DataType::Duration => casparian_protocol::DataType::Duration,
            DataType::String => casparian_protocol::DataType::String,
        }
    }
}

impl From<casparian_protocol::DataType> for DataType {
    fn from(dt: casparian_protocol::DataType) -> Self {
        match dt {
            casparian_protocol::DataType::Null => DataType::Null,
            casparian_protocol::DataType::Boolean => DataType::Boolean,
            casparian_protocol::DataType::Int64 => DataType::Integer,
            casparian_protocol::DataType::Float64 => DataType::Float,
            casparian_protocol::DataType::Date => DataType::Date,
            casparian_protocol::DataType::Timestamp => DataType::DateTime,
            casparian_protocol::DataType::Time => DataType::Time,
            casparian_protocol::DataType::Duration => DataType::Duration,
            casparian_protocol::DataType::String => DataType::String,
            casparian_protocol::DataType::Binary => DataType::String, // Fallback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datatype_all() {
        let all = DataType::all();
        assert!(all.contains(&DataType::Integer));
        assert!(all.contains(&DataType::String));
        assert!(all.contains(&DataType::Date));
    }

    #[test]
    fn test_datatype_is_numeric() {
        assert!(DataType::Integer.is_numeric());
        assert!(DataType::Float.is_numeric());
        assert!(!DataType::String.is_numeric());
        assert!(!DataType::Date.is_numeric());
    }

    #[test]
    fn test_datatype_is_temporal() {
        assert!(DataType::Date.is_temporal());
        assert!(DataType::DateTime.is_temporal());
        assert!(DataType::Time.is_temporal());
        assert!(!DataType::Integer.is_temporal());
        assert!(!DataType::String.is_temporal());
    }
}
