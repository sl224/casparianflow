//! Discovery tools: quick_scan, apply_scope
//!
//! These tools handle file discovery and scope management:
//!
//! - `quick_scan`: Fast metadata scan of a directory (stat only, no content reading)
//! - `apply_scope`: Apply a scope to selected files for processing

use crate::types::{BulkApprovalOption, ScopeId, Tool, ToolError, ToolInputSchema, ToolResult, WorkflowMetadata};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

// =============================================================================
// Quick Scan Tool
// =============================================================================

/// Result of scanning a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedFile {
    /// Absolute path to the file
    pub path: String,
    /// File name without directory
    pub name: String,
    /// File size in bytes
    pub size: u64,
    /// Last modified time (ISO 8601)
    pub modified: Option<String>,
    /// File extension (e.g., "csv", "json")
    pub extension: Option<String>,
    /// Whether the file is hidden
    pub is_hidden: bool,
}

/// Result of a quick scan operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickScanResult {
    /// Directory that was scanned
    pub directory: String,
    /// Total number of files found
    pub file_count: usize,
    /// Total size of all files in bytes
    pub total_size: u64,
    /// Files organized by extension
    pub by_extension: std::collections::HashMap<String, Vec<ScannedFile>>,
    /// Scan duration in milliseconds
    pub duration_ms: u64,
    /// Whether the scan was truncated (hit max files limit)
    pub truncated: bool,
    /// Maximum depth reached during scan
    pub max_depth_reached: usize,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Fast metadata scan of a directory (stat only, no content reading)
///
/// This tool quickly scans a directory to gather file metadata without
/// reading file contents. Useful for understanding the scope of files
/// before processing.
pub struct QuickScanTool;

impl QuickScanTool {
    pub fn new() -> Self {
        Self
    }

    /// Perform the actual directory scan
    fn scan_directory(
        &self,
        path: &Path,
        max_files: usize,
        max_depth: usize,
        include_hidden: bool,
    ) -> Result<QuickScanResult, ToolError> {
        let start = std::time::Instant::now();
        let mut files: Vec<ScannedFile> = Vec::new();
        let mut total_size: u64 = 0;
        let mut max_depth_reached = 0;
        let mut truncated = false;

        // Recursive scan helper
        fn scan_recursive(
            dir: &Path,
            files: &mut Vec<ScannedFile>,
            total_size: &mut u64,
            max_files: usize,
            max_depth: usize,
            current_depth: usize,
            max_depth_reached: &mut usize,
            truncated: &mut bool,
            include_hidden: bool,
        ) -> Result<(), ToolError> {
            if current_depth > max_depth {
                return Ok(());
            }

            if files.len() >= max_files {
                *truncated = true;
                return Ok(());
            }

            *max_depth_reached = (*max_depth_reached).max(current_depth);

            let entries = fs::read_dir(dir).map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to read directory {}: {}", dir.display(), e))
            })?;

            for entry in entries {
                if files.len() >= max_files {
                    *truncated = true;
                    break;
                }

                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden files if not requested
                let is_hidden = name.starts_with('.');
                if is_hidden && !include_hidden {
                    continue;
                }

                if path.is_file() {
                    let metadata = match fs::metadata(&path) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    let size = metadata.len();
                    *total_size += size;

                    let modified = metadata.modified().ok().map(|t| {
                        let datetime: DateTime<Utc> = t.into();
                        datetime.to_rfc3339()
                    });

                    let extension = path
                        .extension()
                        .map(|e| e.to_string_lossy().to_string());

                    files.push(ScannedFile {
                        path: path.to_string_lossy().to_string(),
                        name,
                        size,
                        modified,
                        extension,
                        is_hidden,
                    });
                } else if path.is_dir() {
                    scan_recursive(
                        &path,
                        files,
                        total_size,
                        max_files,
                        max_depth,
                        current_depth + 1,
                        max_depth_reached,
                        truncated,
                        include_hidden,
                    )?;
                }
            }

            Ok(())
        }

        scan_recursive(
            path,
            &mut files,
            &mut total_size,
            max_files,
            max_depth,
            0,
            &mut max_depth_reached,
            &mut truncated,
            include_hidden,
        )?;

        // Group by extension
        let mut by_extension: std::collections::HashMap<String, Vec<ScannedFile>> =
            std::collections::HashMap::new();
        for file in &files {
            let ext = file.extension.clone().unwrap_or_else(|| "(none)".to_string());
            by_extension.entry(ext).or_default().push(file.clone());
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        // Build workflow metadata with bulk approval options for each file type
        let mut workflow = WorkflowMetadata::discovery();

        for (ext, ext_files) in &by_extension {
            if ext_files.len() > 1 {
                workflow = workflow.with_bulk_approval(BulkApprovalOption::new(
                    ext.clone(),
                    ext_files.len(),
                    format!("{} {} files", ext_files.len(), ext),
                ));
            }
        }

        Ok(QuickScanResult {
            directory: path.to_string_lossy().to_string(),
            file_count: files.len(),
            total_size,
            by_extension,
            duration_ms,
            truncated,
            max_depth_reached,
            workflow,
        })
    }
}

impl Default for QuickScanTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for QuickScanTool {
    fn name(&self) -> &str {
        "quick_scan"
    }

    fn description(&self) -> &str {
        "Fast metadata scan of a directory (stat only, no content reading). Returns file counts, sizes, and extensions."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "path": {
                    "type": "string",
                    "description": "Directory to scan (absolute path)"
                },
                "max_files": {
                    "type": "integer",
                    "description": "Maximum number of files to scan (default: 1000)",
                    "default": 1000
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum directory depth to scan (default: 5)",
                    "default": 5
                },
                "include_hidden": {
                    "type": "boolean",
                    "description": "Include hidden files (starting with '.') (default: false)",
                    "default": false
                }
            }),
            vec!["path".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Extract parameters
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'path' parameter".into()))?;

        let max_files = args
            .get("max_files")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000) as usize;

        let max_depth = args
            .get("max_depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let include_hidden = args
            .get("include_hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Validate path exists
        let path = Path::new(path);
        if !path.exists() {
            return Err(ToolError::NotFound(format!(
                "Directory not found: {}",
                path.display()
            )));
        }

        if !path.is_dir() {
            return Err(ToolError::InvalidParams(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }

        // Perform scan
        let result = self.scan_directory(path, max_files, max_depth, include_hidden)?;

        ToolResult::json(&result)
    }
}

// =============================================================================
// Apply Scope Tool
// =============================================================================

/// A scope definition that groups files for processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeDefinition {
    /// Unique identifier for this scope
    pub scope_id: ScopeId,
    /// Human-readable name for the scope
    pub name: String,
    /// Description of what this scope represents
    pub description: Option<String>,
    /// File paths included in this scope
    pub files: Vec<String>,
    /// Optional glob pattern that defines this scope
    pub pattern: Option<String>,
    /// When this scope was created
    pub created_at: String,
    /// Tags applied to files in this scope
    pub tags: Vec<String>,
}

/// Result of applying a scope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyScopeResult {
    /// The created/updated scope
    pub scope: ScopeDefinition,
    /// Number of files included
    pub file_count: usize,
    /// Total size of files in bytes
    pub total_size: u64,
    /// Whether any files were skipped
    pub files_skipped: usize,
    /// Reason for skipped files
    pub skip_reasons: Vec<String>,
    /// Workflow metadata for human-in-loop orchestration
    pub workflow: WorkflowMetadata,
}

/// Apply a scope to selected files for processing
///
/// This tool creates a scope (logical grouping) for a set of files.
/// Scopes are used to track which files should be processed together.
pub struct ApplyScopeTool;

impl ApplyScopeTool {
    pub fn new() -> Self {
        Self
    }

    fn validate_files(&self, files: &[String]) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut valid: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        let mut reasons: Vec<String> = Vec::new();

        for file in files {
            let path = Path::new(file);
            if !path.exists() {
                skipped.push(file.clone());
                reasons.push(format!("File not found: {}", file));
            } else if !path.is_file() {
                skipped.push(file.clone());
                reasons.push(format!("Not a file: {}", file));
            } else {
                valid.push(file.clone());
            }
        }

        (valid, skipped, reasons)
    }
}

impl Default for ApplyScopeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ApplyScopeTool {
    fn name(&self) -> &str {
        "apply_scope"
    }

    fn description(&self) -> &str {
        "Apply a scope to selected files for processing. Creates a logical grouping of files that can be processed together."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "name": {
                    "type": "string",
                    "description": "Name for this scope (e.g., 'sales_data_2024')"
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of file paths to include in the scope"
                },
                "description": {
                    "type": "string",
                    "description": "Optional description of what this scope represents"
                },
                "pattern": {
                    "type": "string",
                    "description": "Optional glob pattern that defines this scope (e.g., '*.csv')"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags to apply to files in this scope"
                }
            }),
            vec!["name".to_string(), "files".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        // Extract parameters
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'name' parameter".into()))?;

        let files: Vec<String> = args
            .get("files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidParams("Missing required 'files' parameter".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if files.is_empty() {
            return Err(ToolError::InvalidParams("'files' array cannot be empty".into()));
        }

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);

        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(String::from);

        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Validate files
        let (valid_files, skipped, reasons) = self.validate_files(&files);

        if valid_files.is_empty() {
            return Err(ToolError::InvalidParams(
                "No valid files found in the provided list".into(),
            ));
        }

        // Calculate total size
        let total_size: u64 = valid_files
            .iter()
            .filter_map(|f| fs::metadata(f).ok())
            .map(|m| m.len())
            .sum();

        // Create scope
        let scope_id = ScopeId::new();
        let now: DateTime<Utc> = Utc::now();

        let scope = ScopeDefinition {
            scope_id,
            name: name.to_string(),
            description,
            files: valid_files.clone(),
            pattern,
            created_at: now.to_rfc3339(),
            tags,
        };

        // Build workflow metadata - scope applied, ready for schema discovery
        let workflow = WorkflowMetadata::scope_applied();

        let result = ApplyScopeResult {
            scope,
            file_count: valid_files.len(),
            total_size,
            files_skipped: skipped.len(),
            skip_reasons: reasons,
            workflow,
        };

        ToolResult::json(&result)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_quick_scan_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let tool = QuickScanTool::new();

        let args = json!({
            "path": temp_dir.path().to_string_lossy().to_string()
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_quick_scan_with_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create some test files
        fs::write(temp_dir.path().join("test1.csv"), "a,b,c").unwrap();
        fs::write(temp_dir.path().join("test2.csv"), "d,e,f").unwrap();
        fs::write(temp_dir.path().join("test.json"), "{}").unwrap();

        let tool = QuickScanTool::new();

        let args = json!({
            "path": temp_dir.path().to_string_lossy().to_string()
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        // Parse result
        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let scan_result: QuickScanResult = serde_json::from_str(text).unwrap();
            assert_eq!(scan_result.file_count, 3);
            assert!(scan_result.by_extension.contains_key("csv"));
            assert!(scan_result.by_extension.contains_key("json"));
        }
    }

    #[tokio::test]
    async fn test_quick_scan_invalid_path() {
        let tool = QuickScanTool::new();

        let args = json!({
            "path": "/nonexistent/path/that/does/not/exist"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_quick_scan_missing_param() {
        let tool = QuickScanTool::new();

        let args = json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_apply_scope_basic() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let file1 = temp_dir.path().join("data1.csv");
        let file2 = temp_dir.path().join("data2.csv");
        fs::write(&file1, "a,b,c").unwrap();
        fs::write(&file2, "d,e,f").unwrap();

        let tool = ApplyScopeTool::new();

        let args = json!({
            "name": "test_scope",
            "files": [
                file1.to_string_lossy().to_string(),
                file2.to_string_lossy().to_string()
            ],
            "description": "Test scope for unit tests",
            "tags": ["test", "csv"]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        // Parse result
        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let scope_result: ApplyScopeResult = serde_json::from_str(text).unwrap();
            assert_eq!(scope_result.file_count, 2);
            assert_eq!(scope_result.scope.name, "test_scope");
            assert_eq!(scope_result.files_skipped, 0);
        }
    }

    #[tokio::test]
    async fn test_apply_scope_with_invalid_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create one valid file
        let file1 = temp_dir.path().join("valid.csv");
        fs::write(&file1, "a,b,c").unwrap();

        let tool = ApplyScopeTool::new();

        let args = json!({
            "name": "mixed_scope",
            "files": [
                file1.to_string_lossy().to_string(),
                "/nonexistent/file.csv"
            ]
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        // Parse result
        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let scope_result: ApplyScopeResult = serde_json::from_str(text).unwrap();
            assert_eq!(scope_result.file_count, 1);
            assert_eq!(scope_result.files_skipped, 1);
        }
    }

    #[tokio::test]
    async fn test_apply_scope_missing_name() {
        let tool = ApplyScopeTool::new();

        let args = json!({
            "files": ["/some/file.csv"]
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_apply_scope_empty_files() {
        let tool = ApplyScopeTool::new();

        let args = json!({
            "name": "empty_scope",
            "files": []
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_quick_scan_schema() {
        let tool = QuickScanTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.is_some());
        assert!(schema.required.is_some());
        assert!(schema.required.as_ref().unwrap().contains(&"path".to_string()));
    }

    #[test]
    fn test_apply_scope_schema() {
        let tool = ApplyScopeTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.is_some());
        assert!(schema.required.is_some());
        let required = schema.required.as_ref().unwrap();
        assert!(required.contains(&"name".to_string()));
        assert!(required.contains(&"files".to_string()));
    }
}
