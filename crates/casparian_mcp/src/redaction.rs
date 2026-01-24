//! Redaction module for sensitive data protection.
//!
//! Provides centralized redaction functionality for query results and previews.
//! Supports hash, truncate, and none modes with configurable parameters.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::types::{RedactionMode, RedactionPolicy};

/// Apply redaction to a single JSON value based on the policy.
pub fn redact_value(value: &Value, policy: &RedactionPolicy) -> Value {
    match policy.mode {
        RedactionMode::None => value.clone(),
        RedactionMode::Truncate => truncate_value(value, policy.max_value_length),
        RedactionMode::Hash => hash_value(value, policy.hash_prefix_length),
    }
}

/// Apply redaction to a row of JSON values.
pub fn redact_row(row: &[Value], policy: &RedactionPolicy) -> Vec<Value> {
    row.iter().map(|v| redact_value(v, policy)).collect()
}

/// Apply redaction to multiple rows of JSON values.
pub fn redact_rows(rows: &[Vec<Value>], policy: &RedactionPolicy) -> Vec<Vec<Value>> {
    rows.iter().map(|row| redact_row(row, policy)).collect()
}

/// Truncate a JSON value to the maximum length.
fn truncate_value(value: &Value, max_length: usize) -> Value {
    match value {
        Value::String(s) => {
            if s.len() <= max_length {
                value.clone()
            } else {
                Value::String(format!("{}...", &s[..max_length]))
            }
        }
        Value::Array(arr) => {
            // Truncate each element, then the array itself if needed
            let truncated: Vec<Value> = arr
                .iter()
                .take(max_length)
                .map(|v| truncate_value(v, max_length))
                .collect();
            Value::Array(truncated)
        }
        Value::Object(obj) => {
            // Truncate values in the object
            let mut truncated = serde_json::Map::new();
            for (k, v) in obj.iter() {
                truncated.insert(k.clone(), truncate_value(v, max_length));
            }
            Value::Object(truncated)
        }
        // Numbers, bools, null pass through
        _ => value.clone(),
    }
}

/// Hash a JSON value for redaction.
fn hash_value(value: &Value, prefix_length: usize) -> Value {
    match value {
        Value::String(s) => {
            let hash = compute_hash(s);
            Value::String(format!("[hash:{}]", &hash[..prefix_length.min(hash.len())]))
        }
        Value::Number(n) => {
            // Hash numbers as strings for consistency
            let hash = compute_hash(&n.to_string());
            Value::String(format!("[hash:{}]", &hash[..prefix_length.min(hash.len())]))
        }
        Value::Array(arr) => {
            // Hash each element
            let hashed: Vec<Value> = arr.iter().map(|v| hash_value(v, prefix_length)).collect();
            Value::Array(hashed)
        }
        Value::Object(obj) => {
            // Hash values in the object (preserve keys)
            let mut hashed = serde_json::Map::new();
            for (k, v) in obj.iter() {
                hashed.insert(k.clone(), hash_value(v, prefix_length));
            }
            Value::Object(hashed)
        }
        // Bool and null pass through (not sensitive)
        Value::Bool(_) | Value::Null => value.clone(),
    }
}

/// Compute SHA256 hash and return as hex string.
fn compute_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let hash = hasher.finalize();
    hex::encode(hash)
}

/// Check if a column name suggests sensitive data.
pub fn is_sensitive_column(name: &str) -> bool {
    let lower = name.to_lowercase();
    SENSITIVE_PATTERNS
        .iter()
        .any(|pattern| lower.contains(pattern))
}

/// Column name patterns that suggest sensitive data.
const SENSITIVE_PATTERNS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "secret",
    "key",
    "token",
    "auth",
    "credential",
    "ssn",
    "social_security",
    "credit_card",
    "creditcard",
    "ccn",
    "card_number",
    "email",
    "phone",
    "address",
    "ip_address",
    "ip_addr",
    "user_id",
    "userid",
    "username",
    "user_name",
    "api_key",
    "apikey",
    "private",
    "sensitive",
    "pii",
    "phi",
    "hipaa",
    "gdpr",
];

/// Selectively redact only sensitive columns in a row.
pub fn redact_sensitive_columns(
    row: &[Value],
    column_names: &[String],
    policy: &RedactionPolicy,
) -> Vec<Value> {
    row.iter()
        .zip(column_names.iter())
        .map(|(v, name)| {
            if is_sensitive_column(name) {
                redact_value(v, policy)
            } else {
                v.clone()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn default_policy() -> RedactionPolicy {
        RedactionPolicy::default()
    }

    fn truncate_policy(max_len: usize) -> RedactionPolicy {
        RedactionPolicy {
            mode: RedactionMode::Truncate,
            max_sample_count: 5,
            max_value_length: max_len,
            hash_prefix_length: 8,
        }
    }

    fn no_redaction_policy() -> RedactionPolicy {
        RedactionPolicy {
            mode: RedactionMode::None,
            ..Default::default()
        }
    }

    #[test]
    fn test_hash_string() {
        let policy = default_policy();
        let result = redact_value(&json!("sensitive-data"), &policy);
        let s = result.as_str().unwrap();
        assert!(s.starts_with("[hash:"));
        assert!(s.ends_with("]"));
        // Default hash prefix is 8 chars
        assert!(s.len() > 8);
    }

    #[test]
    fn test_hash_number() {
        let policy = default_policy();
        let result = redact_value(&json!(12345), &policy);
        let s = result.as_str().unwrap();
        assert!(s.starts_with("[hash:"));
    }

    #[test]
    fn test_hash_preserves_bool_and_null() {
        let policy = default_policy();
        assert_eq!(redact_value(&json!(true), &policy), json!(true));
        assert_eq!(redact_value(&json!(false), &policy), json!(false));
        assert_eq!(redact_value(&json!(null), &policy), json!(null));
    }

    #[test]
    fn test_truncate_string() {
        let policy = truncate_policy(5);
        let result = redact_value(&json!("hello world"), &policy);
        assert_eq!(result, json!("hello..."));
    }

    #[test]
    fn test_truncate_short_string() {
        let policy = truncate_policy(20);
        let result = redact_value(&json!("hi"), &policy);
        assert_eq!(result, json!("hi"));
    }

    #[test]
    fn test_no_redaction() {
        let policy = no_redaction_policy();
        let result = redact_value(&json!("sensitive"), &policy);
        assert_eq!(result, json!("sensitive"));
    }

    #[test]
    fn test_redact_row() {
        let policy = default_policy();
        let row = vec![json!("a"), json!(123), json!(true)];
        let result = redact_row(&row, &policy);

        assert!(result[0].as_str().unwrap().starts_with("[hash:"));
        assert!(result[1].as_str().unwrap().starts_with("[hash:"));
        assert_eq!(result[2], json!(true)); // bool unchanged
    }

    #[test]
    fn test_redact_rows() {
        let policy = truncate_policy(3);
        let rows = vec![
            vec![json!("hello"), json!("world")],
            vec![json!("foo"), json!("bar")],
        ];
        let result = redact_rows(&rows, &policy);

        assert_eq!(result[0][0], json!("hel..."));
        assert_eq!(result[0][1], json!("wor..."));
        assert_eq!(result[1][0], json!("foo"));
        assert_eq!(result[1][1], json!("bar"));
    }

    #[test]
    fn test_is_sensitive_column() {
        assert!(is_sensitive_column("password"));
        assert!(is_sensitive_column("user_password"));
        assert!(is_sensitive_column("email"));
        assert!(is_sensitive_column("api_key"));
        assert!(is_sensitive_column("credit_card_number"));

        assert!(!is_sensitive_column("id"));
        assert!(!is_sensitive_column("name"));
        assert!(!is_sensitive_column("created_at"));
        assert!(!is_sensitive_column("count"));
    }

    #[test]
    fn test_redact_sensitive_columns() {
        let policy = default_policy();
        let row = vec![json!(1), json!("john@example.com"), json!("John Doe")];
        let columns = vec!["id".to_string(), "email".to_string(), "name".to_string()];

        let result = redact_sensitive_columns(&row, &columns, &policy);

        assert_eq!(result[0], json!(1)); // id unchanged (not sensitive)
        assert!(result[1].as_str().unwrap().starts_with("[hash:")); // email redacted
        assert_eq!(result[2], json!("John Doe")); // name unchanged
    }

    #[test]
    fn test_hash_nested_object() {
        let policy = default_policy();
        let value = json!({
            "name": "John",
            "details": {
                "ssn": "123-45-6789"
            }
        });
        let result = redact_value(&value, &policy);

        // Both name and ssn should be hashed
        let obj = result.as_object().unwrap();
        assert!(obj["name"].as_str().unwrap().starts_with("[hash:"));
        let details = obj["details"].as_object().unwrap();
        assert!(details["ssn"].as_str().unwrap().starts_with("[hash:"));
    }

    #[test]
    fn test_hash_array() {
        let policy = default_policy();
        let value = json!(["a", "b", "c"]);
        let result = redact_value(&value, &policy);

        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        for v in arr {
            assert!(v.as_str().unwrap().starts_with("[hash:"));
        }
    }
}
