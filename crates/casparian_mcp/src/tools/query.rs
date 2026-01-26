//! casparian_query - SQL Query (Read-Only)
//!
//! Runs SQL queries against output data in read-only mode.
//! Only SELECT, WITH, and EXPLAIN are allowed.

use super::McpTool;
use crate::core::CoreHandle;
use crate::jobs::JobExecutorHandle;
use crate::redaction;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::types::RedactionPolicy;
use anyhow::{anyhow, Result};
use casparian_db::{apply_row_limit, validate_read_only, DbConnection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

pub struct QueryTool;

#[derive(Debug, Deserialize)]
struct QueryArgs {
    sql: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    redaction: Option<RedactionPolicy>,
}

fn default_limit() -> usize {
    1000
}

#[derive(Debug, Serialize)]
struct ColumnInfo {
    name: String,
    #[serde(rename = "type")]
    data_type: String,
}

#[derive(Debug, Serialize)]
struct QueryResult {
    columns: Vec<ColumnInfo>,
    rows: Vec<Vec<Value>>,
    row_count: usize,
    truncated: bool,
    elapsed_ms: u64,
}

impl McpTool for QueryTool {
    fn name(&self) -> &'static str {
        "casparian_query"
    }

    fn description(&self) -> &'static str {
        "Run SQL query on output data (read-only)"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "sql": {
                    "type": "string",
                    "description": "SQL query (SELECT, WITH, EXPLAIN only)"
                },
                "limit": {
                    "type": "integer",
                    "default": 1000,
                    "maximum": 10000
                },
                "redaction": {
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["none", "truncate", "hash"], "default": "hash" }
                    }
                }
            },
            "required": ["sql"]
        })
    }

    fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        _core: &CoreHandle,
        config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> Result<Value> {
        let args: QueryArgs = serde_json::from_value(args)?;

        // Validate SQL is read-only
        validate_read_only(&args.sql).map_err(|err| anyhow!(err))?;

        // Enforce row limit
        let limit = args.limit.min(security.output_budget.max_rows());

        let start = Instant::now();

        // Open query catalog in read-only mode
        let conn = DbConnection::open_duckdb_readonly(config.query_catalog_path.as_path())
            .map_err(|e| anyhow!("Failed to open database: {}", e))?;

        // Add LIMIT to query if not present
        let sql = apply_row_limit(&args.sql, limit);

        // Execute query
        let db_rows = conn
            .query_all(&sql, &[])
            .map_err(|e| anyhow!("Query failed: {}", e))?;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        // Get column info from the first row (DbRow now exposes column_names)
        let columns: Vec<ColumnInfo> = if let Some(first_row) = db_rows.first() {
            first_row
                .column_names()
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    // Infer type from the first row's values
                    let data_type = first_row
                        .get_raw(i)
                        .map(db_value_to_type_name)
                        .unwrap_or_else(|| "UNKNOWN".to_string());
                    ColumnInfo {
                        name: name.clone(),
                        data_type,
                    }
                })
                .collect()
        } else {
            vec![]
        };

        // Convert rows to JSON values
        let mut rows: Vec<Vec<Value>> = db_rows.iter().map(db_row_to_json_values).collect();

        // Check if truncated
        let truncated = rows.len() >= limit;

        // Apply redaction
        let redaction_policy = args.redaction.unwrap_or_default();
        rows = redaction::redact_rows(&rows, &redaction_policy);

        let row_count = rows.len();

        let result = QueryResult {
            columns,
            rows,
            row_count,
            truncated,
            elapsed_ms,
        };

        Ok(serde_json::to_value(result)?)
    }
}

/// Convert a database row to JSON values with proper type preservation.
fn db_row_to_json_values(row: &casparian_db::UnifiedDbRow) -> Vec<Value> {
    (0..row.len())
        .map(|i| {
            // Use the raw DbValue to preserve types correctly
            // instead of trying string first (which loses type info)
            if let Some(raw) = row.get_raw(i) {
                return db_value_to_json(raw);
            }
            Value::Null
        })
        .collect()
}

/// Convert a DbValue to JSON Value with proper type preservation.
fn db_value_to_json(value: &casparian_db::DbValue) -> Value {
    use casparian_db::DbValue;
    match value {
        DbValue::Null => Value::Null,
        DbValue::Integer(v) => json!(*v),
        DbValue::Real(v) => json!(*v),
        DbValue::Text(v) => json!(v),
        DbValue::Blob(v) => {
            // Base64 encode binary data
            use base64::Engine;
            json!(base64::engine::general_purpose::STANDARD.encode(v))
        }
        DbValue::Boolean(v) => json!(*v),
        DbValue::Timestamp(v) => json!(v.to_rfc3339()),
    }
}

/// Infer SQL type name from DbValue for column metadata.
fn db_value_to_type_name(value: &casparian_db::DbValue) -> String {
    use casparian_db::DbValue;
    match value {
        DbValue::Null => "NULL".to_string(),
        DbValue::Integer(_) => "BIGINT".to_string(),
        DbValue::Real(_) => "DOUBLE".to_string(),
        DbValue::Text(_) => "VARCHAR".to_string(),
        DbValue::Blob(_) => "BLOB".to_string(),
        DbValue::Boolean(_) => "BOOLEAN".to_string(),
        DbValue::Timestamp(_) => "TIMESTAMP".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_only_validation() {
        // Allowed
        assert!(validate_read_only("SELECT * FROM events").is_ok());
        assert!(validate_read_only("WITH cte AS (SELECT 1) SELECT * FROM cte").is_ok());
        assert!(validate_read_only("EXPLAIN SELECT * FROM events").is_ok());

        // Forbidden
        assert!(validate_read_only("INSERT INTO events VALUES (1)").is_err());
        assert!(validate_read_only("DELETE FROM events").is_err());
        assert!(validate_read_only("DROP TABLE events").is_err());
        assert!(validate_read_only("CREATE TABLE foo (id INT)").is_err());
        assert!(validate_read_only("UPDATE events SET id = 1").is_err());

        // Forbidden even in subqueries
        assert!(validate_read_only("SELECT * FROM (DELETE FROM events RETURNING *)").is_err());
    }

    #[test]
    fn test_sql_injection_patterns() {
        // Semicolon chaining attacks
        assert!(validate_read_only("SELECT 1; DROP TABLE events").is_err());
        assert!(validate_read_only("SELECT 1; DELETE FROM events").is_err());

        // UNION-based attacks that include write operations
        // These are allowed if they're just SELECT statements
        assert!(validate_read_only("SELECT 1 UNION SELECT 2").is_ok());

        // Comment-based keywords are ignored
        assert!(validate_read_only("SELECT 1 -- INSERT INTO events").is_ok());
        assert!(validate_read_only("SELECT 1 /* INSERT */ FROM events").is_ok());
    }

    #[test]
    fn test_case_insensitivity() {
        // Should block regardless of case
        assert!(validate_read_only("insert into events values (1)").is_err());
        assert!(validate_read_only("INSERT INTO events VALUES (1)").is_err());
        assert!(validate_read_only("InSeRt InTo events VALUES (1)").is_err());

        // Should allow regardless of case
        assert!(validate_read_only("select * from events").is_ok());
        assert!(validate_read_only("SELECT * FROM events").is_ok());
        assert!(validate_read_only("SeLeCt * FrOm events").is_ok());
    }

    #[test]
    fn test_duckdb_specific_forbidden_commands() {
        // DuckDB-specific commands that should be blocked
        assert!(validate_read_only("COPY events TO '/tmp/data.csv'").is_err());
        assert!(validate_read_only("INSTALL httpfs").is_err());
        assert!(validate_read_only("LOAD httpfs").is_err());
        assert!(validate_read_only("ATTACH '/tmp/other.db' AS other").is_err());
        assert!(validate_read_only("DETACH other").is_err());
    }

    #[test]
    fn test_add_limit_if_missing() {
        assert_eq!(
            apply_row_limit("SELECT * FROM events", 100),
            "SELECT * FROM (SELECT * FROM events) AS _q LIMIT 100"
        );
        assert_eq!(
            apply_row_limit("SELECT * FROM events LIMIT 50", 100),
            "SELECT * FROM (SELECT * FROM events LIMIT 50) AS _q LIMIT 100"
        );
        assert_eq!(
            apply_row_limit("SELECT * FROM events;", 100),
            "SELECT * FROM (SELECT * FROM events) AS _q LIMIT 100"
        );
    }

    #[test]
    fn test_whitespace_handling() {
        // Leading/trailing whitespace should be handled
        assert!(validate_read_only("  SELECT * FROM events  ").is_ok());
        assert!(validate_read_only("\n\tSELECT * FROM events\n").is_ok());

        // Newlines shouldn't break validation
        assert!(validate_read_only("SELECT *\nFROM events").is_ok());
        assert!(validate_read_only("SELECT 1;\nDROP TABLE events").is_err());
    }
}
