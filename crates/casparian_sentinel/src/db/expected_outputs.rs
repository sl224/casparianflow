//! Expected Outputs Query API.
//!
//! Provides an API for querying what outputs a plugin produces.
//! This is used by the execution plan to properly handle default sinks.

use anyhow::{Context, Result};
use casparian_db::{DbConnection, DbValue};
use casparian_protocol::PluginStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::warn;

/// Specification for a single output from a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSpec {
    /// Name of the output (e.g., "orders", "events")
    pub output_name: String,
    /// Optional schema hash if known
    pub schema_hash: Option<String>,
    /// Optional topic/tag association
    pub topic: Option<String>,
}

/// Query API for expected outputs from a plugin.
///
/// This struct provides methods to query what outputs a plugin produces,
/// derived from the plugin manifest's `outputs_json` column.
pub struct ExpectedOutputs;

impl ExpectedOutputs {
    /// Get expected outputs for a plugin.
    ///
    /// Returns empty vec if plugin not found (not an error).
    /// This allows callers to gracefully handle unknown plugins.
    ///
    /// # Arguments
    /// * `conn` - Database connection
    /// * `plugin_name` - Name of the plugin to query
    /// * `version` - Optional version filter (if None, uses latest active version)
    ///
    /// # Returns
    /// * `Ok(Vec<OutputSpec>)` - List of expected outputs (may be empty)
    /// * `Err` - Only on database or parse errors
    ///
    /// # Example
    /// ```ignore
    /// let outputs = ExpectedOutputs::list_for_plugin(&conn, "my_parser", None)?;
    /// for output in outputs {
    ///     println!("Output: {}", output.output_name);
    /// }
    /// ```
    pub fn list_for_plugin(
        conn: &DbConnection,
        plugin_name: &str,
        version: Option<&str>,
    ) -> Result<Vec<OutputSpec>> {
        // Query the manifest based on whether version is specified
        let (query, params) = match version {
            Some(v) => (
                r#"
                SELECT outputs_json, schema_artifacts_json
                FROM cf_plugin_manifest
                WHERE plugin_name = ? AND version = ? AND status IN (?, ?)
                ORDER BY deployed_at DESC NULLS LAST, created_at DESC
                LIMIT 1
                "#,
                vec![
                    DbValue::from(plugin_name),
                    DbValue::from(v),
                    DbValue::from(PluginStatus::Active.as_str()),
                    DbValue::from(PluginStatus::Deployed.as_str()),
                ],
            ),
            None => (
                r#"
                SELECT outputs_json, schema_artifacts_json
                FROM cf_plugin_manifest
                WHERE plugin_name = ? AND status IN (?, ?)
                ORDER BY deployed_at DESC NULLS LAST, created_at DESC
                LIMIT 1
                "#,
                vec![
                    DbValue::from(plugin_name),
                    DbValue::from(PluginStatus::Active.as_str()),
                    DbValue::from(PluginStatus::Deployed.as_str()),
                ],
            ),
        };

        let row = conn.query_optional(query, &params)?;

        let Some(row) = row else {
            // Plugin not found - return empty vec (not an error per spec)
            return Ok(Vec::new());
        };

        // Try outputs_json first (primary source)
        let outputs_json: String = row
            .get_by_name("outputs_json")
            .context("Failed to read outputs_json from cf_plugin_manifest")?;

        // Parse outputs_json - it's a JSON object where keys are output names
        let outputs = Self::parse_outputs_json(&outputs_json, plugin_name)?;

        if outputs.is_empty() {
            // Fallback: try schema_artifacts_json if outputs_json was empty
            let schema_json: Option<String> = row.get_by_name("schema_artifacts_json").ok();
            if let Some(json) = schema_json {
                let fallback = Self::parse_outputs_json(&json, plugin_name)?;
                if !fallback.is_empty() {
                    return Ok(fallback);
                }
            }
            // If still empty, log a warning
            warn!(
                plugin_name = plugin_name,
                "Plugin has no declared outputs in manifest"
            );
        }

        Ok(outputs)
    }

    /// Parse outputs from JSON.
    ///
    /// Expected format is a JSON object where keys are output names:
    /// ```json
    /// {
    ///     "orders": { "columns": [...] },
    ///     "events": { "columns": [...] }
    /// }
    /// ```
    fn parse_outputs_json(json_str: &str, plugin_name: &str) -> Result<Vec<OutputSpec>> {
        // Handle empty or trivial JSON
        if json_str.is_empty() || json_str == "{}" {
            return Ok(Vec::new());
        }

        // Parse as a generic JSON object to extract keys (output names)
        let parsed: HashMap<String, serde_json::Value> = serde_json::from_str(json_str)
            .with_context(|| {
                format!(
                    "Failed to parse outputs_json for plugin '{}': {}",
                    plugin_name,
                    json_str.chars().take(100).collect::<String>()
                )
            })?;

        let mut outputs = Vec::with_capacity(parsed.len());
        for (output_name, value) in parsed {
            // Try to extract schema_hash if present in the value
            let schema_hash = value
                .get("content_hash")
                .or_else(|| value.get("schema_hash"))
                .and_then(|v| v.as_str())
                .map(String::from);

            // Try to extract topic if present
            let topic = value
                .get("topic")
                .and_then(|v| v.as_str())
                .map(String::from);

            outputs.push(OutputSpec {
                output_name,
                schema_hash,
                topic,
            });
        }

        // Sort by output name for consistent ordering
        outputs.sort_by(|a, b| a.output_name.cmp(&b.output_name));

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queue::JobQueue;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn setup_db() -> DbConnection {
        let conn = DbConnection::open_duckdb_memory().unwrap();
        let queue = JobQueue::new(conn.clone());
        queue.init_registry_schema().unwrap();
        conn
    }

    fn insert_test_plugin(
        conn: &DbConnection,
        plugin_name: &str,
        version: &str,
        outputs_json: &str,
    ) {
        let now = now_millis();
        // Generate unique source_hash using plugin_name and version
        let source_hash = format!("hash_{}_{}", plugin_name, version);
        conn.execute(
            r#"
            INSERT INTO cf_plugin_manifest (
                plugin_name, version, runtime_kind, entrypoint,
                source_code, source_hash, status, env_hash, artifact_hash,
                manifest_json, protocol_version, schema_artifacts_json, outputs_json,
                signature_verified, created_at, deployed_at
            ) VALUES (?, ?, 'python_shim', 'test.py:parse', 'code', ?, 'ACTIVE', '', '',
                      '{}', '1.0', '{}', ?, false, ?, ?)
            "#,
            &[
                DbValue::from(plugin_name),
                DbValue::from(version),
                DbValue::from(source_hash.as_str()),
                DbValue::from(outputs_json),
                DbValue::from(now),
                DbValue::from(now),
            ],
        )
        .unwrap();
    }

    fn now_millis() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX_EPOCH - check system clock")
            .as_millis()
            .try_into()
            .unwrap_or(i64::MAX)
    }

    #[test]
    fn test_list_for_plugin_with_outputs() {
        let conn = setup_db();
        let outputs_json = r#"{"orders": {"columns": []}, "events": {"columns": []}}"#;
        insert_test_plugin(&conn, "test_parser", "1.0.0", outputs_json);

        let outputs = ExpectedOutputs::list_for_plugin(&conn, "test_parser", None).unwrap();

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].output_name, "events"); // Sorted alphabetically
        assert_eq!(outputs[1].output_name, "orders");
    }

    #[test]
    fn test_list_for_plugin_unknown_returns_empty() {
        let conn = setup_db();

        let outputs = ExpectedOutputs::list_for_plugin(&conn, "nonexistent_parser", None).unwrap();

        assert!(outputs.is_empty());
    }

    #[test]
    fn test_list_for_plugin_empty_outputs_json() {
        let conn = setup_db();
        insert_test_plugin(&conn, "empty_parser", "1.0.0", "{}");

        let outputs = ExpectedOutputs::list_for_plugin(&conn, "empty_parser", None).unwrap();

        assert!(outputs.is_empty());
    }

    #[test]
    fn test_list_for_plugin_with_version_filter() {
        let conn = setup_db();
        insert_test_plugin(&conn, "versioned_parser", "1.0.0", r#"{"v1_output": {}}"#);
        insert_test_plugin(&conn, "versioned_parser", "2.0.0", r#"{"v2_output": {}}"#);

        // Query specific version
        let outputs_v1 =
            ExpectedOutputs::list_for_plugin(&conn, "versioned_parser", Some("1.0.0")).unwrap();
        assert_eq!(outputs_v1.len(), 1);
        assert_eq!(outputs_v1[0].output_name, "v1_output");

        let outputs_v2 =
            ExpectedOutputs::list_for_plugin(&conn, "versioned_parser", Some("2.0.0")).unwrap();
        assert_eq!(outputs_v2.len(), 1);
        assert_eq!(outputs_v2[0].output_name, "v2_output");
    }

    #[test]
    fn test_list_for_plugin_with_schema_hash() {
        let conn = setup_db();
        let outputs_json = r#"{"orders": {"content_hash": "abc123", "columns": []}}"#;
        insert_test_plugin(&conn, "hash_parser", "1.0.0", outputs_json);

        let outputs = ExpectedOutputs::list_for_plugin(&conn, "hash_parser", None).unwrap();

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].output_name, "orders");
        assert_eq!(outputs[0].schema_hash, Some("abc123".to_string()));
    }

    #[test]
    fn test_parse_outputs_json_handles_various_formats() {
        // Test empty
        let outputs = ExpectedOutputs::parse_outputs_json("{}", "test").unwrap();
        assert!(outputs.is_empty());

        // Test with schema_hash key (alternate)
        let json = r#"{"output1": {"schema_hash": "xyz789"}}"#;
        let outputs = ExpectedOutputs::parse_outputs_json(json, "test").unwrap();
        assert_eq!(outputs[0].schema_hash, Some("xyz789".to_string()));

        // Test with topic
        let json = r#"{"output1": {"topic": "sales_data"}}"#;
        let outputs = ExpectedOutputs::parse_outputs_json(json, "test").unwrap();
        assert_eq!(outputs[0].topic, Some("sales_data".to_string()));
    }
}
