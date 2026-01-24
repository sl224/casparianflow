//! casparian_scan - Discover Files in a Directory
//!
//! Scans a directory and returns file metadata. Path must be within allowed roots.

use super::McpTool;
use crate::core::CoreHandle;
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::SystemTime;

pub struct ScanTool;

#[derive(Debug, Deserialize)]
struct ScanArgs {
    path: String,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default = "default_true")]
    recursive: bool,
    #[serde(default)]
    hash_mode: HashMode,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_true() -> bool {
    true
}

fn default_limit() -> usize {
    1000
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum HashMode {
    #[default]
    None,
    Fast,
    Sha256,
}

#[derive(Debug, Serialize)]
struct FileInfo {
    path: String,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
}

#[derive(Debug, Serialize)]
struct ScanResult {
    files: Vec<FileInfo>,
    total_size: u64,
    file_count: usize,
    truncated: bool,
}

impl McpTool for ScanTool {
    fn name(&self) -> &'static str {
        "casparian_scan"
    }

    fn description(&self) -> &'static str {
        "Discover files in a directory"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to scan (must be within allowed paths)"
                },
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g., *.evtx)"
                },
                "recursive": {
                    "type": "boolean",
                    "default": true
                },
                "hash_mode": {
                    "type": "string",
                    "enum": ["none", "fast", "sha256"],
                    "default": "none",
                    "description": "none=skip hashing, fast=xxhash, sha256=full hash"
                },
                "limit": {
                    "type": "integer",
                    "default": 1000
                }
            },
            "required": ["path"]
        })
    }

    fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> Result<Value> {
        let args: ScanArgs = serde_json::from_value(args)?;

        // Validate path against security allowlist
        let path = PathBuf::from(&args.path);
        let canonical_path = security.validate_path(&path)?;

        // Collect files
        let mut files = Vec::new();
        let mut total_size = 0u64;

        let walker = if args.recursive {
            walkdir::WalkDir::new(&canonical_path)
        } else {
            walkdir::WalkDir::new(&canonical_path).max_depth(1)
        };

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }

            // Check glob pattern if specified
            if let Some(ref pattern) = args.pattern {
                let glob = glob::Pattern::new(pattern)?;
                let file_name = entry.file_name().to_string_lossy();
                if !glob.matches(&file_name) {
                    continue;
                }
            }

            let metadata = entry.metadata()?;
            let size = metadata.len();
            total_size += size;

            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .and_then(|d| {
                    chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0)
                })
                .map(|dt| dt.to_rfc3339());

            let hash = match args.hash_mode {
                HashMode::None => None,
                HashMode::Fast | HashMode::Sha256 => {
                    // TODO: Implement hashing
                    None
                }
            };

            files.push(FileInfo {
                path: entry.path().display().to_string(),
                size,
                modified,
                hash,
            });

            if files.len() >= args.limit {
                break;
            }
        }

        let truncated = files.len() >= args.limit;
        let file_count = files.len();

        let result = ScanResult {
            files,
            total_size,
            file_count,
            truncated,
        };

        Ok(serde_json::to_value(result)?)
    }
}
