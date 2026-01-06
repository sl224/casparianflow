//! Backtest iteration loop
//!
//! Runs multiple backtest iterations until a termination condition is met.
//! Supports pass rate thresholds, plateau detection, and timeouts.

use crate::failfast::{backtest_with_failfast, BacktestResult, FailFastConfig, ParserRunner};
use crate::high_failure::{FileInfo, HighFailureError, HighFailureTable};
use crate::metrics::{BacktestMetrics, IterationMetrics};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Configuration for the backtest iteration loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationConfig {
    /// Maximum number of iterations to run
    pub max_iterations: usize,

    /// Maximum duration for the entire backtest (seconds)
    pub max_duration_secs: u64,

    /// Target pass rate to achieve (0.0 - 1.0)
    pub pass_rate_threshold: f32,

    /// Minimum improvement required per iteration
    pub improvement_threshold: f32,

    /// Number of iterations without improvement before stopping
    pub plateau_window: usize,

    /// Fail-fast configuration
    pub failfast_config: FailFastConfig,
}

impl Default for IterationConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            max_duration_secs: 300, // 5 minutes
            pass_rate_threshold: 0.95,
            improvement_threshold: 0.01,
            plateau_window: 3,
            failfast_config: FailFastConfig::default(),
        }
    }
}

impl IterationConfig {
    /// Create a quick test config (fewer iterations, lower threshold)
    pub fn quick_test() -> Self {
        Self {
            max_iterations: 3,
            max_duration_secs: 60,
            pass_rate_threshold: 0.80,
            improvement_threshold: 0.05,
            plateau_window: 2,
            failfast_config: FailFastConfig::default(),
        }
    }

    /// Create a thorough test config (more iterations, higher threshold)
    pub fn thorough() -> Self {
        Self {
            max_iterations: 20,
            max_duration_secs: 600, // 10 minutes
            pass_rate_threshold: 0.99,
            improvement_threshold: 0.005,
            plateau_window: 5,
            failfast_config: FailFastConfig::default(),
        }
    }
}

/// A single backtest iteration result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestIteration {
    /// Which iteration this is
    pub iteration: usize,
    /// Parser version used
    pub parser_version: usize,
    /// Pass rate achieved
    pub pass_rate: f32,
    /// Files that passed
    pub files_passed: usize,
    /// Files that failed
    pub files_failed: usize,
    /// Whether this was an early stop
    pub was_early_stopped: bool,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl From<&IterationMetrics> for BacktestIteration {
    fn from(metrics: &IterationMetrics) -> Self {
        Self {
            iteration: metrics.iteration,
            parser_version: metrics.parser_version,
            pass_rate: metrics.pass_rate,
            files_passed: metrics.files_passed,
            files_failed: metrics.files_failed,
            was_early_stopped: false,
            duration_ms: metrics.duration_ms,
        }
    }
}

/// Reasons for terminating the backtest loop
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminationReason {
    /// Target pass rate was achieved
    PassRateAchieved,

    /// Maximum iterations reached
    MaxIterations,

    /// No improvement for N iterations
    Plateau {
        no_improvement_for: usize,
    },

    /// Total duration exceeded
    Timeout,

    /// User requested stop
    UserStopped,

    /// Error occurred
    Error {
        message: String,
    },
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminationReason::PassRateAchieved => write!(f, "Target pass rate achieved"),
            TerminationReason::MaxIterations => write!(f, "Maximum iterations reached"),
            TerminationReason::Plateau { no_improvement_for } => {
                write!(f, "No improvement for {} iterations", no_improvement_for)
            }
            TerminationReason::Timeout => write!(f, "Timeout"),
            TerminationReason::UserStopped => write!(f, "User stopped"),
            TerminationReason::Error { message } => write!(f, "Error: {}", message),
        }
    }
}

/// Result of the backtest loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestLoopResult {
    /// All iterations that were run
    pub iterations: Vec<BacktestIteration>,

    /// Aggregate metrics
    pub metrics: BacktestMetrics,

    /// Why the loop terminated
    pub termination_reason: TerminationReason,

    /// Total duration in milliseconds
    pub total_duration_ms: u64,

    /// Final pass rate
    pub final_pass_rate: f32,
}

impl BacktestLoopResult {
    /// Whether the backtest was successful (achieved target pass rate)
    pub fn is_successful(&self) -> bool {
        matches!(self.termination_reason, TerminationReason::PassRateAchieved)
    }
}

/// Check if the backtest loop should terminate
fn should_terminate(
    history: &[BacktestIteration],
    config: &IterationConfig,
    start_time: Instant,
) -> Option<TerminationReason> {
    if history.is_empty() {
        return None;
    }

    let latest = history.last().unwrap();

    // Check pass rate threshold
    if latest.pass_rate >= config.pass_rate_threshold {
        return Some(TerminationReason::PassRateAchieved);
    }

    // Check max iterations
    if history.len() >= config.max_iterations {
        return Some(TerminationReason::MaxIterations);
    }

    // Check timeout
    if start_time.elapsed() >= Duration::from_secs(config.max_duration_secs) {
        return Some(TerminationReason::Timeout);
    }

    // Check plateau
    if history.len() >= config.plateau_window {
        let recent: Vec<_> = history.iter().rev().take(config.plateau_window).collect();
        let first_rate = recent.last().unwrap().pass_rate;
        let last_rate = recent.first().unwrap().pass_rate;
        let improvement = last_rate - first_rate;

        if improvement < config.improvement_threshold {
            return Some(TerminationReason::Plateau {
                no_improvement_for: config.plateau_window,
            });
        }
    }

    None
}

/// Trait for parsers that can be mutated/updated between iterations
pub trait MutableParser: ParserRunner {
    /// Get the current version
    fn version(&self) -> usize;

    /// Apply fixes based on iteration results (called between iterations)
    /// This is where LLM refinement would be triggered
    fn apply_fixes(&mut self, iteration_result: &BacktestIteration) -> bool;
}

/// Run the backtest loop with automatic parser refinement
pub fn run_backtest_loop<P: MutableParser>(
    parser: &mut P,
    files: Vec<FileInfo>,
    high_failure_table: &HighFailureTable,
    scope_id: &Uuid,
    config: &IterationConfig,
) -> Result<BacktestLoopResult, HighFailureError> {
    let start_time = Instant::now();
    let mut iterations: Vec<BacktestIteration> = Vec::new();
    let mut metrics = BacktestMetrics::new();

    loop {
        let iteration_num = iterations.len() + 1;
        let parser_version = parser.version();

        // Run backtest
        let result = backtest_with_failfast(
            parser,
            files.clone(),
            high_failure_table,
            scope_id,
            parser_version,
            iteration_num,
            &config.failfast_config,
        )?;

        // Extract iteration info
        let iteration = match &result {
            BacktestResult::Complete {
                metrics: iter_metrics,
                ..
            } => {
                let mut iter: BacktestIteration = iter_metrics.into();
                iter.was_early_stopped = false;
                metrics.record_iteration(iter_metrics);
                iter
            }
            BacktestResult::EarlyStopped {
                metrics: iter_metrics,
                ..
            } => {
                let mut iter: BacktestIteration = iter_metrics.into();
                iter.was_early_stopped = true;
                metrics.record_iteration(iter_metrics);
                iter
            }
            BacktestResult::Error { error, .. } => {
                return Ok(BacktestLoopResult {
                    iterations,
                    metrics,
                    termination_reason: TerminationReason::Error {
                        message: error.clone(),
                    },
                    total_duration_ms: start_time.elapsed().as_millis() as u64,
                    final_pass_rate: 0.0,
                });
            }
        };

        iterations.push(iteration.clone());

        // Check termination conditions
        if let Some(reason) = should_terminate(&iterations, config, start_time) {
            let final_pass_rate = iterations.last().map(|i| i.pass_rate).unwrap_or(0.0);

            return Ok(BacktestLoopResult {
                iterations,
                metrics,
                termination_reason: reason,
                total_duration_ms: start_time.elapsed().as_millis() as u64,
                final_pass_rate,
            });
        }

        // Apply fixes for next iteration
        if !parser.apply_fixes(&iteration) {
            // No more fixes possible
            let final_pass_rate = iterations.last().map(|i| i.pass_rate).unwrap_or(0.0);

            return Ok(BacktestLoopResult {
                iterations,
                metrics,
                termination_reason: TerminationReason::Plateau {
                    no_improvement_for: 1,
                },
                total_duration_ms: start_time.elapsed().as_millis() as u64,
                final_pass_rate,
            });
        }
    }
}

/// Simple backtest runner (single iteration, no loop)
pub fn run_single_backtest<P: ParserRunner>(
    parser: &P,
    files: Vec<FileInfo>,
    high_failure_table: &HighFailureTable,
    scope_id: &Uuid,
    parser_version: usize,
    config: &FailFastConfig,
) -> Result<BacktestResult, HighFailureError> {
    backtest_with_failfast(parser, files, high_failure_table, scope_id, parser_version, 1, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failfast::FileTestResult;
    use crate::metrics::FailureCategory;
    use rusqlite::Connection;

    struct TestParser {
        version: usize,
        failing_files: Vec<String>,
        fix_one_per_iteration: bool,
    }

    impl ParserRunner for TestParser {
        fn run(&self, file_path: &str) -> FileTestResult {
            if self.failing_files.contains(&file_path.to_string()) {
                FileTestResult {
                    file_path: file_path.to_string(),
                    passed: false,
                    error: Some("Test failure".to_string()),
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

    impl MutableParser for TestParser {
        fn version(&self) -> usize {
            self.version
        }

        fn apply_fixes(&mut self, _result: &BacktestIteration) -> bool {
            if self.fix_one_per_iteration && !self.failing_files.is_empty() {
                self.failing_files.pop();
                self.version += 1;
                true
            } else {
                false
            }
        }
    }

    fn create_test_table() -> HighFailureTable {
        let conn = Connection::open_in_memory().unwrap();
        HighFailureTable::new(conn).unwrap()
    }

    #[test]
    fn test_should_terminate_pass_rate() {
        let config = IterationConfig {
            pass_rate_threshold: 0.95,
            ..Default::default()
        };

        let history = vec![BacktestIteration {
            iteration: 1,
            parser_version: 1,
            pass_rate: 0.96,
            files_passed: 96,
            files_failed: 4,
            was_early_stopped: false,
            duration_ms: 100,
        }];

        let result = should_terminate(&history, &config, Instant::now());
        assert_eq!(result, Some(TerminationReason::PassRateAchieved));
    }

    #[test]
    fn test_should_terminate_max_iterations() {
        let config = IterationConfig {
            max_iterations: 3,
            ..Default::default()
        };

        let history: Vec<BacktestIteration> = (0..3)
            .map(|i| BacktestIteration {
                iteration: i + 1,
                parser_version: 1,
                pass_rate: 0.5,
                files_passed: 50,
                files_failed: 50,
                was_early_stopped: false,
                duration_ms: 100,
            })
            .collect();

        let result = should_terminate(&history, &config, Instant::now());
        assert_eq!(result, Some(TerminationReason::MaxIterations));
    }

    #[test]
    fn test_should_terminate_plateau() {
        let config = IterationConfig {
            plateau_window: 3,
            improvement_threshold: 0.01,
            ..Default::default()
        };

        // Same pass rate for 3 iterations
        let history: Vec<BacktestIteration> = (0..3)
            .map(|i| BacktestIteration {
                iteration: i + 1,
                parser_version: 1,
                pass_rate: 0.8,
                files_passed: 80,
                files_failed: 20,
                was_early_stopped: false,
                duration_ms: 100,
            })
            .collect();

        let result = should_terminate(&history, &config, Instant::now());
        assert_eq!(
            result,
            Some(TerminationReason::Plateau {
                no_improvement_for: 3
            })
        );
    }

    #[test]
    fn test_loop_achieves_pass_rate() {
        let table = create_test_table();
        let scope_id = Uuid::new_v4();

        // Parser starts with 2 failing files, fixes one per iteration
        let mut parser = TestParser {
            version: 1,
            failing_files: vec!["/path/a.csv".to_string(), "/path/b.csv".to_string()],
            fix_one_per_iteration: true,
        };

        let config = IterationConfig {
            max_iterations: 10,
            pass_rate_threshold: 1.0,
            ..Default::default()
        };

        let files = vec![
            FileInfo::new("/path/a.csv", 100),
            FileInfo::new("/path/b.csv", 100),
            FileInfo::new("/path/c.csv", 100),
        ];

        let result = run_backtest_loop(&mut parser, files, &table, &scope_id, &config).unwrap();

        // Should complete in 3 iterations (start with 2 failing, fix one each time)
        assert_eq!(result.termination_reason, TerminationReason::PassRateAchieved);
        assert_eq!(result.iterations.len(), 3);
        assert!((result.final_pass_rate - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_loop_hits_max_iterations() {
        let table = create_test_table();
        let scope_id = Uuid::new_v4();

        // Parser never improves
        let mut parser = TestParser {
            version: 1,
            failing_files: vec!["/path/a.csv".to_string()],
            fix_one_per_iteration: false,
        };

        let config = IterationConfig {
            max_iterations: 3,
            pass_rate_threshold: 1.0,
            ..Default::default()
        };

        let files = vec![
            FileInfo::new("/path/a.csv", 100),
            FileInfo::new("/path/b.csv", 100),
        ];

        let result = run_backtest_loop(&mut parser, files, &table, &scope_id, &config).unwrap();

        // Should stop after 1 iteration (no fixes possible)
        assert!(matches!(
            result.termination_reason,
            TerminationReason::Plateau { .. }
        ));
    }

    #[test]
    fn test_single_backtest() {
        let table = create_test_table();
        let scope_id = Uuid::new_v4();

        let parser = TestParser {
            version: 1,
            failing_files: vec!["/path/a.csv".to_string()],
            fix_one_per_iteration: false,
        };

        let files = vec![
            FileInfo::new("/path/a.csv", 100),
            FileInfo::new("/path/b.csv", 100),
            FileInfo::new("/path/c.csv", 100),
        ];

        let result = run_single_backtest(
            &parser,
            files,
            &table,
            &scope_id,
            1,
            &FailFastConfig::no_early_stop(),
        )
        .unwrap();

        assert!(result.is_complete());
        assert!((result.pass_rate() - 0.666).abs() < 0.01);
    }
}
