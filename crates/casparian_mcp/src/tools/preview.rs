//! Preview tool: preview_files
//!
//! Lets the LLM see file content to make intelligent decisions about
//! which files to process.

use crate::types::{Tool, ToolError, ToolInputSchema, ToolResult, WorkflowMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

// =============================================================================
// Preview Files Tool
// =============================================================================

/// Preview of a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePreview {
    /// File path
    pub path: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// Total line count (estimated for large files)
    pub total_lines: usize,
    /// Whether line count is estimated
    pub lines_estimated: bool,
    /// Starting line number (1-indexed)
    pub start_line: usize,
    /// Lines returned
    pub lines: Vec<String>,
    /// Whether there are more lines after these
    pub has_more: bool,
    /// Detected delimiter (for structured files)
    pub detected_delimiter: Option<String>,
    /// Number of columns (if structured)
    pub column_count: Option<usize>,
    /// Header row (if CSV/TSV)
    pub header: Option<Vec<String>>,
}

/// Result of preview operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewFilesResultOutput {
    /// File previews
    pub previews: Vec<FilePreview>,
    /// Number of files previewed
    pub file_count: usize,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Workflow metadata
    pub workflow: WorkflowMetadata,
}

/// Preview file content to identify which files match target criteria
pub struct PreviewFilesTool;

impl PreviewFilesTool {
    pub fn new() -> Self {
        Self
    }

    /// Detect delimiter from first line
    fn detect_delimiter(line: &str) -> Option<(String, usize)> {
        let delimiters = [(',', "comma"), ('\t', "tab"), ('|', "pipe"), (';', "semicolon")];

        for (delim, name) in delimiters {
            let count = line.matches(delim).count();
            if count > 0 {
                return Some((name.to_string(), count + 1));
            }
        }
        None
    }

    /// Count lines in file (fast estimation for large files)
    fn count_lines(path: &Path, max_scan: usize) -> (usize, bool) {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return (0, false),
        };

        let reader = BufReader::new(file);
        let mut count = 0;

        for _ in reader.lines() {
            count += 1;
            if count >= max_scan {
                // Estimate based on file size
                return (count, true);
            }
        }

        (count, false)
    }

    /// Preview a single file
    fn preview_file(
        &self,
        path: &str,
        lines_count: usize,
        offset: usize,
    ) -> Result<FilePreview, ToolError> {
        let file_path = Path::new(path);

        if !file_path.exists() {
            return Err(ToolError::NotFound(format!("File not found: {}", path)));
        }

        let metadata = std::fs::metadata(file_path)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read metadata: {}", e)))?;

        if metadata.is_dir() {
            return Err(ToolError::InvalidParams(format!("Path is a directory: {}", path)));
        }

        let size_bytes = metadata.len();

        // Count total lines (limit scan for large files)
        let (total_lines, lines_estimated) = Self::count_lines(file_path, 100_000);

        // Read requested lines
        let file = File::open(file_path)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to open file: {}", e)))?;
        let reader = BufReader::new(file);

        let mut lines = Vec::new();
        let mut current_line = 0;
        let mut header: Option<Vec<String>> = None;
        let mut detected_delimiter: Option<String> = None;
        let mut column_count: Option<usize> = None;

        for line_result in reader.lines() {
            let line = line_result
                .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read line: {}", e)))?;

            current_line += 1;

            // Detect structure from first line
            if current_line == 1 {
                if let Some((delim, cols)) = Self::detect_delimiter(&line) {
                    detected_delimiter = Some(delim.clone());
                    column_count = Some(cols);

                    // Parse header if it looks like CSV/TSV
                    let delim_char = match delim.as_str() {
                        "comma" => ',',
                        "tab" => '\t',
                        "pipe" => '|',
                        "semicolon" => ';',
                        _ => ',',
                    };
                    header = Some(line.split(delim_char).map(|s| s.trim().to_string()).collect());
                }
            }

            // Skip to offset
            if current_line <= offset {
                continue;
            }

            // Collect lines up to limit
            if lines.len() < lines_count {
                lines.push(line);
            }

            // Stop if we have enough
            if lines.len() >= lines_count && current_line > offset + lines_count + 1000 {
                break;
            }
        }

        let has_more = current_line > offset + lines.len();
        let start_line = offset + 1;

        Ok(FilePreview {
            path: path.to_string(),
            size_bytes,
            total_lines,
            lines_estimated,
            start_line,
            lines,
            has_more,
            detected_delimiter,
            column_count,
            header,
        })
    }
}

impl Default for PreviewFilesTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PreviewFilesTool {
    fn name(&self) -> &str {
        "preview_files"
    }

    fn description(&self) -> &str {
        "Preview file content to identify which files contain target data. Use after quick_scan to examine promising files. Returns headers, sample rows, and structure info. Supports pagination with offset parameter to explore deeper."
    }

    fn input_schema(&self) -> ToolInputSchema {
        ToolInputSchema::with_properties(
            json!({
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of file paths to preview"
                },
                "lines": {
                    "type": "integer",
                    "description": "Number of lines to return per file (default: 20)",
                    "default": 20
                },
                "offset": {
                    "type": "integer",
                    "description": "Line offset to start from (0-indexed, default: 0). Use to paginate through large files.",
                    "default": 0
                }
            }),
            vec!["files".to_string()],
        )
    }

    async fn execute(&self, args: Value) -> Result<ToolResult, ToolError> {
        let start = std::time::Instant::now();

        // Extract parameters
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

        let lines_count = args
            .get("lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let offset = args
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        // Preview each file
        let mut previews = Vec::new();
        for file in &files {
            match self.preview_file(file, lines_count, offset) {
                Ok(preview) => previews.push(preview),
                Err(e) => {
                    // Include error as a preview with empty content
                    previews.push(FilePreview {
                        path: file.clone(),
                        size_bytes: 0,
                        total_lines: 0,
                        lines_estimated: false,
                        start_line: 0,
                        lines: vec![format!("Error: {}", e)],
                        has_more: false,
                        detected_delimiter: None,
                        column_count: None,
                        header: None,
                    });
                }
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let file_count = previews.len();

        // Workflow: after preview, next step is apply_scope
        let workflow = WorkflowMetadata::discovery();

        let result = PreviewFilesResultOutput {
            previews,
            file_count,
            duration_ms,
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
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_preview_csv() {
        let temp_dir = TempDir::new().unwrap();
        let csv_file = temp_dir.path().join("data.csv");
        fs::write(&csv_file, "id,name,value\n1,Alice,100\n2,Bob,200\n3,Carol,300").unwrap();

        let tool = PreviewFilesTool::new();
        let args = json!({
            "files": [csv_file.to_string_lossy().to_string()],
            "lines": 10
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.is_error);

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let preview: PreviewFilesResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(preview.file_count, 1);

            let file_preview = &preview.previews[0];
            assert_eq!(file_preview.total_lines, 4);
            assert_eq!(file_preview.lines.len(), 4);
            assert_eq!(file_preview.detected_delimiter, Some("comma".to_string()));
            assert_eq!(file_preview.column_count, Some(3));
            assert_eq!(file_preview.header, Some(vec!["id".to_string(), "name".to_string(), "value".to_string()]));
        }
    }

    #[tokio::test]
    async fn test_preview_with_offset() {
        let temp_dir = TempDir::new().unwrap();
        let csv_file = temp_dir.path().join("data.csv");

        let mut content = "id,value\n".to_string();
        for i in 1..=100 {
            content.push_str(&format!("{},{}\n", i, i * 10));
        }
        fs::write(&csv_file, content).unwrap();

        let tool = PreviewFilesTool::new();

        // First page
        let args = json!({
            "files": [csv_file.to_string_lossy().to_string()],
            "lines": 10,
            "offset": 0
        });
        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let preview: PreviewFilesResultOutput = serde_json::from_str(text).unwrap();
            let file_preview = &preview.previews[0];
            assert_eq!(file_preview.start_line, 1);
            assert!(file_preview.has_more);
            assert!(file_preview.lines[0].contains("id,value")); // Header
        }

        // Second page (skip header + first 10 data rows)
        let args = json!({
            "files": [csv_file.to_string_lossy().to_string()],
            "lines": 10,
            "offset": 11
        });
        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let preview: PreviewFilesResultOutput = serde_json::from_str(text).unwrap();
            let file_preview = &preview.previews[0];
            assert_eq!(file_preview.start_line, 12);
            assert!(file_preview.has_more);
        }
    }

    #[tokio::test]
    async fn test_preview_tsv() {
        let temp_dir = TempDir::new().unwrap();
        let tsv_file = temp_dir.path().join("data.tsv");
        fs::write(&tsv_file, "id\tname\tvalue\n1\tAlice\t100").unwrap();

        let tool = PreviewFilesTool::new();
        let args = json!({
            "files": [tsv_file.to_string_lossy().to_string()]
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let preview: PreviewFilesResultOutput = serde_json::from_str(text).unwrap();
            let file_preview = &preview.previews[0];
            assert_eq!(file_preview.detected_delimiter, Some("tab".to_string()));
        }
    }

    #[tokio::test]
    async fn test_preview_nonexistent() {
        let tool = PreviewFilesTool::new();
        let args = json!({
            "files": ["/nonexistent/file.csv"]
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let preview: PreviewFilesResultOutput = serde_json::from_str(text).unwrap();
            // Should still return a preview with error message
            assert_eq!(preview.file_count, 1);
            assert!(preview.previews[0].lines[0].contains("Error"));
        }
    }

    #[tokio::test]
    async fn test_preview_multiple_files() {
        let temp_dir = TempDir::new().unwrap();

        let file1 = temp_dir.path().join("sensor1.csv");
        fs::write(&file1, "timestamp,temp,humidity\n2024-01-01,22.5,45").unwrap();

        let file2 = temp_dir.path().join("sensor2.csv");
        fs::write(&file2, "timestamp,temp,humidity\n2024-01-02,23.1,48").unwrap();

        let file3 = temp_dir.path().join("config.json");
        fs::write(&file3, r#"{"setting": "value"}"#).unwrap();

        let tool = PreviewFilesTool::new();
        let args = json!({
            "files": [
                file1.to_string_lossy().to_string(),
                file2.to_string_lossy().to_string(),
                file3.to_string_lossy().to_string()
            ]
        });

        let result = tool.execute(args).await.unwrap();

        if let Some(crate::types::ToolContent::Text { text }) = result.content.first() {
            let preview: PreviewFilesResultOutput = serde_json::from_str(text).unwrap();
            assert_eq!(preview.file_count, 3);

            // sensor files should be detected as CSV
            assert_eq!(preview.previews[0].detected_delimiter, Some("comma".to_string()));
            assert_eq!(preview.previews[1].detected_delimiter, Some("comma".to_string()));

            // config.json should not have a delimiter detected (no commas in that line)
            // Actually it might detect : as nothing, let's check
        }
    }

    #[test]
    fn test_preview_schema() {
        let tool = PreviewFilesTool::new();
        let schema = tool.input_schema();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.as_ref().unwrap().contains(&"files".to_string()));
    }
}
