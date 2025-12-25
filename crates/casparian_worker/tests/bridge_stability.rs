//! Bridge Tests
//!
//! Tests that verify the bridge correctly executes handlers and streams data.
//! Priority: Success paths first, then critical failure paths.

use casparian_worker::bridge::{execute_bridge, materialize_bridge_shim, BridgeConfig};
use std::path::PathBuf;

fn make_test_config(job_id: u64, python_code: &str) -> BridgeConfig {
    let shim_path = materialize_bridge_shim().expect("Failed to materialize shim");
    let interpreter_path = PathBuf::from(
        std::env::var("PYTHON_PATH").unwrap_or_else(|_| "python3".to_string())
    );

    BridgeConfig {
        interpreter_path,
        source_code: python_code.to_string(),
        file_path: "test.py".to_string(),
        job_id,
        file_version_id: 1,
        shim_path,
    }
}

// =============================================================================
// SUCCESS PATH TESTS - These are the critical paths
// =============================================================================

/// Test multi-batch streaming - the primary use case.
/// Handler publishes multiple batches, verify all arrive in order.
#[tokio::test]
async fn test_multi_batch_streaming() {
    let code = r#"
import pandas as pd

class Handler:
    def configure(self, context, config):
        self.context = context
        self.handle = context.register_topic("output")

    def execute(self, file_path):
        # Publish 5 batches with sequential IDs
        for i in range(5):
            df = pd.DataFrame({
                "batch_id": [i],
                "value": [i * 100]
            })
            self.context.publish(self.handle, df)
        return None
"#;

    let config = make_test_config(1001, code);
    let result = execute_bridge(config).await;

    assert!(result.is_ok(), "Multi-batch should succeed: {:?}", result.err());

    let bridge_result = result.unwrap();
    assert_eq!(bridge_result.batches.len(), 5, "Should have 5 batches");

    // Verify batches arrived in order with correct data
    for (i, batch) in bridge_result.batches.iter().enumerate() {
        assert_eq!(batch.num_rows(), 1);

        let batch_id_col = batch.column(0);
        let batch_id: i64 = batch_id_col
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .expect("batch_id should be i64")
            .value(0);

        assert_eq!(batch_id, i as i64, "Batch {} should have batch_id {}", i, i);
    }
}

/// Test data integrity - exact values survive the round trip.
#[tokio::test]
async fn test_data_integrity() {
    let code = r#"
import pandas as pd
import numpy as np

class Handler:
    def configure(self, context, config):
        self.context = context
        self.handle = context.register_topic("output")

    def execute(self, file_path):
        # Test various data types and edge cases
        df = pd.DataFrame({
            "int_col": [0, -1, 2147483647, -2147483648],
            "float_col": [0.0, -1.5, 3.14159, float('inf')],
            "str_col": ["", "hello", "unicode: 日本語", "with\nnewline"],
            "bool_col": [True, False, True, False],
        })
        self.context.publish(self.handle, df)
        return None
"#;

    let config = make_test_config(1002, code);
    let result = execute_bridge(config).await;

    assert!(result.is_ok(), "Data integrity test should succeed: {:?}", result.err());

    let bridge_result = result.unwrap();
    assert_eq!(bridge_result.batches.len(), 1);

    let batch = &bridge_result.batches[0];
    assert_eq!(batch.num_rows(), 4);
    assert_eq!(batch.num_columns(), 4);

    // Verify int column
    let int_col = batch.column(0)
        .as_any()
        .downcast_ref::<arrow::array::Int64Array>()
        .expect("int_col should be i64");
    assert_eq!(int_col.value(0), 0);
    assert_eq!(int_col.value(1), -1);
    assert_eq!(int_col.value(2), 2147483647);
    assert_eq!(int_col.value(3), -2147483648);

    // Verify string column
    let str_col = batch.column(2)
        .as_any()
        .downcast_ref::<arrow::array::StringArray>()
        .expect("str_col should be string");
    assert_eq!(str_col.value(0), "");
    assert_eq!(str_col.value(1), "hello");
    assert_eq!(str_col.value(2), "unicode: 日本語");
    assert_eq!(str_col.value(3), "with\nnewline");
}

/// Test large batch - verify memory handling with substantial data.
#[tokio::test]
async fn test_large_batch() {
    let code = r#"
import pandas as pd

class Handler:
    def configure(self, context, config):
        self.context = context
        self.handle = context.register_topic("output")

    def execute(self, file_path):
        # 10,000 rows - not huge but enough to stress test
        df = pd.DataFrame({
            "id": range(10000),
            "value": [f"row_{i}" for i in range(10000)]
        })
        self.context.publish(self.handle, df)
        return None
"#;

    let config = make_test_config(1003, code);
    let result = execute_bridge(config).await;

    assert!(result.is_ok(), "Large batch should succeed: {:?}", result.err());

    let bridge_result = result.unwrap();
    assert_eq!(bridge_result.batches.len(), 1);
    assert_eq!(bridge_result.batches[0].num_rows(), 10000);
}

/// Test empty result - handler that processes but produces no output.
#[tokio::test]
async fn test_empty_result() {
    let code = r#"
class Handler:
    def configure(self, context, config):
        pass
    def execute(self, file_path):
        # Filtering logic that results in no output
        return None
"#;

    let config = make_test_config(1004, code);
    let result = execute_bridge(config).await;

    assert!(result.is_ok(), "Empty result should succeed");
    assert!(result.unwrap().batches.is_empty());
}

/// Test concurrent execution - multiple jobs don't interfere.
#[tokio::test]
async fn test_concurrent_execution() {
    use tokio::time::timeout;
    use std::time::Duration;

    // Two handlers that produce different data
    let code1 = r#"
import pandas as pd
class Handler:
    def configure(self, context, config):
        self.context = context
        self.handle = context.register_topic("output")
    def execute(self, file_path):
        df = pd.DataFrame({"job": [1], "data": ["from_job_1"]})
        self.context.publish(self.handle, df)
"#;

    let code2 = r#"
import pandas as pd
class Handler:
    def configure(self, context, config):
        self.context = context
        self.handle = context.register_topic("output")
    def execute(self, file_path):
        df = pd.DataFrame({"job": [2], "data": ["from_job_2"]})
        self.context.publish(self.handle, df)
"#;

    let config1 = make_test_config(2001, code1);
    let config2 = make_test_config(2002, code2);

    let (result1, result2) = tokio::join!(
        timeout(Duration::from_secs(30), execute_bridge(config1)),
        timeout(Duration::from_secs(30), execute_bridge(config2)),
    );

    let result1 = result1.expect("job 1 timed out").expect("job 1 failed");
    let result2 = result2.expect("job 2 timed out").expect("job 2 failed");

    // Verify each job got its own data
    let job1_col = result1.batches[0].column(0)
        .as_any()
        .downcast_ref::<arrow::array::Int64Array>()
        .unwrap();
    assert_eq!(job1_col.value(0), 1);

    let job2_col = result2.batches[0].column(0)
        .as_any()
        .downcast_ref::<arrow::array::Int64Array>()
        .unwrap();
    assert_eq!(job2_col.value(0), 2);
}

// =============================================================================
// FAILURE PATH TESTS - Only the critical ones
// =============================================================================

/// Test handler exception - the most common failure mode.
#[tokio::test]
async fn test_handler_exception() {
    let code = r#"
class Handler:
    def configure(self, context, config):
        pass
    def execute(self, file_path):
        raise ValueError("Handler crashed")
"#;

    let config = make_test_config(3001, code);
    let result = execute_bridge(config).await;

    assert!(result.is_err(), "Should fail on exception");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("ValueError") || err.contains("crashed"), "Error: {}", err);
}

/// Test handler timeout - hung process detection.
#[tokio::test]
#[ignore = "Takes 60+ seconds"]
async fn test_handler_timeout() {
    let code = r#"
import time
class Handler:
    def configure(self, context, config):
        pass
    def execute(self, file_path):
        while True:
            time.sleep(1)
"#;

    let config = make_test_config(3002, code);
    let result = execute_bridge(config).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("TIMEOUT"));
}

/// Test missing dependencies - common deployment issue.
#[tokio::test]
async fn test_missing_import() {
    let code = r#"
import nonexistent_module_xyz
class Handler:
    def execute(self, file_path):
        pass
"#;

    let config = make_test_config(3003, code);
    let result = execute_bridge(config).await;

    assert!(result.is_err());
}

/// Test socket cleanup on failure.
#[tokio::test]
async fn test_socket_cleanup() {
    let job_id = 4001u64;
    let socket_path = format!("/tmp/bridge_{}.sock", job_id);
    let _ = std::fs::remove_file(&socket_path);

    let code = r#"
class Handler:
    def execute(self, file_path):
        raise ValueError("fail")
"#;

    let config = make_test_config(job_id, code);
    let _ = execute_bridge(config).await;

    assert!(!std::path::Path::new(&socket_path).exists(), "Socket should be cleaned up");
}
