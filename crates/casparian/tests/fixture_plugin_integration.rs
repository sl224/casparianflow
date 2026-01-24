//! Integration tests using the fixture plugin
//!
//! Tests basic execution paths using the deterministic fixture plugin.
//! The fixture plugin provides controllable behavior via environment variables.

mod harness;

use casparian_db::DbValue;
use casparian_protocol::{JobId, ProcessingStatus};
use casparian_worker::bridge::{execute_bridge, materialize_bridge_shim, BridgeConfig};
use casparian_worker::cancel::CancellationToken;
use harness::{fixture_plugin_path, fixture_plugin_source, HarnessConfig, TestHarness};
use std::path::PathBuf;

/// Helper to create a BridgeConfig for fixture plugin tests.
/// Note: Tests that use this should run serially (not in parallel) because
/// environment variables are process-global state.
fn make_fixture_bridge_config(job_id: JobId, mode: &str, rows: usize) -> BridgeConfig {
    // Reset environment variables to known state first
    std::env::remove_var("CF_FIXTURE_SLEEP_SECS");
    std::env::remove_var("CF_FIXTURE_ERROR_MSG");

    let shim_path = materialize_bridge_shim().expect("Failed to materialize shim");
    let interpreter_path =
        PathBuf::from(std::env::var("PYTHON_PATH").unwrap_or_else(|_| "python3".to_string()));

    // Set environment variables for the fixture plugin
    std::env::set_var("CF_FIXTURE_MODE", mode);
    std::env::set_var("CF_FIXTURE_ROWS", rows.to_string());

    let source_code = fixture_plugin_source();

    BridgeConfig {
        interpreter_path,
        source_code,
        file_path: "test_input.txt".to_string(),
        job_id,
        file_id: 1,
        shim_path,
        inherit_stdio: false,
        cancel_token: CancellationToken::new(),
    }
}

// =============================================================================
// FIXTURE PLUGIN BRIDGE TESTS
// =============================================================================

/// Test fixture plugin in normal mode produces expected output
#[test]
#[ignore] // Requires serial execution due to global env vars - run with --ignored --test-threads=1
fn test_fixture_plugin_normal_mode() {
    let config = make_fixture_bridge_config(JobId::new(1001), "normal", 10);
    let result = execute_bridge(config);

    assert!(
        result.is_ok(),
        "Fixture plugin normal mode should succeed: {:?}",
        result.err()
    );

    let bridge_result = result.unwrap();

    // Should have one output batch
    assert!(
        !bridge_result.output_batches.is_empty(),
        "Should have at least one output batch"
    );

    // Check the first output
    let first_output = &bridge_result.output_batches[0];
    assert!(!first_output.is_empty(), "Output should have batches");

    // Verify row count
    let total_rows: usize = first_output.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 10, "Should have 10 rows (default)");
}

/// Test fixture plugin with custom row count
#[test]
#[ignore] // Requires serial execution due to global env vars - run with --ignored
fn test_fixture_plugin_custom_rows() {
    let config = make_fixture_bridge_config(JobId::new(1002), "normal", 25);
    let result = execute_bridge(config);

    assert!(
        result.is_ok(),
        "Fixture plugin with custom rows should succeed: {:?}",
        result.err()
    );

    let bridge_result = result.unwrap();

    // Verify row count
    let total_rows: usize = bridge_result
        .output_batches
        .iter()
        .flat_map(|batches| batches.iter())
        .map(|b| b.num_rows())
        .sum();
    assert_eq!(total_rows, 25, "Should have 25 rows");
}

/// Test fixture plugin collision mode adds reserved column
/// This should be detected and potentially rejected by the worker
#[test]
fn test_fixture_plugin_collision_mode() {
    let config = make_fixture_bridge_config(JobId::new(1003), "collision", 5);
    let result = execute_bridge(config);

    // In collision mode, the plugin adds a _cf_job_id column
    // The bridge should execute successfully but the worker should detect this
    assert!(
        result.is_ok(),
        "Fixture plugin collision mode bridge execution should succeed: {:?}",
        result.err()
    );

    let bridge_result = result.unwrap();

    // Check that the reserved column exists in the output
    let first_output = &bridge_result.output_batches[0];
    assert!(!first_output.is_empty(), "Output should have batches");

    let batch = &first_output[0];
    let schema = batch.as_record_batch().schema();

    // In collision mode, the schema should have a _cf_job_id field
    let has_collision_field = schema.fields().iter().any(|f| f.name() == "_cf_job_id");
    assert!(
        has_collision_field,
        "Collision mode should add _cf_job_id column to test lineage collision detection"
    );
}

/// Test fixture plugin error mode raises an exception
#[test]
#[ignore] // Requires serial execution due to global env vars - run with --ignored
fn test_fixture_plugin_error_mode() {
    std::env::set_var("CF_FIXTURE_ERROR_MSG", "Test error from fixture");
    let config = make_fixture_bridge_config(JobId::new(1004), "error", 10);
    let result = execute_bridge(config);

    // Error mode should fail
    assert!(
        result.is_err(),
        "Fixture plugin error mode should fail"
    );

    let err = result.unwrap_err();
    let err_str = err.to_string();

    // The error should contain our message
    assert!(
        err_str.contains("error") || err_str.contains("Error") || err_str.contains("exception"),
        "Error should indicate failure: {}",
        err_str
    );
}

// =============================================================================
// TEST HARNESS TESTS
// =============================================================================

/// Test harness can be created and initialized
#[test]
fn test_harness_init_schema() {
    let harness = TestHarness::new(HarnessConfig::default()).unwrap();
    harness.init_schema().expect("Schema initialization should succeed");

    // Verify tables exist
    let count: i64 = harness
        .conn()
        .query_scalar("SELECT COUNT(*) FROM cf_processing_queue", &[])
        .expect("Queue table should exist");
    assert_eq!(count, 0, "Queue should be empty initially");
}

/// Test harness can create test files and register them
#[test]
fn test_harness_create_test_file() {
    let harness = TestHarness::new(HarnessConfig::default()).unwrap();
    harness.init_schema().expect("Schema init");

    let (file_id, file_path) = harness
        .create_test_file("test.txt", "Hello, World!")
        .expect("File creation should succeed");

    assert!(file_path.exists(), "File should exist");
    assert!(file_id > 0, "File ID should be positive");

    // Verify file is in scout_files
    let count: i64 = harness
        .conn()
        .query_scalar(
            "SELECT COUNT(*) FROM scout_files WHERE id = ?",
            &[DbValue::from(file_id)],
        )
        .expect("Query should succeed");
    assert_eq!(count, 1, "File should be registered");
}

/// Test harness can register plugins
#[test]
fn test_harness_register_plugin() {
    let harness = TestHarness::new(HarnessConfig::default()).unwrap();
    harness.init_schema().expect("Schema init");

    let source = fixture_plugin_source();
    harness
        .register_plugin("fixture_plugin", "1.0.0", &source)
        .expect("Plugin registration should succeed");

    // Verify plugin is registered
    let count: i64 = harness
        .conn()
        .query_scalar(
            "SELECT COUNT(*) FROM cf_plugin_manifest WHERE plugin_name = 'fixture_plugin'",
            &[],
        )
        .expect("Query should succeed");
    assert_eq!(count, 1, "Plugin should be registered");
}

/// Test harness can enqueue jobs
#[test]
fn test_harness_enqueue_job() {
    let harness = TestHarness::new(HarnessConfig::default()).unwrap();
    harness.init_schema().expect("Schema init");

    // Create a test file
    let (file_id, _) = harness
        .create_test_file("input.txt", "test data")
        .expect("File creation");

    // Register the plugin
    harness
        .register_plugin("test_plugin", "1.0.0", "def parse(ctx): pass")
        .expect("Plugin registration");

    // Enqueue a job
    let job_id = harness
        .enqueue_job("test_plugin", file_id)
        .expect("Job enqueue should succeed");

    assert!(job_id.as_u64() > 0, "Job ID should be positive");

    // Verify job is queued
    let status = harness.get_job_status(job_id).expect("Status check");
    assert_eq!(status, ProcessingStatus::Queued, "Job should be queued");
}

/// Test fixture plugin path resolution
#[test]
fn test_fixture_plugin_path_resolution() {
    let path = fixture_plugin_path();

    // The path should point to our fixture plugin
    assert!(
        path.to_string_lossy().contains("fixture_plugin.py"),
        "Path should contain fixture_plugin.py: {:?}",
        path
    );

    // If the file exists (depends on test execution context), verify it
    if path.exists() {
        let content = std::fs::read_to_string(&path).expect("Should read file");
        assert!(content.contains("CF_FIXTURE_MODE"), "Should contain mode handling");
        assert!(content.contains("def parse"), "Should have parse function");
    }
}

// =============================================================================
// FULL PIPELINE INTEGRATION TESTS
// =============================================================================

/// Full pipeline test: sentinel + worker + fixture plugin
///
/// This test spawns a sentinel and worker, enqueues a job, and verifies
/// the job completes successfully.
#[test]
#[ignore] // Ignored by default - run with `cargo test -- --ignored` or explicitly
fn test_full_pipeline_with_fixture_plugin() {
    use std::time::Duration;

    // Initialize tracing for debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("casparian_sentinel=debug".parse().unwrap())
                .add_directive("casparian_worker=debug".parse().unwrap()),
        )
        .with_test_writer()
        .try_init();

    // Use IPC sockets (default) for test isolation
    let config = HarnessConfig::default()
        .with_fixture_mode("normal")
        .with_fixture_rows(5);

    let mut harness = TestHarness::new(config).expect("Harness creation");
    harness.init_schema().expect("Schema init");

    // Register the fixture plugin
    let source = fixture_plugin_source();
    harness
        .register_plugin("fixture_plugin", "1.0.0", &source)
        .expect("Plugin registration");

    // Create a test input file
    let (file_id, _file_path) = harness
        .create_test_file("input.txt", "test input data")
        .expect("File creation");

    // Enqueue a job BEFORE starting (DB connection required)
    let job_id = harness
        .enqueue_job("fixture_plugin", file_id)
        .expect("Job enqueue");

    // Start sentinel and workers (releases DB connection)
    harness.start().expect("Harness start");

    // Wait for job completion (with timeout)
    let result = harness
        .wait_for_job(job_id, Duration::from_secs(60))
        .expect("Job should complete");

    // Verify job succeeded
    assert!(
        result.status == ProcessingStatus::Completed,
        "Job should complete, got status: {:?}, error: {:?}",
        result.status,
        result.error_message
    );

    // Check completion status
    assert!(
        result.completion_status.is_some(),
        "Should have completion status"
    );
}

/// Test harness can start and stop cleanly
#[test]
fn test_harness_start_stop() {
    // Use IPC sockets (default) for test isolation
    let config = HarnessConfig::default();

    let mut harness = TestHarness::new(config).expect("Harness creation");
    harness.init_schema().expect("Schema init");

    // Register a plugin (sentinel needs this for topic configs)
    harness
        .register_plugin("test_plugin", "1.0.0", "def parse(path): return []")
        .expect("Plugin registration");

    // Start should succeed
    harness.start().expect("Harness start");
    assert!(harness.is_started(), "Harness should be started");

    // Drop triggers cleanup - this should not hang or panic
    drop(harness);
}
