//! Plugin Publishing Logic
//!
//! Shared library for publishing plugins to the Sentinel registry.
//! Used by both CLI (`casparian publish`) and Tauri UI.

use anyhow::{Context, Result};
use casparian_security::signing::sha256;
use casparian_security::Gatekeeper;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Result of analyzing a plugin file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAnalysis {
    /// Plugin name (derived from filename)
    pub plugin_name: String,
    /// Source code
    pub source_code: String,
    /// SHA256 hash of source code
    pub source_hash: String,
    /// Whether the plugin passed Gatekeeper validation
    pub is_valid: bool,
    /// Validation errors (empty if valid)
    pub validation_errors: Vec<String>,
    /// Whether a uv.lock file exists in the plugin directory
    pub has_lockfile: bool,
    /// Environment hash (from uv.lock if present)
    pub env_hash: Option<String>,
    /// Detected Handler class methods
    pub handler_methods: Vec<String>,
    /// Detected topic registrations (from configure method)
    pub detected_topics: Vec<String>,
}

/// Analyze a plugin file without deploying it
///
/// This performs:
/// 1. Read source code (Real I/O)
/// 2. Gatekeeper validation (Real AST parsing)
/// 3. Detect Handler methods
/// 4. Check for uv.lock
pub fn analyze_plugin(path: &Path) -> Result<PluginAnalysis> {
    // 1. Read source code (Real I/O)
    let source_code = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read plugin file: {:?}", path))?;

    // 2. Extract plugin name from filename
    let plugin_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid plugin filename"))?
        .to_string();

    // 3. Compute source hash
    let source_hash = sha256(source_code.as_bytes());

    // 4. Validate with Gatekeeper (Real AST parsing)
    let gatekeeper = Gatekeeper::new();
    let validation_result = gatekeeper.validate(&source_code);

    let (is_valid, validation_errors) = match validation_result {
        Ok(_) => (true, vec![]),
        Err(e) => {
            let error_str = e.to_string();
            let errors: Vec<String> = error_str
                .lines()
                .filter(|l| l.starts_with("- ") || l.contains("Banned") || l.contains("error"))
                .map(|s| s.to_string())
                .collect();
            (false, if errors.is_empty() { vec![error_str] } else { errors })
        }
    };

    // 5. Check for uv.lock
    let plugin_dir = path.parent();
    let (has_lockfile, env_hash) = if let Some(dir) = plugin_dir {
        let lockfile_path = dir.join("uv.lock");
        if lockfile_path.exists() {
            let content = std::fs::read_to_string(&lockfile_path).ok();
            let hash = content.map(|c| sha256(c.as_bytes()));
            (true, hash)
        } else {
            (false, None)
        }
    } else {
        (false, None)
    };

    // 6. Detect Handler methods and topics (simple pattern matching)
    let handler_methods = detect_handler_methods(&source_code);
    let detected_topics = detect_topic_registrations(&source_code);

    Ok(PluginAnalysis {
        plugin_name,
        source_code,
        source_hash,
        is_valid,
        validation_errors,
        has_lockfile,
        env_hash,
        handler_methods,
        detected_topics,
    })
}

/// Detect Handler class methods from source code
fn detect_handler_methods(source: &str) -> Vec<String> {
    let mut methods = vec![];

    // Simple pattern matching for def statements after class Handler
    let mut in_handler = false;
    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("class Handler") {
            in_handler = true;
            continue;
        }

        // New class definition ends Handler scope
        if trimmed.starts_with("class ") && !trimmed.starts_with("class Handler") {
            in_handler = false;
        }

        if in_handler && trimmed.starts_with("def ") {
            // Extract method name: def method_name(
            if let Some(name) = trimmed
                .strip_prefix("def ")
                .and_then(|s| s.split('(').next())
            {
                methods.push(name.trim().to_string());
            }
        }
    }

    methods
}

/// Detect topic registrations from context.register_topic calls
fn detect_topic_registrations(source: &str) -> Vec<String> {
    let mut topics = vec![];

    // Look for context.register_topic("topic_name") patterns
    for line in source.lines() {
        if let Some(pos) = line.find("register_topic(") {
            let after = &line[pos + 15..]; // Skip "register_topic("
            // Find the quoted string
            if let Some(quote_start) = after.find('"').or_else(|| after.find('\'')) {
                let quote_char = after.chars().nth(quote_start).unwrap();
                let rest = &after[quote_start + 1..];
                if let Some(quote_end) = rest.find(quote_char) {
                    topics.push(rest[..quote_end].to_string());
                }
            }
        }
    }

    topics
}

/// Options for publishing a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishOptions {
    /// Plugin file path
    pub path: std::path::PathBuf,
    /// Version string (e.g., "1.0.2")
    pub version: String,
    /// Publisher name
    pub publisher_name: String,
    /// Publisher email (optional)
    pub publisher_email: Option<String>,
    /// Override routing pattern (optional)
    pub routing_pattern: Option<String>,
    /// Override routing tag (optional)
    pub routing_tag: Option<String>,
    /// Override topic URI (optional)
    pub topic_uri_override: Option<String>,
}

/// Result of a successful publish
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishReceipt {
    pub plugin_name: String,
    pub version: String,
    pub source_hash: String,
    pub env_hash: String,
    pub artifact_hash: String,
    /// ID of created routing rule (if any)
    pub routing_rule_id: Option<i64>,
    /// ID of created/updated topic config (if any)
    pub topic_config_id: Option<i64>,
}

/// Prepare a plugin for publishing (validates and generates lockfile)
///
/// This performs:
/// 1. Read and validate source
/// 2. Run `uv lock` if needed
/// 3. Compute hashes
/// 4. Return prepared artifact
pub fn prepare_publish(path: &Path) -> Result<PreparedArtifact> {
    // 1. Analyze the plugin
    let analysis = analyze_plugin(path)?;

    if !analysis.is_valid {
        anyhow::bail!(
            "Plugin failed validation:\n{}",
            analysis.validation_errors.join("\n")
        );
    }

    // 2. Get or generate lockfile
    let plugin_dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Plugin file has no parent directory"))?;
    let lockfile_path = plugin_dir.join("uv.lock");

    let lockfile_content = if lockfile_path.exists() {
        std::fs::read_to_string(&lockfile_path)
            .context("Failed to read uv.lock")?
    } else {
        // Run uv lock (Real I/O)
        tracing::info!("No uv.lock found, running `uv lock` in {:?}...", plugin_dir);
        let output = std::process::Command::new("uv")
            .arg("lock")
            .current_dir(plugin_dir)
            .output()
            .context("Failed to run `uv lock` (is uv installed?)")?;

        if !output.status.success() {
            anyhow::bail!(
                "uv lock failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        std::fs::read_to_string(&lockfile_path)
            .context("Failed to read uv.lock after generation")?
    };

    // 3. Compute hashes
    let env_hash = sha256(lockfile_content.as_bytes());
    let artifact_content = format!("{}{}", analysis.source_code, lockfile_content);
    let artifact_hash = sha256(artifact_content.as_bytes());

    Ok(PreparedArtifact {
        plugin_name: analysis.plugin_name,
        source_code: analysis.source_code,
        source_hash: analysis.source_hash,
        lockfile_content,
        env_hash,
        artifact_hash,
        detected_topics: analysis.detected_topics,
    })
}

/// A prepared artifact ready for publishing
#[derive(Debug, Clone)]
pub struct PreparedArtifact {
    pub plugin_name: String,
    pub source_code: String,
    pub source_hash: String,
    pub lockfile_content: String,
    pub env_hash: String,
    pub artifact_hash: String,
    pub detected_topics: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_handler_methods() {
        let source = r#"
class Handler:
    def configure(self, context, config):
        pass

    def execute(self, file_path):
        pass

    def on_error(self, error):
        pass
"#;
        let methods = detect_handler_methods(source);
        assert_eq!(methods, vec!["configure", "execute", "on_error"]);
    }

    #[test]
    fn test_detect_topic_registrations() {
        let source = r#"
class Handler:
    def configure(self, context, config):
        self.handle1 = context.register_topic("output")
        self.handle2 = context.register_topic('errors')
"#;
        let topics = detect_topic_registrations(source);
        assert_eq!(topics, vec!["output", "errors"]);
    }

    #[test]
    fn test_analyze_plugin_valid() {
        let temp_dir = TempDir::new().unwrap();
        let plugin_path = temp_dir.path().join("my_plugin.py");
        std::fs::write(&plugin_path, r#"
import pandas as pd

class Handler:
    def configure(self, context, config):
        self.handle = context.register_topic("processed")

    def execute(self, file_path):
        df = pd.read_csv(file_path)
        self.context.publish(self.handle, df)
"#).unwrap();

        let analysis = analyze_plugin(&plugin_path).unwrap();
        assert_eq!(analysis.plugin_name, "my_plugin");
        assert!(analysis.is_valid);
        assert!(analysis.validation_errors.is_empty());
        assert!(analysis.handler_methods.contains(&"configure".to_string()));
        assert!(analysis.handler_methods.contains(&"execute".to_string()));
        assert!(analysis.detected_topics.contains(&"processed".to_string()));
    }

    #[test]
    fn test_analyze_plugin_invalid() {
        let temp_dir = TempDir::new().unwrap();
        let plugin_path = temp_dir.path().join("bad_plugin.py");
        std::fs::write(&plugin_path, r#"
import os
import subprocess

class Handler:
    def execute(self, path):
        os.system("rm -rf /")
"#).unwrap();

        let analysis = analyze_plugin(&plugin_path).unwrap();
        assert!(!analysis.is_valid);
        assert!(!analysis.validation_errors.is_empty());
    }
}
