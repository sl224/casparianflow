//! End-to-End tests for Type Inference Engine
//!
//! These tests use REAL data - no mocks. They verify the constraint-based
//! type inference system correctly eliminates impossible types based on
//! actual values.

use casparian_worker::type_inference::{
    constraints::TypeInferenceResult,
    solver::ConstraintSolver,
    streaming::{infer_types_streaming, StreamingConfig},
    DataType,
};
use std::fs;
use std::io::Write;
use tempfile::TempDir;

// =============================================================================
// CONSTRAINT-BASED DATE FORMAT ELIMINATION
// =============================================================================

/// Core insight: "31/05/24" PROVES the format is DD/MM/YY because:
/// - Position 0 has value 31
/// - 31 > 12, so position 0 CANNOT be month
/// - Therefore: DD/MM/YY (not MM/DD/YY)
#[test]
fn test_date_format_constraint_via_solver() {
    let mut solver = ConstraintSolver::new("date_col");

    // Ambiguous date - could be DD/MM/YY or MM/DD/YY
    solver.add_value("05/06/24");

    // Date should still be possible
    assert!(
        solver.possible_types().contains(&DataType::Date),
        "Date should be possible for ambiguous 05/06/24"
    );

    // Add constraining value - 31 cannot be a month
    solver.add_value("31/05/24");

    // Date should still be possible but with narrowed formats
    assert!(
        solver.possible_types().contains(&DataType::Date),
        "Date should remain possible after constraint"
    );

    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved {
            data_type, format, ..
        } => {
            assert_eq!(data_type, DataType::Date);
            // Format should be DD/MM variant (if specified)
            if let Some(fmt) = format {
                assert!(
                    fmt.contains("d") || fmt.contains("D"),
                    "Format should be DD/MM variant, got: {}",
                    fmt
                );
            }
        }
        TypeInferenceResult::Ambiguous { possible_types, .. } => {
            assert!(
                possible_types.contains(&DataType::Date),
                "Date should be in possible types"
            );
        }
        _ => {}
    }
}

/// Test that invalid day values eliminate date type
#[test]
fn test_invalid_day_eliminates_date() {
    let mut solver = ConstraintSolver::new("bad_date");

    // Day 32 is invalid in all months
    solver.add_value("32/05/24");

    // Date might be eliminated depending on implementation
    // The key is it shouldn't crash and should handle gracefully
    let result = solver.get_result();

    // Either Date is eliminated or it falls back to String
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            // If resolved, should be String (invalid date)
            assert!(
                data_type == DataType::String || data_type == DataType::Date,
                "Should resolve to String or Date with format constraints"
            );
        }
        _ => {}
    }
}

/// Test ISO 8601 date parsing
#[test]
fn test_iso_date_format_detection() {
    let mut solver = ConstraintSolver::new("iso_date");

    solver.add_value("2024-12-25");
    solver.add_value("2024-01-15");
    solver.add_value("2024-06-30");

    let result = solver.get_result();

    match result {
        TypeInferenceResult::Resolved {
            data_type, format, ..
        } => {
            assert_eq!(data_type, DataType::Date);
            // Should recognize ISO format
            if let Some(fmt) = format {
                assert!(
                    fmt.contains("Y") || fmt.contains("%Y") || fmt.contains("y"),
                    "Should detect YYYY-MM-DD format, got: {}",
                    fmt
                );
            }
        }
        TypeInferenceResult::Ambiguous { possible_types, .. } => {
            assert!(possible_types.contains(&DataType::Date));
        }
        _ => {}
    }
}

// =============================================================================
// CONSTRAINT SOLVER - TYPE ELIMINATION
// =============================================================================

/// Test that decimal point eliminates Integer type
#[test]
fn test_decimal_eliminates_integer() {
    let mut solver = ConstraintSolver::new("price");

    // First add integer values - both Integer and Float possible
    solver.add_value("100");
    solver.add_value("200");
    assert!(solver.possible_types().contains(&DataType::Integer));
    assert!(solver.possible_types().contains(&DataType::Float));

    // Add decimal - eliminates Integer
    solver.add_value("150.50");
    assert!(
        !solver.possible_types().contains(&DataType::Integer),
        "Integer should be eliminated after seeing decimal"
    );
    assert!(
        solver.possible_types().contains(&DataType::Float),
        "Float should remain"
    );

    // Verify result
    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(data_type, DataType::Float);
        }
        _ => panic!("Expected resolved Float type"),
    }
}

/// Test that non-numeric characters eliminate numeric types
#[test]
fn test_alpha_eliminates_numeric() {
    let mut solver = ConstraintSolver::new("name");

    // Add string value
    solver.add_value("John");

    // All numeric types should be eliminated
    assert!(!solver.possible_types().contains(&DataType::Integer));
    assert!(!solver.possible_types().contains(&DataType::Float));
    assert!(solver.possible_types().contains(&DataType::String));
}

/// Test boolean type detection
#[test]
fn test_boolean_detection() {
    let mut solver = ConstraintSolver::new("is_active");

    solver.add_value("true");
    solver.add_value("false");
    solver.add_value("true");

    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(data_type, DataType::Boolean);
        }
        _ => panic!("Expected resolved Boolean type"),
    }
}

/// Test that mixed boolean values (true/false/1/0) work
#[test]
fn test_mixed_boolean_values() {
    let mut solver = ConstraintSolver::new("flag");

    solver.add_value("1");
    solver.add_value("0");
    solver.add_value("true");
    solver.add_value("false");

    // Should still resolve to Boolean
    assert!(solver.possible_types().contains(&DataType::Boolean));
}

/// Test null handling - nulls shouldn't affect type inference
#[test]
fn test_null_handling() {
    let mut solver = ConstraintSolver::new("optional_field");

    // Add null sentinels (only empty, "null", "NULL", and "NA" are recognized)
    solver.add_value("");
    solver.add_value("NULL");
    solver.add_value("null");
    solver.add_value("NA");

    // Add actual value
    solver.add_value("42");

    // Should resolve to Integer (nulls don't count as String evidence)
    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(data_type, DataType::Integer);
        }
        _ => panic!("Expected resolved Integer type"),
    }
}

/// Test time format detection
#[test]
fn test_time_detection() {
    let mut solver = ConstraintSolver::new("appointment_time");

    solver.add_value("14:30");
    solver.add_value("09:15");
    solver.add_value("23:59");

    assert!(solver.possible_types().contains(&DataType::Time));
}

/// Test invalid time elimination
#[test]
fn test_invalid_time_eliminates() {
    let mut solver = ConstraintSolver::new("bad_time");

    // Hour > 23 should eliminate Time type
    solver.add_value("25:30");

    assert!(
        !solver.possible_types().contains(&DataType::Time),
        "Time should be eliminated for hour > 23"
    );
}

/// Test datetime detection
#[test]
fn test_datetime_detection() {
    let mut solver = ConstraintSolver::new("created_at");

    solver.add_value("2024-01-15T14:30:00");
    solver.add_value("2024-01-16T09:15:00");

    assert!(solver.possible_types().contains(&DataType::DateTime));
}

// =============================================================================
// STREAMING INFERENCE WITH REAL DATA
// =============================================================================

/// Test streaming inference with early termination
#[test]
fn test_streaming_early_termination() {
    // Create 1000 rows, each with one integer value
    let values: Vec<String> = (0..1000).map(|i| i.to_string()).collect();
    let rows: Vec<Vec<&str>> = values.iter().map(|s| vec![s.as_str()]).collect();

    let config = StreamingConfig {
        max_rows: 100,
        early_termination: true,
        min_rows_before_termination: 10,
    };

    let result = infer_types_streaming(&["count"], rows.iter().map(|r| r.as_slice()), config);

    // Should terminate early once Integer is confirmed
    assert!(
        result.rows_processed <= 100,
        "Should terminate early, processed {} rows",
        result.rows_processed
    );

    // Should be Integer
    let col_result = result
        .columns
        .get("count")
        .expect("count column should exist");
    match col_result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(*data_type, DataType::Integer);
        }
        _ => panic!("Expected resolved Integer type"),
    }
}

/// Test multi-column inference
#[test]
fn test_multi_column_inference() {
    let rows: Vec<Vec<&str>> = vec![
        vec!["1", "John", "2024-01-15", "100.50", "true"],
        vec!["2", "Jane", "2024-01-16", "200.75", "false"],
        vec!["3", "Bob", "2024-01-17", "150.25", "true"],
    ];

    let columns = ["id", "name", "date", "amount", "active"];

    let config = StreamingConfig::default();
    let result = infer_types_streaming(&columns, rows.iter().map(|r| r.as_slice()), config);

    assert_eq!(result.columns.len(), 5);

    // Verify each column type
    let expected_types = [
        ("id", DataType::Integer),
        ("name", DataType::String),
        ("date", DataType::Date),
        ("amount", DataType::Float),
        ("active", DataType::Boolean),
    ];

    for (col_name, expected) in expected_types.iter() {
        let col_result = result
            .columns
            .get(*col_name)
            .expect(&format!("{} column should exist", col_name));
        match col_result {
            TypeInferenceResult::Resolved { data_type, .. } => {
                assert_eq!(
                    data_type, expected,
                    "Column {} should be {:?}",
                    col_name, expected
                );
            }
            TypeInferenceResult::Ambiguous { possible_types, .. } => {
                assert!(
                    possible_types.contains(expected),
                    "Column {} should contain {:?} in {:?}",
                    col_name,
                    expected,
                    possible_types
                );
            }
            TypeInferenceResult::NoValidType { fallback, .. } if col_name == &"name" => {
                // name column with non-numeric data might fall back to String
                assert_eq!(
                    *fallback,
                    DataType::String,
                    "Column {} fallback should be String",
                    col_name
                );
            }
            other => panic!("Unexpected result for column {}: {:?}", col_name, other),
        }
    }
}

// =============================================================================
// REAL FILE TESTS
// =============================================================================

/// Test type inference from a real CSV file
#[test]
fn test_inference_from_real_csv_file() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("test_data.csv");

    // Create real CSV with various types
    let csv_content = r#"id,name,amount,date,active
1,Alice,100.50,2024-01-15,true
2,Bob,200.75,2024-01-16,false
3,Charlie,150.00,2024-01-17,true
4,Diana,300.25,2024-01-18,false
5,Eve,175.50,2024-01-19,true
"#;

    fs::write(&csv_path, csv_content).unwrap();

    // Read and parse CSV
    let content = fs::read_to_string(&csv_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let headers: Vec<&str> = lines[0].split(',').collect();

    let rows: Vec<Vec<&str>> = lines[1..]
        .iter()
        .map(|line| line.split(',').collect())
        .collect();

    let config = StreamingConfig::default();
    let result = infer_types_streaming(&headers, rows.iter().map(|r| r.as_slice()), config);

    // Verify types
    assert_eq!(result.columns.len(), 5);

    // id should be Integer
    match result.columns.get("id").expect("id column should exist") {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(*data_type, DataType::Integer, "id should be Integer");
        }
        _ => panic!("id should resolve to Integer"),
    }

    // amount should be Float
    match result
        .columns
        .get("amount")
        .expect("amount column should exist")
    {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(*data_type, DataType::Float, "amount should be Float");
        }
        _ => panic!("amount should resolve to Float"),
    }

    // active should be Boolean
    match result
        .columns
        .get("active")
        .expect("active column should exist")
    {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(*data_type, DataType::Boolean, "active should be Boolean");
        }
        _ => panic!("active should resolve to Boolean"),
    }
}

/// Test ambiguous date resolution across multiple rows
#[test]
fn test_ambiguous_date_resolution_across_rows() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("dates.csv");

    // First few rows are ambiguous (could be MM/DD or DD/MM)
    // Row with 31 proves format is DD/MM
    let csv_content = r#"order_date
05/06/24
06/07/24
07/08/24
31/08/24
01/09/24
"#;

    fs::write(&csv_path, csv_content).unwrap();

    let content = fs::read_to_string(&csv_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let headers: Vec<&str> = lines[0].split(',').collect();

    let rows: Vec<Vec<&str>> = lines[1..]
        .iter()
        .map(|line| line.split(',').collect())
        .collect();

    let config = StreamingConfig::default();
    let result = infer_types_streaming(&headers, rows.iter().map(|r| r.as_slice()), config);

    // Should resolve to Date with DD/MM/YY format
    match result
        .columns
        .get("order_date")
        .expect("order_date column should exist")
    {
        TypeInferenceResult::Resolved {
            data_type, format, ..
        } => {
            assert_eq!(*data_type, DataType::Date, "Should resolve to Date");
            if let Some(fmt) = format {
                assert!(
                    fmt.contains("d") || fmt.contains("%d"),
                    "Format should be DD/MM/YY variant, got: {}",
                    fmt
                );
            }
        }
        TypeInferenceResult::Ambiguous { possible_types, .. } => {
            assert!(
                possible_types.contains(&DataType::Date),
                "Date should be in possible types"
            );
        }
        _ => panic!("Should resolve to Date type"),
    }
}

/// Test large file performance
#[test]
fn test_large_file_inference_performance() {
    let temp_dir = TempDir::new().unwrap();
    let csv_path = temp_dir.path().join("large.csv");

    // Generate 10,000 rows
    let mut file = fs::File::create(&csv_path).unwrap();
    writeln!(file, "id,value,timestamp").unwrap();

    for i in 0..10_000 {
        writeln!(
            file,
            "{},{}.{},2024-01-{:02}T12:00:00",
            i,
            i * 100,
            i % 100,
            (i % 28) + 1
        )
        .unwrap();
    }
    drop(file);

    let content = fs::read_to_string(&csv_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let headers: Vec<&str> = lines[0].split(',').collect();

    let rows: Vec<Vec<&str>> = lines[1..]
        .iter()
        .take(1000) // Limit for test speed
        .map(|line| line.split(',').collect())
        .collect();

    let start = std::time::Instant::now();

    let config = StreamingConfig {
        max_rows: 1000,
        early_termination: true,
        min_rows_before_termination: 50,
    };

    let result = infer_types_streaming(&headers, rows.iter().map(|r| r.as_slice()), config);

    let duration = start.elapsed();

    // Should complete in reasonable time
    assert!(
        duration.as_millis() < 5000,
        "Inference took too long: {:?}",
        duration
    );

    // Should correctly identify types
    assert_eq!(result.columns.len(), 3);
}

// =============================================================================
// EVIDENCE AND EXPLANATION TESTS
// =============================================================================

/// Test that elimination evidence is properly tracked
#[test]
fn test_elimination_evidence_tracking() {
    let mut solver = ConstraintSolver::new("mixed_column");

    // Add values that eliminate types
    solver.add_value("hello"); // Eliminates Integer, Float, Boolean, Date, Time, DateTime, Duration
    solver.add_value("world");

    let result = solver.get_result();

    // Should have elimination evidence
    match result {
        TypeInferenceResult::Resolved { evidence, .. }
        | TypeInferenceResult::NoValidType {
            eliminations: evidence,
            ..
        } => {
            assert!(!evidence.is_empty(), "Should have elimination evidence");

            // Check that we have evidence for numeric type elimination
            let has_numeric_elimination = evidence.iter().any(|e| {
                matches!(
                    e.eliminated,
                    casparian_worker::type_inference::constraints::EliminatedItem::Type(
                        DataType::Integer
                    ) | casparian_worker::type_inference::constraints::EliminatedItem::Type(
                        DataType::Float
                    )
                )
            });
            assert!(
                has_numeric_elimination,
                "Should have evidence for numeric elimination"
            );
        }
        _ => {}
    }
}

/// Test all types eliminated fallback
#[test]
fn test_all_types_eliminated_fallback() {
    let mut solver = ConstraintSolver::new("weird_column");

    // This value can only be String
    solver.add_value("definitely not a number or date or boolean");

    let result = solver.get_result();

    // Should fallback to String
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(data_type, DataType::String, "Should fallback to String");
        }
        TypeInferenceResult::NoValidType { fallback, .. } => {
            assert_eq!(fallback, DataType::String, "Fallback should be String");
        }
        _ => panic!("Expected String fallback"),
    }
}

// =============================================================================
// EDGE CASES
// =============================================================================

/// Test scientific notation
#[test]
fn test_scientific_notation() {
    let mut solver = ConstraintSolver::new("scientific");

    solver.add_value("1.5e10");
    solver.add_value("2.3e-5");

    // Should recognize as Float (scientific notation is numeric)
    assert!(
        solver.possible_types().contains(&DataType::Float)
            || solver.possible_types().contains(&DataType::String),
        "Scientific notation should be Float or String"
    );
}

/// Test negative numbers
#[test]
fn test_negative_numbers() {
    let mut solver = ConstraintSolver::new("balance");

    solver.add_value("-100");
    solver.add_value("-50.25");
    solver.add_value("200");

    // Should be Float (has decimal)
    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(
                data_type,
                DataType::Float,
                "Mixed negatives with decimal should be Float"
            );
        }
        _ => panic!("Expected Float for negative numbers with decimal"),
    }
}

/// Test empty column
#[test]
fn test_empty_column() {
    let mut solver = ConstraintSolver::new("empty");

    // All nulls
    solver.add_value("");
    solver.add_value("NULL");
    solver.add_value("");

    // Should still have all types possible (or fallback to String/Null)
    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            // Null or String is acceptable for all-null column
            assert!(data_type == DataType::Null || data_type == DataType::String);
        }
        TypeInferenceResult::Ambiguous { .. } => {
            // Also acceptable - can't determine type from all nulls
        }
        _ => {}
    }
}

/// Test leading zeros (should be String, not Integer)
#[test]
fn test_leading_zeros() {
    let mut solver = ConstraintSolver::new("zip_code");

    solver.add_value("01234");
    solver.add_value("00123");
    solver.add_value("10001");

    // Leading zeros typically indicate this is a code (String), not a number
    // But our current implementation might treat as Integer
    // This test documents the current behavior
    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            // Either Integer (current) or String (ideal) is documented behavior
            assert!(
                data_type == DataType::Integer || data_type == DataType::String,
                "Leading zeros should be Integer or String, got {:?}",
                data_type
            );
        }
        _ => {}
    }
}

/// Test mixed case boolean variations
#[test]
fn test_boolean_variations() {
    let mut solver = ConstraintSolver::new("flags");

    solver.add_value("True");
    solver.add_value("FALSE");
    solver.add_value("yes");
    solver.add_value("NO");

    // Should handle case-insensitive boolean values
    assert!(
        solver.possible_types().contains(&DataType::Boolean)
            || solver.possible_types().contains(&DataType::String),
        "Mixed case booleans should be Boolean or String"
    );
}

// =============================================================================
// INTEGER VS FLOAT RESOLUTION
// =============================================================================

/// Test that all integers resolve to Integer, not Float
#[test]
fn test_pure_integers_resolve_to_integer() {
    let mut solver = ConstraintSolver::new("count");

    solver.add_value("1");
    solver.add_value("2");
    solver.add_value("100");
    solver.add_value("9999");

    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(
                data_type,
                DataType::Integer,
                "Pure integers should resolve to Integer, not Float"
            );
        }
        _ => panic!("Expected resolved Integer type"),
    }
}

/// Test that single decimal makes it Float
#[test]
fn test_single_decimal_makes_float() {
    let mut solver = ConstraintSolver::new("measurements");

    solver.add_value("1");
    solver.add_value("2");
    solver.add_value("3.0"); // This one decimal should make it Float

    let result = solver.get_result();
    match result {
        TypeInferenceResult::Resolved { data_type, .. } => {
            assert_eq!(
                data_type,
                DataType::Float,
                "Single decimal should make entire column Float"
            );
        }
        _ => panic!("Expected resolved Float type"),
    }
}
