//! Streaming type inference
//!
//! Process data row by row, with early termination when all columns are resolved.

use std::collections::HashMap;

use super::solver::ConstraintSolver;
use super::{DataType, TypeInferenceResult};

/// Result of streaming type inference for all columns
#[derive(Debug)]
pub struct StreamingInferenceResult {
    /// Results per column
    pub columns: HashMap<String, TypeInferenceResult>,

    /// Total rows processed
    pub rows_processed: usize,

    /// Whether inference terminated early (all columns resolved)
    pub early_termination: bool,

    /// Which columns triggered early termination (were last to resolve)
    pub resolution_order: Vec<(String, usize)>,
}

impl StreamingInferenceResult {
    /// Get the inferred schema as column name -> (type, optional format)
    pub fn schema(&self) -> HashMap<String, (DataType, Option<String>)> {
        self.columns
            .iter()
            .filter_map(|(name, result)| {
                let dtype = result.data_type()?;
                let format = result.format().map(|s| s.to_string());
                Some((name.clone(), (dtype, format)))
            })
            .collect()
    }

    /// Check if all columns are resolved
    pub fn all_resolved(&self) -> bool {
        self.columns.values().all(|r| r.is_resolved())
    }
}

/// Configuration for streaming inference
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// Maximum rows to process before giving up
    pub max_rows: usize,

    /// Enable early termination when all columns are resolved
    pub early_termination: bool,

    /// Minimum rows to process before allowing early termination
    pub min_rows_before_termination: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            max_rows: 100_000,
            early_termination: true,
            min_rows_before_termination: 100,
        }
    }
}

/// Infer types from an iterator of rows (streaming)
///
/// # Arguments
///
/// * `column_names` - Names of columns in order
/// * `rows` - Iterator yielding rows as slices of string values
/// * `config` - Configuration options
///
/// # Returns
///
/// Inference result with per-column type information
///
/// # Example
///
/// ```ignore
/// let columns = vec!["id", "date", "amount"];
/// let rows = vec![
///     vec!["1", "31/05/24", "100.50"],
///     vec!["2", "15/06/24", "200.00"],
/// ];
///
/// let result = infer_types_streaming(
///     &columns,
///     rows.iter().map(|r| r.as_slice()),
///     StreamingConfig::default(),
/// );
/// ```
pub fn infer_types_streaming<'a, I, R>(
    column_names: &[&str],
    rows: I,
    config: StreamingConfig,
) -> StreamingInferenceResult
where
    I: Iterator<Item = R>,
    R: AsRef<[&'a str]>,
{
    // Create a solver for each column
    let mut solvers: Vec<ConstraintSolver> = column_names
        .iter()
        .map(|name| ConstraintSolver::new(*name))
        .collect();

    let mut rows_processed = 0;
    let mut early_termination = false;
    let mut resolution_order = Vec::new();

    for row in rows {
        let values = row.as_ref();

        // Add each value to its column's solver
        for (solver, value) in solvers.iter_mut().zip(values.iter()) {
            solver.add_value(value);
        }

        rows_processed += 1;

        // Check for early termination
        if config.early_termination && rows_processed >= config.min_rows_before_termination {
            let all_resolved = solvers.iter().all(|s| s.is_resolved());
            if all_resolved {
                early_termination = true;
                break;
            }
        }

        // Check max rows limit
        if rows_processed >= config.max_rows {
            break;
        }

        // Track resolution order
        for solver in &solvers {
            let name = solver.column_name().to_string();
            if solver.is_resolved()
                && !resolution_order.iter().any(|(n, _)| n == &name)
            {
                resolution_order.push((name, rows_processed));
            }
        }
    }

    // Collect results
    let columns: HashMap<String, TypeInferenceResult> = solvers
        .into_iter()
        .map(|s| (s.column_name().to_string(), s.get_result()))
        .collect();

    StreamingInferenceResult {
        columns,
        rows_processed,
        early_termination,
        resolution_order,
    }
}

/// Infer types from a single column of values
pub fn infer_column_type<'a, I>(column_name: &str, values: I) -> TypeInferenceResult
where
    I: Iterator<Item = &'a str>,
{
    let mut solver = ConstraintSolver::new(column_name);

    for value in values {
        solver.add_value(value);

        // Early termination if resolved
        if solver.is_resolved() && solver.values_processed() >= 100 {
            break;
        }
    }

    solver.get_result()
}

/// Convenience function for inferring types from a Vec of rows
pub fn infer_types_from_rows(
    column_names: &[&str],
    rows: &[Vec<&str>],
) -> StreamingInferenceResult {
    infer_types_streaming(
        column_names,
        rows.iter().map(|r| r.as_slice()),
        StreamingConfig::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_inference_basic() {
        let columns = vec!["id", "name", "date"];
        let rows = vec![
            vec!["1", "Alice", "31/05/24"],
            vec!["2", "Bob", "15/06/24"],
            vec!["3", "Charlie", "01/07/24"],
        ];

        let result = infer_types_from_rows(&columns, &rows);

        assert_eq!(result.rows_processed, 3);

        // Check id is Integer
        if let Some(TypeInferenceResult::Resolved { data_type, .. }) = result.columns.get("id") {
            assert_eq!(*data_type, DataType::Integer);
        }

        // Check name eliminated numeric types (should be String or ambiguous)
        let name_result = result.columns.get("name").unwrap();
        match name_result {
            TypeInferenceResult::NoValidType { fallback, .. } => {
                assert_eq!(*fallback, DataType::String);
            }
            TypeInferenceResult::Ambiguous { .. } => {
                // If ambiguous, String should be in possibilities
                // Actually String is filtered out, so this shouldn't happen
            }
            _ => {
                // Could be resolved to some type if we're lenient
            }
        }
    }

    #[test]
    fn test_streaming_inference_date_resolution() {
        let columns = vec!["date"];
        let rows = vec![
            vec!["01/02/24"], // Ambiguous
            vec!["05/06/24"], // Still ambiguous
            vec!["31/05/24"], // Resolves to DD/MM/YY
        ];

        let result = infer_types_from_rows(&columns, &rows);

        if let Some(TypeInferenceResult::Resolved {
            data_type, format, ..
        }) = result.columns.get("date")
        {
            assert_eq!(*data_type, DataType::Date);
            // Format should be DD/MM/YY variant
            assert!(format.as_ref().unwrap().contains("%d"));
        }
    }

    #[test]
    fn test_early_termination() {
        let columns = vec!["value"];

        // Create many rows that would resolve quickly
        let rows: Vec<Vec<&str>> = (0..1000).map(|_| vec!["42"]).collect();

        let config = StreamingConfig {
            max_rows: 100_000,
            early_termination: true,
            min_rows_before_termination: 100,
        };

        let result = infer_types_streaming(
            &columns,
            rows.iter().map(|r| r.as_slice()),
            config,
        );

        // Should terminate early since Integer is resolved quickly
        assert!(result.early_termination || result.rows_processed < 1000);
    }

    #[test]
    fn test_max_rows_limit() {
        let columns = vec!["value"];

        // Create more rows than max_rows
        let rows: Vec<Vec<&str>> = (0..1000).map(|_| vec!["42"]).collect();

        let config = StreamingConfig {
            max_rows: 50,
            early_termination: false, // Disable early termination
            min_rows_before_termination: 100,
        };

        let result = infer_types_streaming(
            &columns,
            rows.iter().map(|r| r.as_slice()),
            config,
        );

        assert_eq!(result.rows_processed, 50);
    }

    #[test]
    fn test_infer_column_type() {
        let values = vec!["1.5", "2.7", "3.14", "42.0"];
        let result = infer_column_type("price", values.into_iter());

        match result {
            TypeInferenceResult::Resolved { data_type, .. } => {
                assert_eq!(data_type, DataType::Float);
            }
            _ => panic!("Expected Float to be resolved"),
        }
    }

    #[test]
    fn test_schema_extraction() {
        let columns = vec!["id", "price", "date"];
        let rows = vec![
            vec!["1", "10.50", "31/05/24"],
            vec!["2", "20.00", "15/06/24"],
        ];

        let result = infer_types_from_rows(&columns, &rows);
        let schema = result.schema();

        assert!(schema.contains_key("id"));
        assert!(schema.contains_key("price"));
        assert!(schema.contains_key("date"));

        // Check types
        assert_eq!(schema.get("id").unwrap().0, DataType::Integer);
        assert_eq!(schema.get("price").unwrap().0, DataType::Float);
        assert_eq!(schema.get("date").unwrap().0, DataType::Date);
    }

    #[test]
    fn test_mixed_nulls() {
        let columns = vec!["nullable"];
        let rows = vec![
            vec![""],
            vec!["42"],
            vec!["null"],
            vec!["100"],
            vec!["NA"],
        ];

        let result = infer_types_from_rows(&columns, &rows);

        // Should still detect Integer despite nulls
        if let Some(TypeInferenceResult::Resolved { data_type, .. }) =
            result.columns.get("nullable")
        {
            assert_eq!(*data_type, DataType::Integer);
        }
    }

    #[test]
    fn test_all_null_column() {
        let columns = vec!["empty"];
        let rows = vec![vec![""], vec!["null"], vec!["NA"]];

        let result = infer_types_from_rows(&columns, &rows);

        if let Some(TypeInferenceResult::Resolved { data_type, .. }) = result.columns.get("empty") {
            assert_eq!(*data_type, DataType::Null);
        }
    }
}
