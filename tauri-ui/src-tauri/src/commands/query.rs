//! SQL query execution commands.
//!
//! These commands execute read-only SQL queries against the database.

use crate::state::{AppState, CommandError, CommandResult};
use casparian_db::DbValue;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Query execution request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    pub sql: String,
    pub limit: Option<usize>,
}

/// Query execution result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub exec_time_ms: u64,
}

/// Allowed SQL prefixes (read-only operations).
const ALLOWED_PREFIXES: &[&str] = &["SELECT", "WITH", "EXPLAIN"];

/// Forbidden SQL keywords (write operations).
const FORBIDDEN_KEYWORDS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER", "TRUNCATE", "COPY", "INSTALL", "LOAD",
    "ATTACH", "DETACH",
];

/// Validate that the SQL query is read-only.
fn validate_sql(sql: &str) -> Result<(), CommandError> {
    let trimmed = sql.trim().to_uppercase();

    // Check if starts with allowed prefix
    let has_allowed_prefix = ALLOWED_PREFIXES
        .iter()
        .any(|prefix| trimmed.starts_with(prefix));

    if !has_allowed_prefix {
        return Err(CommandError::InvalidArgument(format!(
            "Query must start with one of: {}",
            ALLOWED_PREFIXES.join(", ")
        )));
    }

    // Check for forbidden keywords
    for keyword in FORBIDDEN_KEYWORDS {
        // Check for keyword at word boundary
        if trimmed.contains(&format!(" {} ", keyword))
            || trimmed.contains(&format!("({})", keyword))
            || trimmed.contains(&format!("({} ", keyword))
            || trimmed.ends_with(&format!(" {}", keyword))
        {
            return Err(CommandError::InvalidArgument(format!(
                "Query contains forbidden keyword: {}",
                keyword
            )));
        }
    }

    Ok(())
}

/// Convert a database value to JSON.
fn db_value_to_json(value: &DbValue) -> serde_json::Value {
    match value {
        DbValue::Null => serde_json::Value::Null,
        DbValue::Boolean(b) => serde_json::Value::Bool(*b),
        DbValue::Integer(i) => serde_json::json!(*i),
        DbValue::Real(f) => serde_json::json!(*f),
        DbValue::Text(s) => serde_json::Value::String(s.clone()),
        DbValue::Timestamp(ts) => serde_json::Value::String(ts.to_rfc3339()),
        DbValue::Blob(b) => {
            // Encode blob as base64
            use base64::Engine;
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(b))
        }
    }
}

/// Execute a SQL query.
#[tauri::command]
pub async fn query_execute(
    request: QueryRequest,
    state: State<'_, AppState>,
) -> CommandResult<QueryResult> {
    // Validate the SQL
    validate_sql(&request.sql)?;

    let start = std::time::Instant::now();

    // Open a read-only connection
    let conn = state
        .open_readonly_connection()
        .map_err(|e| CommandError::Database(e.to_string()))?;

    // Apply limit
    let limit = request.limit.unwrap_or(1000).min(10000);
    let sql_with_limit = if request.sql.to_uppercase().contains(" LIMIT ") {
        request.sql.clone()
    } else {
        format!("{} LIMIT {}", request.sql.trim_end_matches(';'), limit)
    };

    // Execute query
    let rows = conn
        .query_all(&sql_with_limit, &[])
        .map_err(|e| CommandError::Database(e.to_string()))?;

    let exec_time_ms = start.elapsed().as_millis() as u64;

    // Extract column names from the first row if available
    let columns: Vec<String> = if rows.is_empty() {
        vec![]
    } else {
        rows[0].column_names().to_vec()
    };

    // Convert rows to JSON
    let json_rows: Vec<Vec<serde_json::Value>> = rows
        .iter()
        .map(|row| {
            (0..row.len())
                .filter_map(|i| row.get_raw(i).map(|v| db_value_to_json(v)))
                .collect()
        })
        .collect();

    let row_count = json_rows.len();

    Ok(QueryResult {
        columns,
        rows: json_rows,
        row_count,
        exec_time_ms,
    })
}
