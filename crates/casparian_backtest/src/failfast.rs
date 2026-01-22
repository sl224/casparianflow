//! Fail-fast backtest logic
//!
//! Tests high-failure files first and stops early if they don't pass.
//! This enables rapid feedback during parser development.

use crate::high_failure::{FailureHistoryEntry, FileInfo, HighFailureError, HighFailureTable};
use crate::metrics::{FailureCategory, IterationMetrics};
use crate::ScopeId;
use serde::{Deserialize, Serialize};

/// Configuration for fail-fast backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailFastConfig {
    /// Pass rate threshold for high-failure files (0.0 - 1.0)
    /// If high-failure files pass rate is below this, stop early
    pub high_failure_threshold: f32,

    /// Whether early stopping is enabled
    pub early_stop_enabled: bool,

    /// Maximum number of files to test before checking threshold
    pub check_after_n_files: usize,

    /// Minimum number of high-failure files before applying fail-fast
    pub min_high_failure_files: usize,
}

impl Default for FailFastConfig {
    fn default() -> Self {
        Self {
            high_failure_threshold: 0.8,
            early_stop_enabled: true,
            check_after_n_files: 10,
            min_high_failure_files: 3,
        }
    }
}

impl FailFastConfig {
    /// Create a new config with custom threshold
    pub fn with_threshold(threshold: f32) -> Self {
        Self {
            high_failure_threshold: threshold,
            ..Default::default()
        }
    }

    /// Disable early stopping (run all files)
    pub fn no_early_stop() -> Self {
        Self {
            early_stop_enabled: false,
            ..Default::default()
        }
    }
}

/// Result of a fail-fast backtest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BacktestResult {
    /// Backtest completed normally (all files tested)
    Complete {
        metrics: IterationMetrics,
        high_failure_pass_rate: f32,
        remaining_pass_rate: f32,
    },

    /// Backtest stopped early due to low high-failure pass rate
    EarlyStopped {
        metrics: IterationMetrics,
        high_failure_pass_rate: f32,
        files_tested: usize,
        files_remaining: usize,
        reason: String,
    },

    /// Backtest had an error
    Error {
        files_tested: usize,
        error: String,
    },
}

impl BacktestResult {
    /// Get pass rate regardless of result type
    pub fn pass_rate(&self) -> f32 {
        match self {
            BacktestResult::Complete { metrics, .. } => metrics.pass_rate,
            BacktestResult::EarlyStopped { metrics, .. } => metrics.pass_rate,
            BacktestResult::Error { .. } => 0.0,
        }
    }

    /// Whether the backtest completed without early stop
    pub fn is_complete(&self) -> bool {
        matches!(self, BacktestResult::Complete { .. })
    }

    /// Whether the backtest was stopped early
    pub fn is_early_stopped(&self) -> bool {
        matches!(self, BacktestResult::EarlyStopped { .. })
    }
}

/// File test result
#[derive(Debug, Clone)]
pub struct FileTestResult {
    pub file_path: String,
    pub passed: bool,
    pub error: Option<String>,
    pub category: Option<FailureCategory>,
}

/// Trait for running parser on a file
pub trait ParserRunner: Send + Sync {
    /// Run the parser on a file and return the result
    fn run(&self, file_path: &str) -> FileTestResult;
}

/// Run a fail-fast backtest
/// F-009: Take files by reference instead of by value to avoid cloning in iteration loop
pub fn backtest_with_failfast<P: ParserRunner>(
    parser: &P,
    files: &[FileInfo],
    high_failure_table: &HighFailureTable,
    scope_id: &ScopeId,
    parser_version: usize,
    iteration: usize,
    config: &FailFastConfig,
) -> Result<BacktestResult, HighFailureError> {
    // Get files in optimal order
    let ordered_files = high_failure_table.get_backtest_order(files, scope_id)?;

    let total_files = ordered_files.len();
    if total_files == 0 {
        return Ok(BacktestResult::Complete {
            metrics: IterationMetrics::new(iteration, parser_version),
            high_failure_pass_rate: 1.0,
            remaining_pass_rate: 1.0,
        });
    }

    let mut metrics = IterationMetrics::new(iteration, parser_version);
    let mut high_failure_tested = 0;
    let mut high_failure_passed = 0;
    let mut remaining_tested = 0;
    let mut remaining_passed = 0;

    let start_time = std::time::Instant::now();

    for (idx, file) in ordered_files.iter().enumerate() {
        // Run parser on file
        let result = parser.run(&file.path);

        if result.passed {
            metrics.record_pass();
            high_failure_table.record_success(&file.path, scope_id)?;

            if file.is_high_failure {
                high_failure_passed += 1;
            } else {
                remaining_passed += 1;
            }
        } else {
            let category = result.category.unwrap_or(FailureCategory::Unknown);
            let error_msg = result.error.as_deref().unwrap_or("Unknown error");
            metrics.record_fail(&file.path, category, error_msg);

            // Record failure in high-failure table
            let entry = FailureHistoryEntry::new(iteration, parser_version, category, error_msg);
            high_failure_table.record_failure(&file.path, scope_id, entry)?;
        }

        // Track high-failure vs remaining
        if file.is_high_failure {
            high_failure_tested += 1;
        } else {
            remaining_tested += 1;
        }

        // Check for early stop after testing high-failure files
        if config.early_stop_enabled
            && file.is_high_failure
            && high_failure_tested >= config.min_high_failure_files
        {
            // Look ahead to see if we've finished all high-failure files
            let next_is_not_high_failure = ordered_files
                .get(idx + 1)
                .map(|f| !f.is_high_failure)
                .unwrap_or(true);

            if next_is_not_high_failure || high_failure_tested >= config.check_after_n_files {
                let hf_pass_rate = if high_failure_tested > 0 {
                    high_failure_passed as f32 / high_failure_tested as f32
                } else {
                    1.0
                };

                if hf_pass_rate < config.high_failure_threshold {
                    metrics.duration_ms = start_time.elapsed().as_millis() as u64;
                    metrics.finalize();

                    return Ok(BacktestResult::EarlyStopped {
                        metrics,
                        high_failure_pass_rate: hf_pass_rate,
                        files_tested: idx + 1,
                        files_remaining: total_files - idx - 1,
                        reason: format!(
                            "High-failure pass rate ({:.1}%) below threshold ({:.1}%)",
                            hf_pass_rate * 100.0,
                            config.high_failure_threshold * 100.0
                        ),
                    });
                }
            }
        }
    }

    metrics.duration_ms = start_time.elapsed().as_millis() as u64;
    metrics.finalize();

    let high_failure_pass_rate = if high_failure_tested > 0 {
        high_failure_passed as f32 / high_failure_tested as f32
    } else {
        1.0
    };

    let remaining_pass_rate = if remaining_tested > 0 {
        remaining_passed as f32 / remaining_tested as f32
    } else {
        1.0
    };

    Ok(BacktestResult::Complete {
        metrics,
        high_failure_pass_rate,
        remaining_pass_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockParser {
        /// Files that should fail
        failing_files: Vec<String>,
    }

    impl ParserRunner for MockParser {
        fn run(&self, file_path: &str) -> FileTestResult {
            if self.failing_files.contains(&file_path.to_string()) {
                FileTestResult {
                    file_path: file_path.to_string(),
                    passed: false,
                    error: Some("Mock failure".to_string()),
                    category: Some(FailureCategory::TypeMismatch),
                }
            } else {
                FileTestResult {
                    file_path: file_path.to_string(),
                    passed: true,
                    error: None,
                    category: None,
                }
            }
        }
    }

    fn create_test_table() -> HighFailureTable {
        HighFailureTable::in_memory().unwrap()
    }

    #[test]
    fn test_backtest_all_pass() {
        let table = create_test_table();
        let scope_id = ScopeId::new();
        let parser = MockParser {
            failing_files: vec![],
        };
        let config = FailFastConfig::default();

        let files = vec![
            FileInfo::new("/path/a.csv", 100),
            FileInfo::new("/path/b.csv", 100),
            FileInfo::new("/path/c.csv", 100),
        ];

        let result =
            backtest_with_failfast(&parser, &files, &table, &scope_id, 1, 1, &config).unwrap();

        assert!(result.is_complete());
        assert!((result.pass_rate() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_backtest_some_fail() {
        let table = create_test_table();
        let scope_id = ScopeId::new();
        let parser = MockParser {
            failing_files: vec!["/path/a.csv".to_string()],
        };
        let config = FailFastConfig::no_early_stop(); // Disable early stop

        let files = vec![
            FileInfo::new("/path/a.csv", 100),
            FileInfo::new("/path/b.csv", 100),
            FileInfo::new("/path/c.csv", 100),
        ];

        let result =
            backtest_with_failfast(&parser, &files, &table, &scope_id, 1, 1, &config).unwrap();

        assert!(result.is_complete());
        assert!((result.pass_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_backtest_early_stop() {
        let table = create_test_table();
        let scope_id = ScopeId::new();

        // Record some prior failures
        for i in 0..5 {
            let entry =
                FailureHistoryEntry::new(0, 1, FailureCategory::TypeMismatch, "Prior failure");
            table
                .record_failure(&format!("/path/high{}.csv", i), &scope_id, entry)
                
                .unwrap();
        }

        // Parser still fails on high-failure files
        let parser = MockParser {
            failing_files: vec![
                "/path/high0.csv".to_string(),
                "/path/high1.csv".to_string(),
                "/path/high2.csv".to_string(),
                "/path/high3.csv".to_string(),
            ],
        };

        let config = FailFastConfig {
            high_failure_threshold: 0.8,
            early_stop_enabled: true,
            check_after_n_files: 5,
            min_high_failure_files: 3,
        };

        let files = vec![
            FileInfo::new("/path/high0.csv", 100),
            FileInfo::new("/path/high1.csv", 100),
            FileInfo::new("/path/high2.csv", 100),
            FileInfo::new("/path/high3.csv", 100),
            FileInfo::new("/path/high4.csv", 100),
            FileInfo::new("/path/good1.csv", 100),
            FileInfo::new("/path/good2.csv", 100),
            FileInfo::new("/path/good3.csv", 100),
        ];

        let result =
            backtest_with_failfast(&parser, &files, &table, &scope_id, 1, 1, &config).unwrap();

        // Should have stopped early because high-failure pass rate < 80%
        assert!(result.is_early_stopped());
    }

    #[test]
    fn test_backtest_continues_if_high_failure_passes() {
        let table = create_test_table();
        let scope_id = ScopeId::new();

        // Record some prior failures
        for i in 0..3 {
            let entry =
                FailureHistoryEntry::new(0, 1, FailureCategory::TypeMismatch, "Prior failure");
            table
                .record_failure(&format!("/path/high{}.csv", i), &scope_id, entry)
                
                .unwrap();
        }

        // Parser now passes on high-failure files (fixes were made)
        let parser = MockParser {
            failing_files: vec!["/path/other.csv".to_string()],
        };

        let config = FailFastConfig {
            high_failure_threshold: 0.8,
            early_stop_enabled: true,
            check_after_n_files: 3,
            min_high_failure_files: 2,
        };

        let files = vec![
            FileInfo::new("/path/high0.csv", 100),
            FileInfo::new("/path/high1.csv", 100),
            FileInfo::new("/path/high2.csv", 100),
            FileInfo::new("/path/other.csv", 100),
        ];

        let result =
            backtest_with_failfast(&parser, &files, &table, &scope_id, 1, 1, &config).unwrap();

        // Should complete because high-failure files passed
        assert!(result.is_complete());
    }

    #[test]
    fn test_empty_files() {
        let table = create_test_table();
        let scope_id = ScopeId::new();
        let parser = MockParser {
            failing_files: vec![],
        };
        let config = FailFastConfig::default();

        let result =
            backtest_with_failfast(&parser, &[], &table, &scope_id, 1, 1, &config).unwrap();

        assert!(result.is_complete());
        assert!((result.pass_rate() - 0.0).abs() < 0.001); // 0/0 defaults to 0
    }
}
