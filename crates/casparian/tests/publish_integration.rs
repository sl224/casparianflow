//! Integration tests for Plugin Publishing
//!
//! These tests verify the REAL publish flow:
//! 1. Create real plugin file on disk
//! 2. Call analyze_plugin (Real I/O + Real AST parsing)
//! 3. Call prepare_publish (Real lockfile generation)
//!
//! Following the "Map Is Not The Territory" principle:
//! - We verify actual filesystem state, not in-memory state
//! - Tests use real files, not mocks

use casparian::publish::{analyze_plugin, prepare_publish};
use std::fs;
use tempfile::TempDir;

/// Helper to create a valid plugin file
fn create_valid_plugin(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let plugin_path = dir.path().join(format!("{}.py", name));
    let source = r#"
import pandas as pd

class Handler:
    def configure(self, context, config):
        """Configure the handler with topic registrations."""
        self.output_handle = context.register_topic("processed_data")
        self.errors_handle = context.register_topic("errors")

    def execute(self, file_path):
        """Process the input file."""
        df = pd.read_csv(file_path)
        # Simple transformation
        df['processed'] = True
        self.context.publish(self.output_handle, df)
        return None
"#;
    fs::write(&plugin_path, source).expect("Failed to write plugin file");
    plugin_path
}

/// Helper to create an invalid plugin file (uses banned imports)
fn create_invalid_plugin(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let plugin_path = dir.path().join(format!("{}.py", name));
    let source = r#"
import os
import subprocess

class Handler:
    def execute(self, file_path):
        # This is banned by Gatekeeper
        os.system("rm -rf /")
        subprocess.run(["echo", "dangerous"])
"#;
    fs::write(&plugin_path, source).expect("Failed to write plugin file");
    plugin_path
}

/// Helper to create a pyproject.toml for uv lock to work
fn create_pyproject(dir: &TempDir) {
    let pyproject = r#"
[project]
name = "test-plugin"
version = "0.1.0"
requires-python = ">=3.10"
dependencies = ["pandas>=2.0"]
"#;
    fs::write(dir.path().join("pyproject.toml"), pyproject).expect("Failed to write pyproject.toml");
}

// ===========================================================================
// analyze_plugin Tests
// ===========================================================================

#[test]
fn test_analyze_valid_plugin_reads_real_file() {
    // Setup: Create a real plugin file on disk
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "my_processor");

    // Act: Analyze the plugin (Real I/O)
    let analysis = analyze_plugin(&plugin_path).expect("analyze_plugin should succeed");

    // Assert: Verify correct extraction from real file
    assert_eq!(analysis.plugin_name, "my_processor");
    assert!(analysis.is_valid, "Valid plugin should pass Gatekeeper");
    assert!(
        analysis.validation_errors.is_empty(),
        "Valid plugin should have no errors"
    );
    assert!(!analysis.source_code.is_empty(), "Should contain source code");
    assert!(!analysis.source_hash.is_empty(), "Should have a source hash");
}

#[test]
fn test_analyze_detects_handler_methods() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "method_test");

    let analysis = analyze_plugin(&plugin_path).unwrap();

    // Should detect configure and execute methods
    assert!(
        analysis.handler_methods.contains(&"configure".to_string()),
        "Should detect configure method"
    );
    assert!(
        analysis.handler_methods.contains(&"execute".to_string()),
        "Should detect execute method"
    );
}

#[test]
fn test_analyze_detects_topic_registrations() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "topic_test");

    let analysis = analyze_plugin(&plugin_path).unwrap();

    // Should detect registered topics
    assert!(
        analysis.detected_topics.contains(&"processed_data".to_string()),
        "Should detect processed_data topic"
    );
    assert!(
        analysis.detected_topics.contains(&"errors".to_string()),
        "Should detect errors topic"
    );
}

#[test]
fn test_analyze_invalid_plugin_fails_gatekeeper() {
    // Setup: Create an invalid plugin with banned imports
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_invalid_plugin(&temp_dir, "bad_plugin");

    // Act: Analyze the plugin
    let analysis = analyze_plugin(&plugin_path).unwrap();

    // Assert: Should fail validation
    assert!(
        !analysis.is_valid,
        "Invalid plugin should fail Gatekeeper validation"
    );
    assert!(
        !analysis.validation_errors.is_empty(),
        "Should have validation errors"
    );

    // The errors should mention the banned imports
    let errors = analysis.validation_errors.join(" ");
    assert!(
        errors.contains("os") || errors.contains("subprocess") || errors.contains("Banned"),
        "Should mention banned imports in errors: {}",
        errors
    );
}

#[test]
fn test_analyze_nonexistent_file_returns_error() {
    let result = analyze_plugin(std::path::Path::new("/nonexistent/plugin.py"));
    assert!(result.is_err(), "Should error for nonexistent file");
}

#[test]
fn test_analyze_checks_for_lockfile() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "lockfile_test");

    // Without lockfile
    let analysis = analyze_plugin(&plugin_path).unwrap();
    assert!(!analysis.has_lockfile, "Should not have lockfile initially");
    assert!(analysis.env_hash.is_none(), "Should have no env_hash without lockfile");

    // Create a lockfile
    fs::write(temp_dir.path().join("uv.lock"), "# fake lockfile content")
        .expect("Failed to write lockfile");

    // With lockfile
    let analysis = analyze_plugin(&plugin_path).unwrap();
    assert!(analysis.has_lockfile, "Should detect lockfile");
    assert!(analysis.env_hash.is_some(), "Should have env_hash with lockfile");
}

#[test]
fn test_analyze_computes_stable_source_hash() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "hash_test");

    // Analyze twice
    let analysis1 = analyze_plugin(&plugin_path).unwrap();
    let analysis2 = analyze_plugin(&plugin_path).unwrap();

    // Same content should produce same hash
    assert_eq!(
        analysis1.source_hash, analysis2.source_hash,
        "Same source should produce same hash"
    );

    // Different content should produce different hash
    fs::write(&plugin_path, "# different content").unwrap();
    let analysis3 = analyze_plugin(&plugin_path).unwrap();
    assert_ne!(
        analysis1.source_hash, analysis3.source_hash,
        "Different source should produce different hash"
    );
}

// ===========================================================================
// prepare_publish Tests
// ===========================================================================

#[test]
fn test_prepare_publish_fails_for_invalid_plugin() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_invalid_plugin(&temp_dir, "invalid_for_publish");
    create_pyproject(&temp_dir);

    // Should fail because plugin doesn't pass validation
    let result = prepare_publish(&plugin_path);
    assert!(result.is_err(), "Should fail to prepare invalid plugin");

    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("validation") || error.contains("Banned"),
        "Error should mention validation failure: {}",
        error
    );
}

#[test]
fn test_prepare_publish_uses_existing_lockfile() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "existing_lock");

    // Create existing lockfile
    let lockfile_content = "# existing uv.lock\nversion = 1";
    fs::write(temp_dir.path().join("uv.lock"), lockfile_content)
        .expect("Failed to write lockfile");

    let artifact = prepare_publish(&plugin_path).unwrap();

    // Should use existing lockfile
    assert!(
        artifact.lockfile_content.contains("existing uv.lock"),
        "Should use existing lockfile content"
    );
    assert!(!artifact.env_hash.is_empty(), "Should compute env_hash from lockfile");
}

#[test]
fn test_prepare_publish_computes_artifact_hash() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "artifact_hash_test");

    // Create lockfile (avoid running uv lock in tests)
    fs::write(temp_dir.path().join("uv.lock"), "# test lockfile").unwrap();

    let artifact = prepare_publish(&plugin_path).unwrap();

    // Should have all hashes computed
    assert!(!artifact.source_hash.is_empty(), "Should have source_hash");
    assert!(!artifact.env_hash.is_empty(), "Should have env_hash");
    assert!(!artifact.artifact_hash.is_empty(), "Should have artifact_hash");

    // Artifact hash should be different from source hash (includes lockfile)
    assert_ne!(
        artifact.source_hash, artifact.artifact_hash,
        "Artifact hash should include lockfile"
    );
}

#[test]
fn test_prepare_publish_preserves_detected_topics() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "topics_preserved");

    fs::write(temp_dir.path().join("uv.lock"), "# test lockfile").unwrap();

    let artifact = prepare_publish(&plugin_path).unwrap();

    // Detected topics should be preserved
    assert!(
        artifact.detected_topics.contains(&"processed_data".to_string()),
        "Should preserve detected topics"
    );
}

// ===========================================================================
// End-to-End Flow Test
// ===========================================================================

#[test]
fn test_publish_flow_end_to_end() {
    // This test simulates the full publish wizard flow:
    // 1. User selects plugin file
    // 2. Frontend calls analyze_plugin_manifest
    // 3. Frontend displays results
    // 4. User clicks publish
    // 5. Frontend calls publish_with_overrides
    // 6. Backend writes to SQLite (not tested here - would need db fixture)

    let temp_dir = TempDir::new().unwrap();
    let plugin_path = create_valid_plugin(&temp_dir, "end_to_end_plugin");
    fs::write(temp_dir.path().join("uv.lock"), "# e2e lockfile").unwrap();

    // Step 1: Analyze (simulates analyze_plugin_manifest command)
    let analysis = analyze_plugin(&plugin_path).expect("Analyze should succeed");

    // Verify analysis results that frontend would display
    assert_eq!(analysis.plugin_name, "end_to_end_plugin");
    assert!(analysis.is_valid);
    assert!(analysis.has_lockfile);
    assert!(!analysis.handler_methods.is_empty());
    assert!(!analysis.detected_topics.is_empty());

    // Step 2: Prepare (simulates part of publish_with_overrides)
    let artifact = prepare_publish(&plugin_path).expect("Prepare should succeed");

    // Verify artifact ready for database insert
    assert_eq!(artifact.plugin_name, "end_to_end_plugin");
    assert!(!artifact.source_code.is_empty());
    assert!(!artifact.source_hash.is_empty());
    assert!(!artifact.lockfile_content.is_empty());
    assert!(!artifact.env_hash.is_empty());
    assert!(!artifact.artifact_hash.is_empty());

    // The actual database operations would be tested in Tauri command tests
    // with a real SQLite fixture, following the "Map Is Not The Territory" principle
}
