//! Bridge Tests
//!
//! Tests that verify the bridge correctly executes parsers and streams data.
//! Priority: Success paths first, then critical failure paths.

use casparian_protocol::JobId;
use casparian_worker::bridge::{execute_bridge, materialize_bridge_shim, BridgeConfig};
use std::path::PathBuf;

fn make_test_config(job_id: JobId, python_code: &str) -> BridgeConfig {
    let shim_path = materialize_bridge_shim().expect("Failed to materialize shim");
    let interpreter_path =
        PathBuf::from(std::env::var("PYTHON_PATH").unwrap_or_else(|_| "python3".to_string()));

    BridgeConfig {
        interpreter_path,
        source_code: python_code.to_string(),
        file_path: "test.py".to_string(),
        job_id,
        file_id: 1,
        shim_path,
        inherit_stdio: false,
    }
}

// =============================================================================
// SUCCESS PATH TESTS - These are the critical paths
// =============================================================================

/// Test multi-batch streaming - the primary use case.
/// parse() publishes multiple outputs, verify all arrive in order.
#[test]
fn test_multi_batch_streaming() {
    let code = r#"
import pandas as pd
from casparian_types import Output

def parse(file_path):
    outputs = []
    # Publish 5 outputs with sequential IDs
    for i in range(5):
        df = pd.DataFrame({
            "batch_id": [i],
            "value": [i * 100]
        })
        outputs.append(Output("output", df))
    return outputs
"#;

    let config = make_test_config(JobId::new(1001), code);
    let result = execute_bridge(config);

    assert!(
        result.is_ok(),
        "Multi-batch should succeed: {:?}",
        result.err()
    );

    let bridge_result = result.unwrap();
    assert_eq!(
        bridge_result.output_batches.len(),
        5,
        "Should have 5 outputs"
    );

    // Verify batches arrived in order with correct data
    for (i, output) in bridge_result.output_batches.iter().enumerate() {
        assert_eq!(output.len(), 1, "Each output should have one batch");
        let batch = &output[0];
        assert_eq!(batch.num_rows(), 1);

        let batch_id_col = batch.as_record_batch().column(0);
        let batch_id: i64 = batch_id_col
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .expect("batch_id should be i64")
            .value(0);

        assert_eq!(batch_id, i as i64, "Batch {} should have batch_id {}", i, i);
    }
}

/// Test data integrity - exact values survive the round trip.
#[test]
fn test_data_integrity() {
    let code = r#"
import pandas as pd
import numpy as np

def parse(file_path):
    # Test various data types and edge cases
    df = pd.DataFrame({
        "int_col": [0, -1, 2147483647, -2147483648],
        "float_col": [0.0, -1.5, 3.14159, float('inf')],
        "str_col": ["", "hello", "unicode: 日本語", "with\nnewline"],
        "bool_col": [True, False, True, False],
    })
    return df
"#;

    let config = make_test_config(JobId::new(1002), code);
    let result = execute_bridge(config);

    assert!(
        result.is_ok(),
        "Data integrity test should succeed: {:?}",
        result.err()
    );

    let bridge_result = result.unwrap();
    assert_eq!(bridge_result.output_batches.len(), 1);
    assert_eq!(bridge_result.output_batches[0].len(), 1);

    let batch = &bridge_result.output_batches[0][0];
    assert_eq!(batch.num_rows(), 4);
    assert_eq!(batch.as_record_batch().num_columns(), 4);

    // Verify int column
    let int_col = batch
        .as_record_batch()
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::Int64Array>()
        .expect("int_col should be i64");
    assert_eq!(int_col.value(0), 0);
    assert_eq!(int_col.value(1), -1);
    assert_eq!(int_col.value(2), 2147483647);
    assert_eq!(int_col.value(3), -2147483648);

    // Verify string column
    let str_col = batch
        .as_record_batch()
        .column(2)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .expect("str_col should be string");
    assert_eq!(str_col.value(0), "");
    assert_eq!(str_col.value(1), "hello");
    assert_eq!(str_col.value(2), "unicode: 日本語");
    assert_eq!(str_col.value(3), "with\nnewline");
}

/// Test large batch - verify memory handling with substantial data.
#[test]
fn test_large_batch() {
    let code = r#"
import pandas as pd

def parse(file_path):
    # 10,000 rows - not huge but enough to stress test
    df = pd.DataFrame({
        "id": range(10000),
        "value": [f"row_{i}" for i in range(10000)]
    })
    return df
"#;

    let config = make_test_config(JobId::new(1003), code);
    let result = execute_bridge(config);

    assert!(
        result.is_ok(),
        "Large batch should succeed: {:?}",
        result.err()
    );

    let bridge_result = result.unwrap();
    assert_eq!(bridge_result.output_batches.len(), 1);
    assert_eq!(bridge_result.output_batches[0].len(), 1);
    assert_eq!(bridge_result.output_batches[0][0].num_rows(), 10000);
}

/// Test empty result - handler that processes but produces no output.
#[test]
fn test_empty_result() {
    let code = r#"
def parse(file_path):
    # Filtering logic that results in no output
    return None
"#;

    let config = make_test_config(JobId::new(1004), code);
    let result = execute_bridge(config);

    assert!(result.is_ok(), "Empty result should succeed");
    assert!(result.unwrap().output_batches.is_empty());
}

/// Test concurrent execution - multiple jobs don't interfere.
#[test]
fn test_concurrent_execution() {
    // Two parsers that produce different data
    let code1 = r#"
import pandas as pd
def parse(file_path):
    df = pd.DataFrame({"job": [1], "data": ["from_job_1"]})
    return df
"#;

    let code2 = r#"
import pandas as pd
def parse(file_path):
    df = pd.DataFrame({"job": [2], "data": ["from_job_2"]})
    return df
"#;

    let config1 = make_test_config(JobId::new(2001), code1);
    let config2 = make_test_config(JobId::new(2002), code2);

    let handle1 = std::thread::spawn(move || execute_bridge(config1));
    let handle2 = std::thread::spawn(move || execute_bridge(config2));

    let result1 = handle1
        .join()
        .expect("job 1 panicked")
        .expect("job 1 failed");
    let result2 = handle2
        .join()
        .expect("job 2 panicked")
        .expect("job 2 failed");

    // Verify each job got its own data
    let job1_col = result1.output_batches[0][0]
        .as_record_batch()
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::Int64Array>()
        .unwrap();
    assert_eq!(job1_col.value(0), 1);

    let job2_col = result2.output_batches[0][0]
        .as_record_batch()
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::Int64Array>()
        .unwrap();
    assert_eq!(job2_col.value(0), 2);
}

// =============================================================================
// FAILURE PATH TESTS - Only the critical ones
// =============================================================================

/// Test handler exception - the most common failure mode.
#[test]
fn test_handler_exception() {
    let code = r#"
def parse(file_path):
    raise ValueError("Parser crashed")
"#;

    let config = make_test_config(JobId::new(3001), code);
    let result = execute_bridge(config);

    assert!(result.is_err(), "Should fail on exception");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("ValueError") || err.contains("crashed"),
        "Error: {}",
        err
    );
}

/// Test handler timeout - hung process detection.
#[test]
#[ignore = "Takes 60+ seconds"]
fn test_handler_timeout() {
    let code = r#"
import time
def parse(file_path):
    while True:
        time.sleep(1)
"#;

    let config = make_test_config(JobId::new(3002), code);
    let result = execute_bridge(config);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("TIMEOUT"));
}

/// Test missing dependencies - common deployment issue.
#[test]
fn test_missing_import() {
    let code = r#"
import nonexistent_module_xyz
def parse(file_path):
    return None
"#;

    let config = make_test_config(JobId::new(3003), code);
    let result = execute_bridge(config);

    assert!(result.is_err());
}

/// Test socket cleanup on failure.
#[test]
fn test_socket_cleanup() {
    let job_id = JobId::new(4001);
    let socket_path = format!("/tmp/bridge_{}.sock", job_id);
    let _ = std::fs::remove_file(&socket_path);

    let code = r#"
def parse(file_path):
    raise ValueError("fail")
"#;

    let config = make_test_config(job_id, code);
    let _ = execute_bridge(config);

    assert!(
        !std::path::Path::new(&socket_path).exists(),
        "Socket should be cleaned up"
    );
}
