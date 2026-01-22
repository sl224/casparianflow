//! casparian_query - SQL Query (Read-Only)
//!
//! Runs SQL queries against output data in read-only mode.
//! Only SELECT, WITH, and EXPLAIN are allowed.

use super::McpTool;
use crate::approvals::ApprovalManager;
use crate::jobs::JobManager;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::types::RedactionPolicy;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

pub struct QueryTool;

#[derive(Debug, Deserialize)]
struct QueryArgs {
    sql: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
    #[serde(default)]
    redaction: Option<RedactionPolicy>,
}

fn default_limit() -> usize {
    1000
}

fn default_timeout() -> u64 {
    30_000
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

/// SQL commands that are allowed (read-only)
const ALLOWED_PREFIXES: &[&str] = &["SELECT", "WITH", "EXPLAIN"];

/// SQL commands that are forbidden (write operations)
const FORBIDDEN_KEYWORDS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER",
    "TRUNCATE", "COPY", "INSTALL", "LOAD", "ATTACH", "DETACH",
];

fn is_read_only_query(sql: &str) -> Result<()> {
    let normalized = sql.trim().to_uppercase();

    // Check if it starts with an allowed command
    let starts_allowed = ALLOWED_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix));

    if !starts_allowed {
        return Err(anyhow!(
            "Query must start with SELECT, WITH, or EXPLAIN"
        ));
    }

    // Check for forbidden keywords (even in subqueries)
    for keyword in FORBIDDEN_KEYWORDS {
        // Look for keyword as a word boundary (not part of identifier)
        let pattern = format!(r"\b{}\b", keyword);
        if regex::Regex::new(&pattern)
            .map(|re| re.is_match(&normalized))
            .unwrap_or(false)
        {
            return Err(anyhow!(
                "Query contains forbidden keyword: {}",
                keyword
            ));
        }
    }

    Ok(())
}

#[async_trait::async_trait]
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
                "timeout_ms": {
                    "type": "integer",
                    "default": 30000,
                    "maximum": 300000
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

    async fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        _jobs: &Arc<Mutex<JobManager>>,
        _approvals: &Arc<Mutex<ApprovalManager>>,
        config: &McpServerConfig,
    ) -> Result<Value> {
        let args: QueryArgs = serde_json::from_value(args)?;

        // Validate SQL is read-only
        is_read_only_query(&args.sql)?;

        // Enforce row limit
        let limit = args.limit.min(security.output_budget.max_rows());

        let start = Instant::now();

        // TODO: Execute query against DuckDB in read-only mode
        // For now, return a placeholder response

        // Open DuckDB in read-only mode
        // let conn = casparian_db::DbConnection::open_duckdb_readonly(&config.db_path)?;
        // let rows = conn.query(&args.sql, &[])?;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        // Placeholder response
        let result = QueryResult {
            columns: vec![
                ColumnInfo {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                },
            ],
            rows: vec![],
            row_count: 0,
            truncated: false,
            elapsed_ms,
        };

        Ok(serde_json::to_value(result)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_only_validation() {
        // Allowed
        assert!(is_read_only_query("SELECT * FROM events").is_ok());
        assert!(is_read_only_query("WITH cte AS (SELECT 1) SELECT * FROM cte").is_ok());
        assert!(is_read_only_query("EXPLAIN SELECT * FROM events").is_ok());

        // Forbidden
        assert!(is_read_only_query("INSERT INTO events VALUES (1)").is_err());
        assert!(is_read_only_query("DELETE FROM events").is_err());
        assert!(is_read_only_query("DROP TABLE events").is_err());
        assert!(is_read_only_query("CREATE TABLE foo (id INT)").is_err());
        assert!(is_read_only_query("UPDATE events SET id = 1").is_err());

        // Forbidden even in subqueries
        assert!(is_read_only_query("SELECT * FROM (DELETE FROM events RETURNING *)").is_err());
    }
}
