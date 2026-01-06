//! Iteration metrics and failure analysis
//!
//! Provides categorization and summarization of failures during backtest iterations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Categories of failures that can occur during parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    /// Value type doesn't match schema (e.g., "abc" for Int64)
    TypeMismatch,
    /// Null/empty value in non-nullable column
    NullNotAllowed,
    /// Value doesn't match expected format (e.g., wrong date format)
    FormatMismatch,
    /// Parser failed to read/process the file
    ParseError,
    /// Data violates schema contract
    SchemaViolation,
    /// File not found or inaccessible
    FileNotFound,
    /// Unknown/uncategorized error
    Unknown,
}

impl FailureCategory {
    /// Get a human-readable label for this category
    pub fn label(&self) -> &'static str {
        match self {
            FailureCategory::TypeMismatch => "Type Mismatch",
            FailureCategory::NullNotAllowed => "Null Not Allowed",
            FailureCategory::FormatMismatch => "Format Mismatch",
            FailureCategory::ParseError => "Parse Error",
            FailureCategory::SchemaViolation => "Schema Violation",
            FailureCategory::FileNotFound => "File Not Found",
            FailureCategory::Unknown => "Unknown",
        }
    }

    /// Categorize an error message into a failure category
    pub fn from_error_message(msg: &str) -> Self {
        let msg_lower = msg.to_lowercase();

        if msg_lower.contains("type") && msg_lower.contains("mismatch") {
            FailureCategory::TypeMismatch
        } else if msg_lower.contains("null") || msg_lower.contains("empty") {
            FailureCategory::NullNotAllowed
        } else if msg_lower.contains("format") {
            FailureCategory::FormatMismatch
        } else if msg_lower.contains("parse") || msg_lower.contains("parsing") {
            FailureCategory::ParseError
        } else if msg_lower.contains("schema") {
            FailureCategory::SchemaViolation
        } else if msg_lower.contains("not found") || msg_lower.contains("no such file") {
            FailureCategory::FileNotFound
        } else {
            FailureCategory::Unknown
        }
    }
}

impl std::fmt::Display for FailureCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Summary of failures in a backtest iteration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureSummary {
    /// Total number of failures
    pub total_failures: usize,
    /// Failures by category
    pub by_category: HashMap<FailureCategory, usize>,
    /// Top failing files (path, failure count)
    pub top_failing_files: Vec<(String, usize)>,
    /// Sample error messages by category
    pub sample_errors: HashMap<FailureCategory, Vec<String>>,
}

impl FailureSummary {
    /// Create a new empty failure summary
    pub fn new() -> Self {
        Self {
            total_failures: 0,
            by_category: HashMap::new(),
            top_failing_files: Vec::new(),
            sample_errors: HashMap::new(),
        }
    }

    /// Record a failure
    pub fn record_failure(&mut self, file_path: &str, category: FailureCategory, error_msg: &str) {
        self.total_failures += 1;

        // Count by category
        *self.by_category.entry(category).or_insert(0) += 1;

        // Track failing files
        if let Some(pos) = self.top_failing_files.iter().position(|(p, _)| p == file_path) {
            self.top_failing_files[pos].1 += 1;
        } else {
            self.top_failing_files.push((file_path.to_string(), 1));
        }

        // Keep sample errors (max 3 per category)
        let samples = self.sample_errors.entry(category).or_insert_with(Vec::new);
        if samples.len() < 3 && !samples.contains(&error_msg.to_string()) {
            samples.push(error_msg.to_string());
        }
    }

    /// Sort and limit top failing files
    pub fn finalize(&mut self, limit: usize) {
        self.top_failing_files.sort_by(|a, b| b.1.cmp(&a.1));
        self.top_failing_files.truncate(limit);
    }

    /// Get the most common failure category
    pub fn most_common_category(&self) -> Option<FailureCategory> {
        self.by_category
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(cat, _)| *cat)
    }

    /// Get failure rate for a category
    pub fn category_rate(&self, category: FailureCategory) -> f32 {
        if self.total_failures == 0 {
            return 0.0;
        }
        let count = self.by_category.get(&category).copied().unwrap_or(0);
        count as f32 / self.total_failures as f32
    }
}

impl Default for FailureSummary {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics for a single backtest iteration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationMetrics {
    /// Which iteration this is
    pub iteration: usize,
    /// Parser version used
    pub parser_version: usize,
    /// Total files tested
    pub files_tested: usize,
    /// Files that passed
    pub files_passed: usize,
    /// Files that failed
    pub files_failed: usize,
    /// Pass rate (0.0 - 1.0)
    pub pass_rate: f32,
    /// Duration of this iteration in milliseconds
    pub duration_ms: u64,
    /// Failure summary
    pub failure_summary: FailureSummary,
}

impl IterationMetrics {
    /// Create new iteration metrics
    pub fn new(iteration: usize, parser_version: usize) -> Self {
        Self {
            iteration,
            parser_version,
            files_tested: 0,
            files_passed: 0,
            files_failed: 0,
            pass_rate: 0.0,
            duration_ms: 0,
            failure_summary: FailureSummary::new(),
        }
    }

    /// Record a passed file
    pub fn record_pass(&mut self) {
        self.files_tested += 1;
        self.files_passed += 1;
        self.update_pass_rate();
    }

    /// Record a failed file
    pub fn record_fail(&mut self, file_path: &str, category: FailureCategory, error_msg: &str) {
        self.files_tested += 1;
        self.files_failed += 1;
        self.failure_summary.record_failure(file_path, category, error_msg);
        self.update_pass_rate();
    }

    /// Update the pass rate
    fn update_pass_rate(&mut self) {
        if self.files_tested > 0 {
            self.pass_rate = self.files_passed as f32 / self.files_tested as f32;
        }
    }

    /// Finalize metrics (sort/limit failure summary)
    pub fn finalize(&mut self) {
        self.failure_summary.finalize(10);
    }
}

/// Aggregate metrics across all backtest iterations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestMetrics {
    /// Total iterations completed
    pub iterations: usize,
    /// Final pass rate
    pub final_pass_rate: f32,
    /// Best pass rate achieved
    pub best_pass_rate: f32,
    /// Which iteration achieved best pass rate
    pub best_iteration: usize,
    /// Pass rate history
    pub pass_rate_history: Vec<f32>,
    /// Total files tested (unique)
    pub total_files_tested: usize,
    /// Total test executions (may include retries)
    pub total_test_executions: usize,
    /// Total duration in milliseconds
    pub total_duration_ms: u64,
    /// Aggregate failure summary
    pub aggregate_failures: FailureSummary,
}

impl BacktestMetrics {
    /// Create new backtest metrics
    pub fn new() -> Self {
        Self {
            iterations: 0,
            final_pass_rate: 0.0,
            best_pass_rate: 0.0,
            best_iteration: 0,
            pass_rate_history: Vec::new(),
            total_files_tested: 0,
            total_test_executions: 0,
            total_duration_ms: 0,
            aggregate_failures: FailureSummary::new(),
        }
    }

    /// Record metrics from an iteration
    pub fn record_iteration(&mut self, metrics: &IterationMetrics) {
        self.iterations += 1;
        self.final_pass_rate = metrics.pass_rate;
        self.pass_rate_history.push(metrics.pass_rate);
        self.total_test_executions += metrics.files_tested;
        self.total_duration_ms += metrics.duration_ms;

        if metrics.pass_rate > self.best_pass_rate {
            self.best_pass_rate = metrics.pass_rate;
            self.best_iteration = metrics.iteration;
        }

        // Merge failure summary
        for (cat, count) in &metrics.failure_summary.by_category {
            *self.aggregate_failures.by_category.entry(*cat).or_insert(0) += count;
        }
        self.aggregate_failures.total_failures += metrics.failure_summary.total_failures;
    }

    /// Check if there's a plateau (no improvement for N iterations)
    pub fn has_plateau(&self, window: usize) -> bool {
        if self.pass_rate_history.len() < window {
            return false;
        }

        let recent: Vec<_> = self.pass_rate_history.iter().rev().take(window).collect();
        if recent.is_empty() {
            return false;
        }

        // Check if all values are within 0.01 of each other
        let first = recent[0];
        recent.iter().all(|r| (*r - first).abs() < 0.01)
    }

    /// Get improvement from last iteration
    pub fn last_improvement(&self) -> f32 {
        if self.pass_rate_history.len() < 2 {
            return 0.0;
        }
        let len = self.pass_rate_history.len();
        self.pass_rate_history[len - 1] - self.pass_rate_history[len - 2]
    }
}

impl Default for BacktestMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failure_category_from_message() {
        assert_eq!(
            FailureCategory::from_error_message("Type mismatch: expected Int64"),
            FailureCategory::TypeMismatch
        );
        assert_eq!(
            FailureCategory::from_error_message("Column 'id' cannot be null"),
            FailureCategory::NullNotAllowed
        );
        assert_eq!(
            FailureCategory::from_error_message("Invalid date format"),
            FailureCategory::FormatMismatch
        );
        assert_eq!(
            FailureCategory::from_error_message("Failed to parse CSV"),
            FailureCategory::ParseError
        );
        assert_eq!(
            FailureCategory::from_error_message("Some random error"),
            FailureCategory::Unknown
        );
    }

    #[test]
    fn test_failure_summary() {
        let mut summary = FailureSummary::new();

        summary.record_failure("/path/a.csv", FailureCategory::TypeMismatch, "Error 1");
        summary.record_failure("/path/a.csv", FailureCategory::TypeMismatch, "Error 2");
        summary.record_failure("/path/b.csv", FailureCategory::NullNotAllowed, "Error 3");

        assert_eq!(summary.total_failures, 3);
        assert_eq!(summary.by_category[&FailureCategory::TypeMismatch], 2);
        assert_eq!(summary.by_category[&FailureCategory::NullNotAllowed], 1);
        assert_eq!(summary.most_common_category(), Some(FailureCategory::TypeMismatch));

        summary.finalize(10);
        assert_eq!(summary.top_failing_files[0], ("/path/a.csv".to_string(), 2));
    }

    #[test]
    fn test_iteration_metrics() {
        let mut metrics = IterationMetrics::new(1, 1);

        metrics.record_pass();
        metrics.record_pass();
        metrics.record_fail("/path/a.csv", FailureCategory::TypeMismatch, "Error");

        assert_eq!(metrics.files_tested, 3);
        assert_eq!(metrics.files_passed, 2);
        assert_eq!(metrics.files_failed, 1);
        assert!((metrics.pass_rate - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_backtest_metrics_plateau() {
        let mut metrics = BacktestMetrics::new();

        // Add some iterations with same pass rate
        for i in 0..5 {
            let mut iter = IterationMetrics::new(i, 1);
            iter.pass_rate = 0.8;
            iter.files_tested = 10;
            metrics.record_iteration(&iter);
        }

        assert!(metrics.has_plateau(3));

        // Add an improving iteration
        let mut iter = IterationMetrics::new(5, 1);
        iter.pass_rate = 0.9;
        iter.files_tested = 10;
        metrics.record_iteration(&iter);

        assert!(!metrics.has_plateau(3)); // No longer a plateau
    }

    #[test]
    fn test_backtest_metrics_improvement() {
        let mut metrics = BacktestMetrics::new();

        let mut iter1 = IterationMetrics::new(1, 1);
        iter1.pass_rate = 0.7;
        metrics.record_iteration(&iter1);

        let mut iter2 = IterationMetrics::new(2, 2);
        iter2.pass_rate = 0.85;
        metrics.record_iteration(&iter2);

        assert!((metrics.last_improvement() - 0.15).abs() < 0.001);
    }
}
