//! Read-only SQL guard utilities.

use std::error::Error;
use std::fmt;

const ALLOWED_PREFIXES: &[&str] = &["SELECT", "WITH", "EXPLAIN"];
const FORBIDDEN_KEYWORDS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "DROP", "CREATE", "ALTER", "TRUNCATE", "COPY", "ATTACH",
    "DETACH", "INSTALL", "LOAD",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlGuardError {
    message: String,
}

impl SqlGuardError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SqlGuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for SqlGuardError {}

/// Validate that a SQL query is read-only.
pub fn validate_read_only(sql: &str) -> Result<(), SqlGuardError> {
    let sanitized = sanitize_sql(sql);
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        return Err(SqlGuardError::new("Query is empty"));
    }

    validate_single_statement(&sanitized)?;

    let first = first_keyword(&sanitized)
        .ok_or_else(|| SqlGuardError::new("Query must start with SELECT, WITH, or EXPLAIN"))?;
    if !ALLOWED_PREFIXES.contains(&first.as_str()) {
        return Err(SqlGuardError::new(
            "Query must start with SELECT, WITH, or EXPLAIN",
        ));
    }

    for token in tokens_upper(&sanitized) {
        if FORBIDDEN_KEYWORDS.contains(&token.as_str()) {
            return Err(SqlGuardError::new(format!(
                "Query contains forbidden keyword: {}",
                token
            )));
        }
    }

    Ok(())
}

/// Apply a row limit to a read-only SQL query.
pub fn apply_row_limit(sql: &str, limit: usize) -> String {
    let stripped = strip_trailing_semicolon(sql);
    let keyword = first_keyword(&sanitize_sql(stripped));
    if matches!(keyword.as_deref(), Some("EXPLAIN")) {
        return stripped.trim().to_string();
    }
    if matches!(keyword.as_deref(), Some("SELECT") | Some("WITH")) {
        return format!("SELECT * FROM ({}) AS _q LIMIT {}", stripped.trim(), limit);
    }
    stripped.trim().to_string()
}

fn strip_trailing_semicolon(sql: &str) -> &str {
    let trimmed = sql.trim();
    if let Some(stripped) = trimmed.strip_suffix(';') {
        stripped.trim_end()
    } else {
        trimmed
    }
}

fn validate_single_statement(sql: &str) -> Result<(), SqlGuardError> {
    let mut semicolons = sql.match_indices(';').map(|(idx, _)| idx);
    let first = semicolons.next();
    if semicolons.next().is_some() {
        return Err(SqlGuardError::new("Multiple statements are not allowed"));
    }
    if let Some(idx) = first {
        if sql[idx + 1..].chars().any(|c| !c.is_whitespace()) {
            return Err(SqlGuardError::new("Multiple statements are not allowed"));
        }
    }
    Ok(())
}

fn first_keyword(sql: &str) -> Option<String> {
    let mut current = String::new();
    for ch in sql.chars() {
        if ch.is_ascii_alphabetic() {
            current.push(ch);
        } else if !current.is_empty() {
            break;
        }
    }
    if current.is_empty() {
        None
    } else {
        Some(current.to_ascii_uppercase())
    }
}

fn tokens_upper(sql: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in sql.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(current.to_ascii_uppercase());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.push(current.to_ascii_uppercase());
    }
    tokens
}

fn sanitize_sql(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
            }
            out.push(' ');
            continue;
        }
        if in_block_comment {
            if ch == '*' && matches!(chars.peek(), Some('/')) {
                chars.next();
                in_block_comment = false;
                out.push(' ');
                out.push(' ');
                continue;
            }
            out.push(' ');
            continue;
        }
        if in_single {
            if ch == '\'' {
                if matches!(chars.peek(), Some('\'')) {
                    chars.next();
                    out.push(' ');
                    out.push(' ');
                    continue;
                }
                in_single = false;
            }
            out.push(' ');
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            }
            out.push(' ');
            continue;
        }

        if ch == '-' && matches!(chars.peek(), Some('-')) {
            chars.next();
            in_line_comment = true;
            out.push(' ');
            out.push(' ');
            continue;
        }
        if ch == '/' && matches!(chars.peek(), Some('*')) {
            chars.next();
            in_block_comment = true;
            out.push(' ');
            out.push(' ');
            continue;
        }
        if ch == '\'' {
            in_single = true;
            out.push(' ');
            continue;
        }
        if ch == '"' {
            in_double = true;
            out.push(' ');
            continue;
        }

        out.push(ch);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_read_only_basic() {
        assert!(validate_read_only("SELECT * FROM events").is_ok());
        assert!(validate_read_only("WITH cte AS (SELECT 1) SELECT * FROM cte").is_ok());
        assert!(validate_read_only("EXPLAIN SELECT * FROM events").is_ok());
        assert!(validate_read_only("INSERT INTO events VALUES (1)").is_err());
        assert!(validate_read_only("DELETE FROM events").is_err());
        assert!(validate_read_only("DROP TABLE events").is_err());
        assert!(validate_read_only("CREATE TABLE foo (id INT)").is_err());
        assert!(validate_read_only("UPDATE events SET id = 1").is_err());
    }

    #[test]
    fn test_validate_read_only_comments_and_literals() {
        assert!(validate_read_only("SELECT 1 -- INSERT INTO events").is_ok());
        assert!(validate_read_only("SELECT 1 /* INSERT */ FROM events").is_ok());
        assert!(validate_read_only("SELECT 'DROP TABLE x' FROM events").is_ok());
    }

    #[test]
    fn test_validate_read_only_multi_statement() {
        assert!(validate_read_only("SELECT 1; DROP TABLE events").is_err());
        assert!(validate_read_only("SELECT 1;\nDELETE FROM events").is_err());
        assert!(validate_read_only("SELECT 1; ").is_ok());
    }

    #[test]
    fn test_validate_read_only_nested_forbidden() {
        assert!(validate_read_only("SELECT * FROM (DELETE FROM events RETURNING *)").is_err());
    }

    #[test]
    fn test_apply_row_limit() {
        let sql = apply_row_limit("SELECT * FROM events", 100);
        assert_eq!(sql, "SELECT * FROM (SELECT * FROM events) AS _q LIMIT 100");

        let sql = apply_row_limit("SELECT * FROM events;", 25);
        assert_eq!(sql, "SELECT * FROM (SELECT * FROM events) AS _q LIMIT 25");

        let sql = apply_row_limit("EXPLAIN SELECT * FROM events", 10);
        assert_eq!(sql, "EXPLAIN SELECT * FROM events");
    }
}
