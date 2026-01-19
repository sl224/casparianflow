# Claude Code Instructions for casparian_backtest

## Quick Reference

```bash
cargo test -p casparian_backtest              # All tests
cargo test -p casparian_backtest --test e2e_backtest  # E2E tests
```

---

## Overview

`casparian_backtest` implements the **Backtest Engine** - a multi-file validation system with fail-fast optimization. It ensures parsers work correctly across the variety of real-world data before deployment.

### The Philosophy

**High-Failure Files First.**

Traditional testing: Run all files, see what fails, fix, repeat.

Our approach:
1. Track which files have **historically failed**
2. Test those files **first** in each iteration
3. If they still fail, **stop early** (parser not ready)
4. If they pass, continue with remaining files

This provides rapid feedback during parser development.

---

## Key Concepts

### High-Failure Table

SQLite-backed tracking of files that consistently fail:

```rust
pub struct HighFailureFile {
    pub file_id: FileId,
    pub file_path: String,
    pub scope_id: ScopeId,
    pub failure_count: usize,         // Total failures ever
    pub consecutive_failures: usize,  // Resets on success
    pub first_failure_at: DbTimestamp,
    pub last_failure_at: DbTimestamp,
    pub last_tested_at: DbTimestamp,
    pub failure_history: Vec<FailureHistoryEntry>,
}
```

### Failure History

Each failure is tracked with context:

```rust
pub struct FailureHistoryEntry {
    pub iteration: usize,         // Which backtest iteration
    pub parser_version: usize,    // Parser version at failure
    pub failure_category: FailureCategory,
    pub error_message: String,
    pub resolved: bool,           // Was this later fixed?
    pub resolved_by: Option<String>,
    pub occurred_at: DbTimestamp,
}
```

### Failure Categories

```rust
pub enum FailureCategory {
    TypeMismatch,      // Wrong data type
    NullNotAllowed,    // Null in required column
    ParseError,        // Couldn't parse file
    SchemaViolation,   // Column count/name mismatch
    Timeout,           // Took too long
    Unknown,           // Uncategorized
}
```

---

## Backtest Loop

### Algorithm

```
1. Get all files in scope
2. Sort by: high-failure first, then resolved, then untested, then passing
3. For each file:
   a. Run parser
   b. Record result (pass/fail)
   c. Update high-failure table
   d. Check early termination conditions
4. Calculate metrics
5. Determine if more iterations needed
```

### Test Order Priority

Files are tested in this order:

1. **High-failure** (sorted by consecutive_failures DESC)
2. **Resolved** (previously failed, now passing)
3. **Untested** (never been tested)
4. **Passing** (tested and always passed)

### Early Termination

Stop the backtest early if:
- All high-failure files still fail (parser not ready)
- Pass rate drops below threshold after N files
- Timeout reached

---

## Termination Conditions

```rust
pub enum TerminationReason {
    PassRateAchieved,    // Hit target (e.g., 95%)
    MaxIterations,       // Reached max iterations
    PlateauDetected,     // No improvement for N iterations
    Timeout,             // Time limit exceeded
    UserStopped,         // Manual cancellation
    HighFailureEarlyStop, // High-failure files still failing
}
```

### Plateau Detection

Detects when improvements have stalled:

```rust
pub struct PlateauConfig {
    pub window_size: usize,    // How many iterations to compare
    pub min_improvement: f64,  // Minimum pass rate improvement
}

// Example: Stop if pass rate hasn't improved by 1% in 3 iterations
let config = PlateauConfig {
    window_size: 3,
    min_improvement: 0.01,
};
```

---

## Usage

### Basic Backtest

```rust
use casparian_backtest::{BacktestRunner, BacktestConfig, ParserRunner};

// Implement parser runner
struct MyParser;

impl ParserRunner for MyParser {
    fn run(&self, file_path: &str) -> FileTestResult {
        // Parse the file, return result
        FileTestResult {
            file_path: file_path.to_string(),
            passed: true,
            error: None,
            category: None,
        }
    }
}

// Configure backtest
let config = BacktestConfig {
    pass_threshold: 0.95,    // 95% must pass
    max_iterations: 10,
    early_stop_on_high_failure: true,
    plateau_detection: Some(PlateauConfig {
        window_size: 3,
        min_improvement: 0.01,
    }),
    timeout: Some(Duration::from_secs(300)),
};

// Run backtest
let runner = BacktestRunner::new(config, Box::new(MyParser));
let result = runner.run(&files, &scope_id)?;

match result {
    BacktestResult::Complete { metrics, .. } => {
        println!("Pass rate: {:.1}%", metrics.pass_rate * 100.0);
    }
    BacktestResult::EarlyStopped { reason, metrics, .. } => {
        println!("Stopped early: {:?}", reason);
    }
}
```

### Backtest Loop (Multiple Iterations)

```rust
use casparian_backtest::{BacktestLoop, LoopConfig};

let loop_config = LoopConfig {
    target_pass_rate: 0.99,   // Keep going until 99%
    max_iterations: 20,
    plateau_window: 5,
    min_improvement: 0.005,
};

let mut loop_runner = BacktestLoop::new(loop_config, parser);

while !loop_runner.should_stop() {
    let iteration = loop_runner.run_iteration(&files, &scope_id)?;

    println!("Iteration {}: {:.1}% pass rate",
        iteration.number,
        iteration.metrics.pass_rate * 100.0
    );

    if iteration.failures.is_empty() {
        break;  // All files pass!
    }

    // Fix parser based on failures...
}
```

---

## High-Failure Table API

```rust
use casparian_backtest::{HighFailureTable, FailureHistoryEntry, FailureCategory};
use rusqlite::Connection;

// Create table
let conn = Connection::open("backtest.db")?;
let table = HighFailureTable::new(conn)?;

// Record a failure
let entry = FailureHistoryEntry::new(
    1,                        // iteration
    1,                        // parser_version
    FailureCategory::TypeMismatch,
    "Expected Int64, got String in column 'amount'",
);
table.record_failure("/path/to/file.csv", &scope_id, entry)?;

// Record success (resets consecutive failures)
table.record_success("/path/to/file.csv", &scope_id)?;

// Get files in backtest order
let ordered = table.get_backtest_order(&all_files, &scope_id)?;

// Get active high-failure files
let high_failure = table.get_active(&scope_id)?;

// Clear scope (fresh start)
table.clear_scope(&scope_id)?;
```

---

## Metrics

### IterationMetrics

```rust
pub struct IterationMetrics {
    pub total_files: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub duration: Duration,
    pub high_failure_tested: usize,
    pub high_failure_passed: usize,
}
```

### Failure Summary

```rust
pub struct FailureSummary {
    pub by_category: HashMap<FailureCategory, usize>,
    pub by_file: Vec<FileFailure>,
    pub most_common_error: Option<String>,
}
```

---

## Common Tasks

### Add a New Failure Category

1. Add variant to `FailureCategory`:
```rust
pub enum FailureCategory {
    // ... existing
    EncodingError,  // File encoding issues
}
```

2. Implement Display:
```rust
impl fmt::Display for FailureCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FailureCategory::EncodingError => write!(f, "Encoding Error"),
            // ...
        }
    }
}
```

3. Update categorization logic in your parser

### Implement Custom Early Termination

```rust
struct CustomTerminator {
    error_threshold: usize,  // Stop after N consecutive errors
}

impl EarlyTerminator for CustomTerminator {
    fn should_stop(&self, metrics: &IterationMetrics, errors: &[FileFailure]) -> Option<TerminationReason> {
        let consecutive = count_consecutive_failures(errors);
        if consecutive >= self.error_threshold {
            Some(TerminationReason::Custom("Too many consecutive failures".to_string()))
        } else {
            None
        }
    }
}
```

---

## Testing

### Unit Tests

```rust
#[test]
fn test_consecutive_failures_increment() {
    let table = create_test_table();
    let scope_id = ScopeId::new();

    // First failure
    let entry = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error");
    table.record_failure("/file.csv", &scope_id, entry.clone()).unwrap();

    // Second failure
    table.record_failure("/file.csv", &scope_id, entry).unwrap();

    let files = table.get_active(&scope_id).unwrap();
    assert_eq!(files[0].consecutive_failures, 2);
}

#[test]
fn test_success_resets_consecutive() {
    let table = create_test_table();
    let scope_id = ScopeId::new();

    // Record failures
    let entry = FailureHistoryEntry::new(1, 1, FailureCategory::TypeMismatch, "Error");
    table.record_failure("/file.csv", &scope_id, entry).unwrap();

    // Record success
    table.record_success("/file.csv", &scope_id).unwrap();

    // Should have no active high-failure files
    let active = table.get_active(&scope_id).unwrap();
    assert!(active.is_empty());
}
```

### E2E Tests

```rust
#[test]
fn test_complete_backtest_workflow() {
    let temp_dir = tempdir().unwrap();
    create_test_files(&temp_dir);

    let config = BacktestConfig {
        pass_threshold: 0.80,
        max_iterations: 5,
        early_stop_on_high_failure: true,
        ..Default::default()
    };

    let parser = TestParser::new();
    let runner = BacktestRunner::new(config, Box::new(parser));

    let files = get_test_files(&temp_dir);
    let result = runner.run(&files, &ScopeId::new()).unwrap();

    match result {
        BacktestResult::Complete { metrics, .. } => {
            assert!(metrics.pass_rate >= 0.80);
        }
        _ => panic!("Expected complete result"),
    }
}
```

---

## File Structure

```
casparian_backtest/
├── CLAUDE.md           # This file
├── Cargo.toml
├── src/
│   ├── lib.rs          # Crate root, exports
│   ├── high_failure.rs # High-failure table
│   ├── failfast.rs     # Early termination logic
│   ├── loop_.rs        # Backtest loop
│   └── metrics.rs      # Metrics and categories
└── tests/
    └── e2e_backtest.rs # E2E tests (14 tests)
```

---

## Key Principles

1. **Fail fast** - Test problematic files first
2. **Track history** - Learn from past failures
3. **Detect plateaus** - Stop when improvements stall
4. **Rapid feedback** - Know quickly if parser is ready
5. **Comprehensive metrics** - Understand failure patterns
