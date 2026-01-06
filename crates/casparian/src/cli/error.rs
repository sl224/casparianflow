//! Helpful error types for CLI commands
//!
//! Every error includes:
//! - What went wrong
//! - Context about the situation
//! - Suggestions for how to fix it

use std::fmt;
use std::path::Path;

/// An error with helpful context and suggestions
#[derive(Debug)]
pub struct HelpfulError {
    /// The main error message
    pub message: String,
    /// Additional context about what was happening
    pub context: Option<String>,
    /// Suggestions for how to fix the error
    pub suggestions: Vec<String>,
}

impl HelpfulError {
    /// Create a new helpful error
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            context: None,
            suggestions: Vec::new(),
        }
    }

    /// Add context to the error
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Add a suggestion for fixing the error
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    /// Add multiple suggestions
    pub fn with_suggestions(mut self, suggestions: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.suggestions.extend(suggestions.into_iter().map(|s| s.into()));
        self
    }

    // === Common error constructors ===

    /// Path does not exist
    pub fn path_not_found(path: &Path) -> Self {
        Self::new(format!("Path not found: {}", path.display()))
            .with_context("The specified path does not exist on the filesystem")
            .with_suggestions([
                format!("TRY: Check that the path exists: ls -la {}", path.display()),
                "TRY: Verify you have read permissions for this path".to_string(),
                "TRY: Check for typos in the path".to_string(),
            ])
    }

    /// Path exists but is not a directory
    pub fn not_a_directory(path: &Path) -> Self {
        Self::new(format!("Not a directory: {}", path.display()))
            .with_context("The scan command expects a directory, not a file")
            .with_suggestions([
                format!("TRY: Use 'preview' to inspect a single file: casparian preview {}", path.display()),
                format!("TRY: Scan the parent directory: casparian scan {}",
                    path.parent().map(|p| p.display().to_string()).unwrap_or_else(|| ".".to_string())),
            ])
    }

    /// File does not exist
    pub fn file_not_found(path: &Path) -> Self {
        Self::new(format!("File not found: {}", path.display()))
            .with_context("The specified file does not exist")
            .with_suggestions([
                format!("TRY: Check if the file exists: ls -la {}", path.display()),
                format!("TRY: Look for similar files: ls {}",
                    path.parent().map(|p| p.display().to_string()).unwrap_or_else(|| ".".to_string())),
            ])
    }

    /// File type is not recognized
    #[allow(dead_code)]
    pub fn unknown_file_type(path: &Path) -> Self {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("(no extension)");

        Self::new(format!("Unknown file type: {}", ext))
            .with_context(format!("Cannot preview file: {}", path.display()))
            .with_suggestions([
                "TRY: Use --raw to view file as raw bytes".to_string(),
                "TRY: Supported types: csv, json, jsonl, ndjson, parquet, txt, log".to_string(),
            ])
    }

    /// File cannot be read (permission or encoding error)
    pub fn cannot_read_file(path: &Path, reason: &str) -> Self {
        Self::new(format!("Cannot read file: {}", path.display()))
            .with_context(reason.to_string())
            .with_suggestions([
                format!("TRY: Check file permissions: ls -la {}", path.display()),
                "TRY: Ensure the file is not open in another program".to_string(),
                "TRY: Use --raw to view binary content".to_string(),
            ])
    }

    /// Invalid size format
    pub fn invalid_size_format(size_str: &str) -> Self {
        Self::new(format!("Invalid size format: '{}'", size_str))
            .with_context("Size must be a number followed by a unit")
            .with_suggestions([
                "TRY: Use formats like: 100, 1KB, 10MB, 1GB".to_string(),
                "TRY: Valid units: B, KB, MB, GB, TB (case insensitive)".to_string(),
                "TRY: Examples: --min-size 1MB --max-size 100MB".to_string(),
            ])
    }

    /// CSV parsing error
    pub fn csv_parse_error(path: &Path, line: usize, details: &str) -> Self {
        Self::new(format!("CSV parse error at line {}: {}", line, details))
            .with_context(format!("Failed to parse CSV file: {}", path.display()))
            .with_suggestions([
                "TRY: Check if the delimiter is correct (use --delimiter)".to_string(),
                "TRY: Verify the CSV file is well-formed".to_string(),
                format!("TRY: Inspect the raw file: head -n {} {}", line + 5, path.display()),
            ])
    }

    /// JSON parsing error
    pub fn json_parse_error(path: &Path, details: &str) -> Self {
        Self::new(format!("JSON parse error: {}", details))
            .with_context(format!("Failed to parse JSON file: {}", path.display()))
            .with_suggestions([
                "TRY: Validate the JSON: cat FILE | python -m json.tool".to_string(),
                "TRY: For line-delimited JSON, each line must be valid JSON".to_string(),
                "TRY: Use --raw to view the raw file content".to_string(),
            ])
    }

    /// Parquet error
    pub fn parquet_error(path: &Path, details: &str) -> Self {
        Self::new(format!("Parquet error: {}", details))
            .with_context(format!("Failed to read Parquet file: {}", path.display()))
            .with_suggestions([
                "TRY: Verify this is a valid Parquet file".to_string(),
                "TRY: Check if the file was fully written (not truncated)".to_string(),
            ])
    }
}

impl fmt::Display for HelpfulError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ERROR: {}", self.message)?;

        if let Some(ctx) = &self.context {
            writeln!(f, "CONTEXT: {}", ctx)?;
        }

        if !self.suggestions.is_empty() {
            writeln!(f)?;
            for suggestion in &self.suggestions {
                writeln!(f, "  {}", suggestion)?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for HelpfulError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_helpful_error_display() {
        let err = HelpfulError::new("Something went wrong")
            .with_context("While processing data")
            .with_suggestion("Try again");

        let display = format!("{}", err);
        assert!(display.contains("ERROR: Something went wrong"));
        assert!(display.contains("CONTEXT: While processing data"));
        assert!(display.contains("Try again"));
    }

    #[test]
    fn test_path_not_found() {
        let path = PathBuf::from("/nonexistent/path");
        let err = HelpfulError::path_not_found(&path);

        let display = format!("{}", err);
        assert!(display.contains("/nonexistent/path"));
        assert!(display.contains("TRY:"));
    }

    #[test]
    fn test_invalid_size_format() {
        let err = HelpfulError::invalid_size_format("abc");

        let display = format!("{}", err);
        assert!(display.contains("abc"));
        assert!(display.contains("KB"));
        assert!(display.contains("MB"));
    }
}
