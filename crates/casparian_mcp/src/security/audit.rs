//! Audit Logging - Tool Invocation Recording
//!
//! Records all MCP tool invocations for security auditing.
//! Logs are written to a file in append-only mode.
//!
//! # Log Format
//!
//! Each line is a JSON object:
//! ```json
//! {"ts":"2026-01-21T10:30:00Z","type":"request","method":"tools/call","tool":"casparian_scan","args":{...}}
//! {"ts":"2026-01-21T10:30:01Z","type":"response","tool":"casparian_scan","success":true}
//! ```

use super::SecurityError;
use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;

/// Audit log for recording MCP operations
#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
    writer: Mutex<BufWriter<File>>,
}

impl AuditLog {
    /// Create a new audit log
    pub fn new(path: PathBuf) -> Result<Self, SecurityError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                SecurityError::AuditError(format!(
                    "Failed to create audit log directory: {}",
                    e
                ))
            })?;
        }

        // Open file in append mode
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| {
                SecurityError::AuditError(format!("Failed to open audit log: {}", e))
            })?;

        let writer = Mutex::new(BufWriter::new(file));

        Ok(Self { path, writer })
    }

    /// Log a request
    pub fn log_request(&mut self, request: &JsonRpcRequest) -> Result<(), SecurityError> {
        let entry = AuditEntry::Request {
            ts: Utc::now(),
            method: request.method.clone(),
            id: request.id.as_ref().map(|id| format!("{:?}", id)),
            params_summary: request.params.as_ref().map(summarize_params),
        };

        self.write_entry(&entry)
    }

    /// Log a response
    pub fn log_response(&mut self, response: &JsonRpcResponse) -> Result<(), SecurityError> {
        let entry = AuditEntry::Response {
            ts: Utc::now(),
            id: response.id.as_ref().map(|id| format!("{:?}", id)),
            success: response.error.is_none(),
            error_code: response.error.as_ref().map(|e| e.code),
        };

        self.write_entry(&entry)
    }

    /// Log a tool call specifically
    pub fn log_tool_call(
        &mut self,
        tool_name: &str,
        success: bool,
        duration_ms: u64,
    ) -> Result<(), SecurityError> {
        let entry = AuditEntry::ToolCall {
            ts: Utc::now(),
            tool: tool_name.to_string(),
            success,
            duration_ms,
        };

        self.write_entry(&entry)
    }

    /// Write an entry to the log
    fn write_entry(&self, entry: &AuditEntry) -> Result<(), SecurityError> {
        let json = serde_json::to_string(entry).map_err(|e| {
            SecurityError::AuditError(format!("Failed to serialize audit entry: {}", e))
        })?;

        let mut writer = self.writer.lock().map_err(|e| {
            SecurityError::AuditError(format!("Failed to lock audit log: {}", e))
        })?;

        writeln!(writer, "{}", json).map_err(|e| {
            SecurityError::AuditError(format!("Failed to write audit entry: {}", e))
        })?;

        writer.flush().map_err(|e| {
            SecurityError::AuditError(format!("Failed to flush audit log: {}", e))
        })?;

        Ok(())
    }

    /// Get the log file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Audit log entry types
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AuditEntry {
    Request {
        ts: DateTime<Utc>,
        method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        params_summary: Option<String>,
    },
    Response {
        ts: DateTime<Utc>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_code: Option<i32>,
    },
    ToolCall {
        ts: DateTime<Utc>,
        tool: String,
        success: bool,
        duration_ms: u64,
    },
}

/// Summarize params for logging (avoid logging sensitive data)
fn summarize_params(params: &serde_json::Value) -> String {
    match params {
        serde_json::Value::Object(map) => {
            let keys: Vec<&str> = map.keys().map(|s| s.as_str()).collect();
            format!("{{keys: [{}]}}", keys.join(", "))
        }
        serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
        _ => "[value]".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_audit_log_creation() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("audit.log");

        let log = AuditLog::new(path.clone());
        assert!(log.is_ok());
        assert!(path.exists());
    }

    #[test]
    fn test_audit_log_write() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("audit.log");

        let mut log = AuditLog::new(path.clone()).unwrap();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(crate::protocol::RequestId::Number(1)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({"name": "test"})),
        };

        log.log_request(&request).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("tools/call"));
        assert!(content.contains("request"));
    }

    #[test]
    fn test_summarize_params() {
        let obj = serde_json::json!({"path": "/data", "limit": 100});
        let summary = summarize_params(&obj);
        assert!(summary.contains("path"));
        assert!(summary.contains("limit"));

        let arr = serde_json::json!([1, 2, 3]);
        let summary = summarize_params(&arr);
        assert!(summary.contains("3 items"));
    }
}
