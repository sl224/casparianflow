//! MCP tools for file set operations (§7.3)
//!
//! - `casp.fileset.sample` → Get bounded sample from a file set
//! - `casp.fileset.page` → Paginate through a file set

// Sync tool implementations (no async)
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::CoreHandle;
use crate::intent::fileset::{FileSetPage, FileSetSample, FileSetStore};
use crate::intent::session::{FileSetEntry, SessionStore};
use crate::intent::types::{FileSetId, FileSetMeta, SessionId};
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::tools::McpTool;

// ============================================================================
// FileSet Sample Tool
// ============================================================================

/// Tool: casp.fileset.sample
pub struct FileSetSampleTool;

#[derive(Debug, Deserialize)]
struct FileSetSampleArgs {
    /// Session ID
    session_id: SessionId,
    /// File set ID to sample from
    file_set_id: FileSetId,
    /// Number of examples to return (bounded)
    #[serde(default = "default_sample_size")]
    n: usize,
    /// Optional seed for reproducibility
    #[serde(default)]
    seed: Option<u64>,
}

fn default_sample_size() -> usize {
    25
}

#[derive(Debug, Serialize)]
struct FileSetSampleResponse {
    file_set_id: FileSetId,
    examples: Vec<String>,
    total_count: u64,
    sample_size: usize,
}

impl McpTool for FileSetSampleTool {
    fn name(&self) -> &'static str {
        "casp_fileset_sample"
    }

    fn description(&self) -> &'static str {
        "Get a bounded random sample of files from a file set. Returns file paths only, never full contents."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID"
                },
                "file_set_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "File set ID to sample from"
                },
                "n": {
                    "type": "integer",
                    "description": "Number of examples to return (default: 25, max: 100)",
                    "minimum": 1,
                    "maximum": 100
                },
                "seed": {
                    "type": "integer",
                    "description": "Optional seed for reproducible sampling"
                }
            },
            "required": ["session_id", "file_set_id"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: FileSetSampleArgs = serde_json::from_value(args)?;

        // Bound the sample size
        let n = args.n.min(100);

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Create a temporary fileset store to access the data
        let fs_store = FileSetStore::new();
        let samples = fs_store.sample(&bundle, args.file_set_id, n, args.seed)?;

        // Get total count from the file
        let all_entries = bundle.read_fileset(args.file_set_id)?;
        let total_count = all_entries.len() as u64;

        let response = FileSetSampleResponse {
            file_set_id: args.file_set_id,
            examples: samples.into_iter().map(|e| e.path).collect(),
            total_count,
            sample_size: n.min(total_count as usize),
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// FileSet Page Tool
// ============================================================================

/// Tool: casp.fileset.page
pub struct FileSetPageTool;

#[derive(Debug, Deserialize)]
struct FileSetPageArgs {
    /// Session ID
    session_id: SessionId,
    /// File set ID to page through
    file_set_id: FileSetId,
    /// Cursor for pagination (from previous response)
    #[serde(default)]
    cursor: Option<String>,
    /// Page size (bounded)
    #[serde(default = "default_page_size")]
    limit: usize,
}

fn default_page_size() -> usize {
    50
}

#[derive(Debug, Serialize)]
struct FileSetPageResponse {
    file_set_id: FileSetId,
    items: Vec<FileSetItemResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
    page_size: usize,
}

#[derive(Debug, Serialize)]
struct FileSetItemResponse {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
}

impl From<FileSetEntry> for FileSetItemResponse {
    fn from(entry: FileSetEntry) -> Self {
        Self {
            path: entry.path,
            size: entry.size,
            content_hash: entry.content_hash,
        }
    }
}

impl McpTool for FileSetPageTool {
    fn name(&self) -> &'static str {
        "casp_fileset_page"
    }

    fn description(&self) -> &'static str {
        "Paginate through a file set. Use the next_cursor from the response to get the next page."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID"
                },
                "file_set_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "File set ID to page through"
                },
                "cursor": {
                    "type": "string",
                    "description": "Cursor from previous response for pagination"
                },
                "limit": {
                    "type": "integer",
                    "description": "Page size (default: 50, max: 200)",
                    "minimum": 1,
                    "maximum": 200
                }
            },
            "required": ["session_id", "file_set_id"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: FileSetPageArgs = serde_json::from_value(args)?;

        // Bound the page size
        let limit = args.limit.min(200);

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        let fs_store = FileSetStore::new();
        let cursor_offset = args.cursor.and_then(|c| c.parse().ok());

        let page = fs_store.page(&bundle, args.file_set_id, cursor_offset, limit)?;

        let response = FileSetPageResponse {
            file_set_id: args.file_set_id,
            items: page
                .items
                .into_iter()
                .map(FileSetItemResponse::from)
                .collect(),
            next_cursor: page.next_cursor,
            page_size: limit,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// FileSet Info Tool
// ============================================================================

/// Tool: casp.fileset.info
pub struct FileSetInfoTool;

#[derive(Debug, Deserialize)]
struct FileSetInfoArgs {
    /// Session ID
    session_id: SessionId,
    /// File set ID to get info for
    file_set_id: FileSetId,
}

#[derive(Debug, Serialize)]
struct FileSetInfoResponse {
    file_set_id: FileSetId,
    count: u64,
    sampling_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u64>,
    manifest_ref: String,
    created_at: String,
}

impl McpTool for FileSetInfoTool {
    fn name(&self) -> &'static str {
        "casp_fileset_info"
    }

    fn description(&self) -> &'static str {
        "Get metadata about a file set including count, sampling method, and creation time."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID"
                },
                "file_set_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "File set ID to get info for"
                }
            },
            "required": ["session_id", "file_set_id"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: FileSetInfoArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Read the fileset to get count (meta is stored in the store)
        let entries = bundle.read_fileset(args.file_set_id)?;

        let response = FileSetInfoResponse {
            file_set_id: args.file_set_id,
            count: entries.len() as u64,
            sampling_method: "all".to_string(), // Would come from metadata
            seed: None,
            manifest_ref: format!("corpora/filesets/{}.jsonl", args.file_set_id),
            created_at: chrono::Utc::now().to_rfc3339(), // Would come from metadata
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fileset_sample_args_deserialize() {
        let session_id = SessionId::new();
        let file_set_id = FileSetId::new();

        let json = json!({
            "session_id": session_id.to_string(),
            "file_set_id": file_set_id.to_string(),
            "n": 10
        });

        let args: FileSetSampleArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.session_id, session_id);
        assert_eq!(args.file_set_id, file_set_id);
        assert_eq!(args.n, 10);
    }

    #[test]
    fn test_fileset_page_args_default() {
        let session_id = SessionId::new();
        let file_set_id = FileSetId::new();

        let json = json!({
            "session_id": session_id.to_string(),
            "file_set_id": file_set_id.to_string()
        });

        let args: FileSetPageArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.limit, 50); // default
        assert!(args.cursor.is_none());
    }
}
