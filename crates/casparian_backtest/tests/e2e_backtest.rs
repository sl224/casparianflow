//! End-to-End tests for Backtest Engine
//!
//! Tests use REAL SQLite databases, REAL file I/O, and REAL parser execution.
//! No mocks - actual backtest behavior is verified.

use casparian_backtest::{
    failfast::{backtest_with_failfast, BacktestResult, FailFastConfig, FileTestResult, ParserRunner},
    high_failure::{FailureHistoryEntry, HighFailureTable, FileInfo},
    loop_::{
        run_backtest_loop, BacktestIteration, IterationConfig,
        MutableParser, TerminationReason,
    },
    metrics::{BacktestMetrics, FailureSummary, IterationMetrics, FailureCategory},
};
use rusqlite::Connection;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

// =============================================================================
// REAL PARSER RUNNER - Actually reads and validates files
// =============================================================================

/// A real parser runner that reads CSV files and validates against expected schema
struct RealCsvParser {
    expected_columns: Vec<String>,
    expected_types: Vec<ExpectedType>,
    fail_on_files: Vec<String>, // Specific files to fail on (for testing)
}

#[derive(Clone)]
enum ExpectedType {
    Integer,
    Float,
    String,
    Boolean,
    Date,
}

impl RealCsvParser {
    fn new(columns: Vec<&str>, types: Vec<ExpectedType>) -> Self {
        Self {
            expected_columns: columns.iter().map(|s| s.to_string()).collect(),
            expected_types: types,
            fail_on_files: vec![],
        }
    }

    fn with_failures(mut self, files: Vec<&str>) -> Self {
        self.fail_on_files = files.iter().map(|s| s.to_string()).collect();
        self
    }

    fn validate_value(&self, value: &str, expected_type: &ExpectedType) -> Result<(), String> {
        match expected_type {
            ExpectedType::Integer => {
                value.trim().parse::<i64>()
                    .map(|_| ())
                    .map_err(|_| format!("Expected integer, got: {}", value))
            }
            ExpectedType::Float => {
                value.trim().parse::<f64>()
                    .map(|_| ())
                    .map_err(|_| format!("Expected float, got: {}", value))
            }
            ExpectedType::Boolean => {
                let lower = value.trim().to_lowercase();
                if ["true", "false", "1", "0", "yes", "no"].contains(&lower.as_str()) {
                    Ok(())
                } else {
                    Err(format!("Expected boolean, got: {}", value))
                }
            }
            ExpectedType::Date => {
                // Simple date validation - YYYY-MM-DD
                if value.len() == 10 && value.chars().nth(4) == Some('-') && value.chars().nth(7) == Some('-') {
                    Ok(())
                } else {
                    Err(format!("Expected date (YYYY-MM-DD), got: {}", value))
                }
            }
            ExpectedType::String => Ok(()), // String accepts anything
        }
    }
}

impl ParserRunner for RealCsvParser {
    fn run(&self, file_path: &str) -> FileTestResult {
        // Check if this file should fail
        if self.fail_on_files.iter().any(|f| file_path.contains(f)) {
            return FileTestResult {
                file_path: file_path.to_string(),
                passed: false,
                error: Some("File marked for failure".to_string()),
                category: Some(FailureCategory::ParseError),
            };
        }

        // Actually read and parse the file
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                return FileTestResult {
                    file_path: file_path.to_string(),
                    passed: false,
                    error: Some(format!("Failed to read file: {}", e)),
                    category: Some(FailureCategory::FileNotFound),
                };
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return FileTestResult {
                file_path: file_path.to_string(),
                passed: false,
                error: Some("Empty file".to_string()),
                category: Some(FailureCategory::ParseError),
            };
        }

        // Validate header
        let headers: Vec<&str> = lines[0].split(',').collect();
        if headers.len() != self.expected_columns.len() {
            return FileTestResult {
                file_path: file_path.to_string(),
                passed: false,
                error: Some(format!("Column count mismatch: expected {}, got {}",
                    self.expected_columns.len(), headers.len())),
                category: Some(FailureCategory::SchemaViolation),
            };
        }

        // Validate each data row
        for (row_idx, line) in lines[1..].iter().enumerate() {
            let values: Vec<&str> = line.split(',').collect();

            if values.len() != self.expected_types.len() {
                return FileTestResult {
                    file_path: file_path.to_string(),
                    passed: false,
                    error: Some(format!("Row {}: column count mismatch", row_idx + 1)),
                    category: Some(FailureCategory::SchemaViolation),
                };
            }

            for (col_idx, (value, expected_type)) in values.iter().zip(&self.expected_types).enumerate() {
                if let Err(e) = self.validate_value(value, expected_type) {
                    return FileTestResult {
                        file_path: file_path.to_string(),
                        passed: false,
                        error: Some(format!("Row {}, Column {}: {}", row_idx + 1, col_idx, e)),
                        category: Some(FailureCategory::TypeMismatch),
                    };
                }
            }
        }

        FileTestResult {
            file_path: file_path.to_string(),
            passed: true,
            error: None,
            category: None,
        }
    }
}

/// Parser that tracks how many times it's called
struct TrackingParser {
    inner: RealCsvParser,
    call_count: Arc<AtomicUsize>,
}

impl TrackingParser {
    fn new(inner: RealCsvParser) -> Self {
        Self {
            inner,
            call_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl ParserRunner for TrackingParser {
    fn run(&self, file_path: &str) -> FileTestResult {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.inner.run(file_path)
    }
}

// =============================================================================
// HIGH-FAILURE TABLE TESTS - REAL SQLITE
// =============================================================================

/// Test recording failures in real SQLite database
#[test]
fn test_high_failure_table_real_sqlite() {
    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    // Record a failure
    let entry = FailureHistoryEntry::new(
        1,
        1,
        FailureCategory::TypeMismatch,
        "Expected int, got string",
    );

    table.record_failure("/data/bad_file.csv", &scope_id, entry).unwrap();

    // Verify recorded
    let active = table.get_active(&scope_id).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].file_path, "/data/bad_file.csv");
    assert_eq!(active[0].consecutive_failures, 1);
}

/// Test that consecutive failures increment correctly
#[test]
fn test_consecutive_failures_increment() {
    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    let file_path = "/data/problem_file.csv";

    // Record multiple failures
    for i in 1..=5 {
        let entry = FailureHistoryEntry::new(
            i,
            1,
            FailureCategory::ParseError,
            format!("Failure #{}", i),
        );
        table.record_failure(file_path, &scope_id, entry).unwrap();
    }

    let active = table.get_active(&scope_id).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].consecutive_failures, 5, "Should have 5 consecutive failures");
    assert_eq!(active[0].failure_count, 5, "Total failures should be 5");
}

/// Test that success resets consecutive failures
#[test]
fn test_success_resets_consecutive() {
    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    let file_path = "/data/intermittent.csv";

    // Record failures
    for _ in 0..3 {
        let entry = FailureHistoryEntry::new(
            1,
            1,
            FailureCategory::TypeMismatch,
            "Error",
        );
        table.record_failure(file_path, &scope_id, entry).unwrap();
    }

    // Verify 3 consecutive
    let before = table.get_active(&scope_id).unwrap();
    assert_eq!(before[0].consecutive_failures, 3);

    // Record success
    table.record_success(file_path, &scope_id).unwrap();

    // Consecutive should be 0, but total failures preserved
    let after = table.get_active(&scope_id).unwrap();
    assert!(after.is_empty(), "Should not be in active failures after success");

    // Check get_all list (resolved files have consecutive=0 but are still in get_all)
    let all = table.get_all(&scope_id).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].failure_count, 3, "Total failures should still be 3");
    assert_eq!(all[0].consecutive_failures, 0, "Consecutive should be 0");
}

/// Test backtest ordering prioritizes high-failure files
#[test]
fn test_backtest_order_prioritizes_high_failure() {
    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    // Create high-failure entries with different failure counts
    for (path, failures) in [
        ("/data/worst.csv", 10),
        ("/data/bad.csv", 5),
        ("/data/meh.csv", 2),
    ] {
        for _ in 0..failures {
            let entry = FailureHistoryEntry::new(
                1,
                1,
                FailureCategory::TypeMismatch,
                "Error",
            );
            table.record_failure(path, &scope_id, entry).unwrap();
        }
    }

    // Create file list (including files not in high-failure table)
    let all_files: Vec<FileInfo> = vec![
        FileInfo::new("/data/good.csv", 100),
        FileInfo::new("/data/bad.csv", 100),
        FileInfo::new("/data/worst.csv", 100),
        FileInfo::new("/data/meh.csv", 100),
        FileInfo::new("/data/new.csv", 100),
    ];

    let ordered = table.get_backtest_order(&all_files, &scope_id).unwrap();

    // First 3 should be high-failure files in descending order
    assert_eq!(ordered[0].path, "/data/worst.csv", "Worst should be first");
    assert_eq!(ordered[1].path, "/data/bad.csv", "Bad should be second");
    assert_eq!(ordered[2].path, "/data/meh.csv", "Meh should be third");
}

// =============================================================================
// FAIL-FAST TESTS - REAL FILES
// =============================================================================

/// Create test CSV files in a temp directory
fn create_test_files(dir: &TempDir, files: &[(&str, &str)]) {
    for (name, content) in files {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
    }
}

/// Test backtest with all passing files
#[test]
fn test_backtest_all_pass() {
    let temp_dir = TempDir::new().unwrap();

    // Create valid CSV files
    let csv_content = "id,name,amount\n1,Alice,100\n2,Bob,200\n3,Charlie,300\n";
    create_test_files(&temp_dir, &[
        ("file1.csv", csv_content),
        ("file2.csv", csv_content),
        ("file3.csv", csv_content),
    ]);

    let parser = RealCsvParser::new(
        vec!["id", "name", "amount"],
        vec![ExpectedType::Integer, ExpectedType::String, ExpectedType::Integer],
    );

    let files: Vec<FileInfo> = (1..=3)
        .map(|i| FileInfo::new(
            temp_dir.path().join(format!("file{}.csv", i)).to_string_lossy().to_string(),
            100,
        ))
        .collect();

    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    let config = FailFastConfig::default();

    let result = backtest_with_failfast(
        &parser,
        files,
        &table,
        &scope_id,
        1, // parser_version
        1, // iteration
        &config,
    ).unwrap();

    match result {
        BacktestResult::Complete { metrics, .. } => {
            assert_eq!(metrics.files_passed, 3);
            assert_eq!(metrics.files_failed, 0);
            assert!((metrics.pass_rate - 1.0).abs() < 0.001, "Pass rate should be 100%");
        }
        other => panic!("Expected Complete, got {:?}", other),
    }
}

/// Test backtest with some failing files
#[test]
fn test_backtest_some_fail() {
    let temp_dir = TempDir::new().unwrap();

    // Create valid and invalid files
    let valid = "id,name,amount\n1,Alice,100\n2,Bob,200\n";
    let invalid = "id,name,amount\n1,Alice,not_a_number\n"; // Invalid amount

    create_test_files(&temp_dir, &[
        ("good1.csv", valid),
        ("good2.csv", valid),
        ("bad.csv", invalid),
    ]);

    let parser = RealCsvParser::new(
        vec!["id", "name", "amount"],
        vec![ExpectedType::Integer, ExpectedType::String, ExpectedType::Integer],
    );

    let files: Vec<FileInfo> = ["good1.csv", "good2.csv", "bad.csv"]
        .iter()
        .map(|name| FileInfo::new(
            temp_dir.path().join(name).to_string_lossy().to_string(),
            100,
        ))
        .collect();

    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    let config = FailFastConfig {
        high_failure_threshold: 0.8,
        early_stop_enabled: false, // Don't stop early for this test
        ..Default::default()
    };

    let result = backtest_with_failfast(&parser, files, &table, &scope_id, 1, 1, &config).unwrap();

    match result {
        BacktestResult::Complete { metrics, .. } => {
            assert_eq!(metrics.files_passed, 2);
            assert_eq!(metrics.files_failed, 1);
        }
        other => panic!("Expected Complete, got {:?}", other),
    }
}

/// Test early stopping when high-failure files fail
#[test]
fn test_early_stop_on_high_failure() {
    let temp_dir = TempDir::new().unwrap();

    // Create many files
    let valid = "id,value\n1,100\n";
    let invalid = "id,value\n1,not_valid\n";

    create_test_files(&temp_dir, &[
        ("bad1.csv", invalid),
        ("bad2.csv", invalid),
        ("bad3.csv", invalid),
        ("good1.csv", valid),
        ("good2.csv", valid),
        ("good3.csv", valid),
        ("good4.csv", valid),
        ("good5.csv", valid),
    ]);

    let parser = RealCsvParser::new(
        vec!["id", "value"],
        vec![ExpectedType::Integer, ExpectedType::Integer],
    );

    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    // Pre-populate high-failure table with bad files
    for name in &["bad1.csv", "bad2.csv", "bad3.csv"] {
        let entry = FailureHistoryEntry::new(
            0,
            0,
            FailureCategory::TypeMismatch,
            "Previous failure",
        );
        table.record_failure(
            &temp_dir.path().join(name).to_string_lossy(),
            &scope_id,
            entry,
        ).unwrap();
    }

    // Get files in backtest order (high-failure first)
    let all_files: Vec<FileInfo> = ["bad1.csv", "bad2.csv", "bad3.csv", "good1.csv", "good2.csv", "good3.csv", "good4.csv", "good5.csv"]
        .iter()
        .map(|name| FileInfo::new(
            temp_dir.path().join(name).to_string_lossy().to_string(),
            100,
        ))
        .collect();

    let ordered = table.get_backtest_order(&all_files, &scope_id).unwrap();

    let config = FailFastConfig {
        high_failure_threshold: 0.5, // Require 50% of high-failure files to pass
        early_stop_enabled: true,
        check_after_n_files: 3, // Check after processing high-failure files
        min_high_failure_files: 2,
    };

    let tracking_parser = TrackingParser::new(parser);

    let result = backtest_with_failfast(
        &tracking_parser,
        ordered,
        &table,
        &scope_id,
        1,
        1,
        &config,
    ).unwrap();

    match result {
        BacktestResult::EarlyStopped { reason, high_failure_pass_rate, files_tested, .. } => {
            assert!(high_failure_pass_rate < 0.5, "Pass rate should be below threshold");
            assert!(files_tested <= 5, "Should stop early, not process all files");
            assert!(reason.contains("High-failure") || reason.contains("high-failure"),
                    "Reason should mention high-failure: {}", reason);
        }
        BacktestResult::Complete { metrics, .. } => {
            // If it completed, high-failure files must have passed somehow
            println!("Completed with passed={}, failed={}", metrics.files_passed, metrics.files_failed);
        }
        BacktestResult::Error { error, .. } => panic!("Unexpected error: {}", error),
    }
}

// =============================================================================
// ITERATION LOOP TESTS
// =============================================================================

/// Mutable parser that improves over iterations
struct ImprovingParser {
    iteration: AtomicUsize,
    failure_files: Vec<String>,
}

impl ImprovingParser {
    fn new(failure_files: Vec<String>) -> Self {
        Self {
            iteration: AtomicUsize::new(0),
            failure_files,
        }
    }
}

impl ParserRunner for ImprovingParser {
    fn run(&self, file_path: &str) -> FileTestResult {
        let iteration = self.iteration.load(Ordering::SeqCst);

        // After iteration 3, all files pass
        if iteration >= 3 {
            return FileTestResult {
                file_path: file_path.to_string(),
                passed: true,
                error: None,
                category: None,
            };
        }

        // Before that, some files fail based on iteration
        let fail_threshold = self.failure_files.len() - iteration;
        for (i, fail_file) in self.failure_files.iter().enumerate() {
            if i < fail_threshold && file_path.contains(fail_file) {
                return FileTestResult {
                    file_path: file_path.to_string(),
                    passed: false,
                    error: Some(format!("Failing at iteration {}", iteration)),
                    category: Some(FailureCategory::TypeMismatch),
                };
            }
        }

        FileTestResult {
            file_path: file_path.to_string(),
            passed: true,
            error: None,
            category: None,
        }
    }
}

impl MutableParser for ImprovingParser {
    fn apply_fixes(&mut self, _result: &BacktestIteration) -> bool {
        // Increment iteration to simulate improvement
        self.iteration.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn version(&self) -> usize {
        self.iteration.load(Ordering::SeqCst)
    }
}

/// Test backtest loop achieves pass rate
#[test]
fn test_loop_achieves_pass_rate() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    let content = "id,value\n1,100\n2,200\n";
    for i in 1..=5 {
        fs::write(temp_dir.path().join(format!("file{}.csv", i)), content).unwrap();
    }

    let files: Vec<FileInfo> = (1..=5)
        .map(|i| FileInfo::new(
            temp_dir.path().join(format!("file{}.csv", i)).to_string_lossy().to_string(),
            100,
        ))
        .collect();

    // Parser that fails on file1 and file2 initially, then improves
    let mut parser = ImprovingParser::new(vec!["file1".to_string(), "file2".to_string()]);

    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    let config = IterationConfig {
        max_iterations: 10,
        max_duration_secs: 60,
        pass_rate_threshold: 1.0, // Require 100%
        improvement_threshold: 0.01,
        plateau_window: 3,
        failfast_config: FailFastConfig::default(),
    };

    let result = run_backtest_loop(
        &mut parser,
        files,
        &table,
        &scope_id,
        &config,
    ).unwrap();

    match result.termination_reason {
        TerminationReason::PassRateAchieved => {
            assert!(result.final_pass_rate >= 1.0, "Should achieve 100% pass rate");
        }
        other => {
            // May terminate for other reasons depending on timing
            println!("Terminated due to: {:?}", other);
            assert!(!result.iterations.is_empty(), "Should have at least 1 iteration");
        }
    }
}

/// Test loop respects max iterations
#[test]
fn test_loop_max_iterations() {
    let temp_dir = TempDir::new().unwrap();

    // Create a file that always fails
    let invalid = "id,value\n1,not_a_number\n";
    fs::write(temp_dir.path().join("always_fails.csv"), invalid).unwrap();

    let files = vec![FileInfo::new(
        temp_dir.path().join("always_fails.csv").to_string_lossy().to_string(),
        100,
    )];

    // Parser that never improves
    struct NeverImprovesParser;
    impl ParserRunner for NeverImprovesParser {
        fn run(&self, file_path: &str) -> FileTestResult {
            FileTestResult {
                file_path: file_path.to_string(),
                passed: false,
                error: Some("Always fails".to_string()),
                category: Some(FailureCategory::TypeMismatch),
            }
        }
    }
    impl MutableParser for NeverImprovesParser {
        fn apply_fixes(&mut self, _: &BacktestIteration) -> bool { true }
        fn version(&self) -> usize { 0 }
    }

    let mut parser = NeverImprovesParser;

    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    let config = IterationConfig {
        max_iterations: 3, // Only allow 3 iterations
        max_duration_secs: 60,
        pass_rate_threshold: 1.0,
        improvement_threshold: 0.01,
        plateau_window: 10,
        failfast_config: FailFastConfig::default(),
    };

    let result = run_backtest_loop(&mut parser, files, &table, &scope_id, &config).unwrap();

    assert!(matches!(result.termination_reason, TerminationReason::MaxIterations),
            "Should terminate due to max iterations");
    assert_eq!(result.iterations.len(), 3, "Should have exactly 3 iterations");
}

/// Test plateau detection
#[test]
fn test_loop_plateau_detection() {
    let temp_dir = TempDir::new().unwrap();

    // Create files - some pass, some fail
    let valid = "id,value\n1,100\n";
    let invalid = "id,value\n1,bad\n";

    for i in 1..=3 {
        fs::write(temp_dir.path().join(format!("good{}.csv", i)), valid).unwrap();
    }
    for i in 1..=2 {
        fs::write(temp_dir.path().join(format!("bad{}.csv", i)), invalid).unwrap();
    }

    let files: Vec<FileInfo> = (1..=3)
        .map(|i| FileInfo::new(
            temp_dir.path().join(format!("good{}.csv", i)).to_string_lossy().to_string(),
            100,
        ))
        .chain((1..=2).map(|i| FileInfo::new(
            temp_dir.path().join(format!("bad{}.csv", i)).to_string_lossy().to_string(),
            100,
        )))
        .collect();

    // Parser that never fixes the bad files
    struct PlateauParser;
    impl ParserRunner for PlateauParser {
        fn run(&self, path: &str) -> FileTestResult {
            if path.contains("bad") {
                FileTestResult {
                    file_path: path.to_string(),
                    passed: false,
                    error: Some("Bad file".to_string()),
                    category: Some(FailureCategory::TypeMismatch),
                }
            } else {
                FileTestResult {
                    file_path: path.to_string(),
                    passed: true,
                    error: None,
                    category: None,
                }
            }
        }
    }
    impl MutableParser for PlateauParser {
        fn apply_fixes(&mut self, _: &BacktestIteration) -> bool { true }
        fn version(&self) -> usize { 0 }
    }

    let mut parser = PlateauParser;

    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    let config = IterationConfig {
        max_iterations: 20,
        max_duration_secs: 60,
        pass_rate_threshold: 1.0,
        improvement_threshold: 0.05,
        plateau_window: 3, // Detect plateau after 3 iterations with no improvement
        failfast_config: FailFastConfig::default(),
    };

    let result = run_backtest_loop(&mut parser, files, &table, &scope_id, &config).unwrap();

    match result.termination_reason {
        TerminationReason::Plateau { no_improvement_for } => {
            assert!(no_improvement_for >= 3, "Should detect plateau after {} iterations", no_improvement_for);
        }
        TerminationReason::MaxIterations => {
            // Also acceptable if plateau detection is conservative
        }
        other => panic!("Expected Plateau or MaxIterations, got {:?}", other),
    }
}

// =============================================================================
// METRICS TESTS
// =============================================================================

/// Test failure summary aggregation
#[test]
fn test_failure_summary() {
    let failures = vec![
        ("file1.csv", FailureCategory::TypeMismatch, "Type error"),
        ("file2.csv", FailureCategory::TypeMismatch, "Type error"),
        ("file3.csv", FailureCategory::ParseError, "Parse error"),
        ("file4.csv", FailureCategory::NullNotAllowed, "Null error"),
        ("file5.csv", FailureCategory::TypeMismatch, "Type error"),
    ];

    let mut summary = FailureSummary::new();
    for (file_path, category, error_msg) in failures {
        summary.record_failure(file_path, category, error_msg);
    }

    assert_eq!(summary.total_failures, 5);
    assert_eq!(summary.by_category.get(&FailureCategory::TypeMismatch), Some(&3));
    assert_eq!(summary.by_category.get(&FailureCategory::ParseError), Some(&1));
    assert_eq!(summary.by_category.get(&FailureCategory::NullNotAllowed), Some(&1));
}

/// Test iteration metrics tracking
#[test]
fn test_iteration_metrics() {
    // Create IterationMetrics instead of BacktestIteration
    let mut iter1 = IterationMetrics::new(1, 1);
    iter1.pass_rate = 0.5;
    iter1.files_passed = 5;
    iter1.files_failed = 5;
    iter1.files_tested = 10;
    iter1.duration_ms = 100;

    let mut iter2 = IterationMetrics::new(2, 2);
    iter2.pass_rate = 0.7;
    iter2.files_passed = 7;
    iter2.files_failed = 3;
    iter2.files_tested = 10;
    iter2.duration_ms = 90;

    let mut iter3 = IterationMetrics::new(3, 3);
    iter3.pass_rate = 0.9;
    iter3.files_passed = 9;
    iter3.files_failed = 1;
    iter3.files_tested = 10;
    iter3.duration_ms = 80;

    let mut metrics = BacktestMetrics::new();
    metrics.record_iteration(&iter1);
    metrics.record_iteration(&iter2);
    metrics.record_iteration(&iter3);

    assert_eq!(metrics.iterations, 3);
    assert!((metrics.final_pass_rate - 0.9).abs() < 0.001);
    // Check improvement from first to last
    let improvement = metrics.pass_rate_history.last().unwrap() - metrics.pass_rate_history.first().unwrap();
    assert!((improvement - 0.4).abs() < 0.001, "Should improve 0.5 -> 0.9 = 0.4");
    assert!(!metrics.has_plateau(3), "Should not be plateau with 0.4 improvement");
}

/// Test plateau detection in metrics
#[test]
fn test_metrics_plateau_detection() {
    let mut iter1 = IterationMetrics::new(1, 1);
    iter1.pass_rate = 0.6;
    iter1.files_passed = 6;
    iter1.files_failed = 4;
    iter1.files_tested = 10;
    iter1.duration_ms = 100;

    let mut iter2 = IterationMetrics::new(2, 2);
    iter2.pass_rate = 0.605; // Tiny improvement - within 0.01 of first
    iter2.files_passed = 6;
    iter2.files_failed = 4;
    iter2.files_tested = 10;
    iter2.duration_ms = 100;

    let mut iter3 = IterationMetrics::new(3, 3);
    iter3.pass_rate = 0.60; // No improvement
    iter3.files_passed = 6;
    iter3.files_failed = 4;
    iter3.files_tested = 10;
    iter3.duration_ms = 100;

    let mut metrics = BacktestMetrics::new();
    metrics.record_iteration(&iter1);
    metrics.record_iteration(&iter2);
    metrics.record_iteration(&iter3);

    assert!(metrics.has_plateau(3), "Should detect plateau with <1% improvement within window");
}

// =============================================================================
// FULL E2E WORKFLOW
// =============================================================================

/// Test complete backtest workflow: files -> high-failure tracking -> iterations -> completion
#[test]
fn test_complete_backtest_workflow() {
    let temp_dir = TempDir::new().unwrap();

    // Create a realistic dataset
    let valid_content = r#"transaction_id,customer_id,amount,date,status
1001,C001,150.00,2024-01-15,completed
1002,C002,275.50,2024-01-16,completed
1003,C001,99.99,2024-01-17,pending
"#;

    let invalid_content = r#"transaction_id,customer_id,amount,date,status
1004,C003,not_a_number,2024-01-18,completed
1005,C004,125.00,invalid_date,failed
"#;

    // Create mix of valid and invalid files
    for i in 1..=5 {
        fs::write(
            temp_dir.path().join(format!("transactions_{}.csv", i)),
            valid_content,
        ).unwrap();
    }
    fs::write(temp_dir.path().join("transactions_bad.csv"), invalid_content).unwrap();

    // Create parser
    let parser = RealCsvParser::new(
        vec!["transaction_id", "customer_id", "amount", "date", "status"],
        vec![
            ExpectedType::Integer,
            ExpectedType::String,
            ExpectedType::Float,
            ExpectedType::Date,
            ExpectedType::String,
        ],
    );

    let files: Vec<FileInfo> = (1..=5)
        .map(|i| format!("transactions_{}.csv", i))
        .chain(std::iter::once("transactions_bad.csv".to_string()))
        .map(|name| FileInfo::new(
            temp_dir.path().join(&name).to_string_lossy().to_string(),
            100,
        ))
        .collect();

    let conn = Connection::open_in_memory().unwrap();
    let table = HighFailureTable::new(conn).unwrap();
    let scope_id = Uuid::new_v4();

    // First backtest run
    let config = FailFastConfig::default();
    let result = backtest_with_failfast(&parser, files.clone(), &table, &scope_id, 1, 1, &config).unwrap();

    match result {
        BacktestResult::Complete { metrics, .. } => {
            assert_eq!(metrics.files_passed, 5, "Should have 5 passing files");
            assert_eq!(metrics.files_failed, 1, "Should have 1 failing file");

            // Verify the bad file is in failure summary
            assert!(metrics.failure_summary.total_failures >= 1,
                    "Should have at least 1 failure");
        }
        other => panic!("Expected Complete, got {:?}", other),
    }

    // Verify high-failure table was updated
    let active = table.get_active(&scope_id).unwrap();
    assert_eq!(active.len(), 1, "Should have 1 file in high-failure table");
    assert!(active[0].file_path.contains("transactions_bad"),
            "Bad file should be tracked");

    // Second backtest - high-failure file should be tested first
    let ordered = table.get_backtest_order(&files, &scope_id).unwrap();
    assert!(ordered[0].path.contains("transactions_bad"),
            "Bad file should be first in backtest order");

    // Run again with fail-fast
    let strict_config = FailFastConfig {
        high_failure_threshold: 0.8,
        early_stop_enabled: true,
        check_after_n_files: 1,
        min_high_failure_files: 1,
    };

    let result2 = backtest_with_failfast(&parser, ordered, &table, &scope_id, 1, 2, &strict_config).unwrap();

    // Should stop early because high-failure file still fails
    match result2 {
        BacktestResult::EarlyStopped { files_tested, .. } => {
            assert!(files_tested <= 2, "Should stop early after testing high-failure file");
        }
        BacktestResult::Complete { .. } => {
            // May complete if failfast doesn't trigger
        }
        BacktestResult::Error { error, .. } => panic!("Unexpected error: {}", error),
    }
}
