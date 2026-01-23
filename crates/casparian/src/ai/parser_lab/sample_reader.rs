//! Sample File Reader
//!
//! Reads sample files and detects format, schema, and data types.
//! Supports CSV, TSV, JSON, NDJSON, and Parquet files.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use super::ParserLabError;

/// Detected file format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// Comma-separated values
    Csv,
    /// Tab-separated values
    Tsv,
    /// JSON array or object
    Json,
    /// Newline-delimited JSON
    Ndjson,
    /// Apache Parquet
    Parquet,
    /// Unknown format
    Unknown,
}

impl FileFormat {
    /// Get file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            FileFormat::Csv => "csv",
            FileFormat::Tsv => "tsv",
            FileFormat::Json => "json",
            FileFormat::Ndjson => "ndjson",
            FileFormat::Parquet => "parquet",
            FileFormat::Unknown => "",
        }
    }

    /// Get polars read function name
    pub fn polars_reader(&self) -> &'static str {
        match self {
            FileFormat::Csv => "read_csv",
            FileFormat::Tsv => "read_csv",
            FileFormat::Json => "read_json",
            FileFormat::Ndjson => "read_ndjson",
            FileFormat::Parquet => "read_parquet",
            FileFormat::Unknown => "read_csv",
        }
    }
}

impl std::fmt::Display for FileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileFormat::Csv => write!(f, "CSV"),
            FileFormat::Tsv => write!(f, "TSV"),
            FileFormat::Json => write!(f, "JSON"),
            FileFormat::Ndjson => write!(f, "NDJSON"),
            FileFormat::Parquet => write!(f, "Parquet"),
            FileFormat::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Information about a detected column
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    /// Column name
    pub name: String,
    /// Inferred data type
    pub data_type: String,
    /// Whether the column contains null values
    pub nullable: bool,
    /// Sample values (up to 5)
    pub sample_values: Vec<String>,
    /// Detected format (for dates/timestamps)
    pub format: Option<String>,
}

/// Result of sample file analysis
#[derive(Debug, Clone)]
pub struct SampleAnalysis {
    /// Detected file format
    pub format: FileFormat,
    /// Detected delimiter (for CSV/TSV)
    pub delimiter: Option<char>,
    /// Number of rows sampled
    pub sample_row_count: usize,
    /// Total rows (if known)
    pub total_rows: Option<usize>,
    /// Column information
    pub columns: Vec<ColumnInfo>,
    /// Has header row
    pub has_header: bool,
    /// File encoding (if detected)
    pub encoding: Option<String>,
    /// Sample rows (raw strings)
    pub sample_rows: Vec<Vec<String>>,
}

/// Sample file reader and analyzer
pub struct SampleReader {
    /// Maximum rows to sample
    max_sample_rows: usize,
}

impl SampleReader {
    /// Create a new sample reader
    pub fn new() -> Self {
        Self {
            max_sample_rows: 100,
        }
    }

    /// Create with custom sample size
    pub fn with_sample_size(max_rows: usize) -> Self {
        Self {
            max_sample_rows: max_rows,
        }
    }

    /// Analyze sample file(s)
    pub fn analyze(&self, paths: &[String]) -> super::Result<SampleAnalysis> {
        if paths.is_empty() {
            return Err(ParserLabError::NoFiles);
        }

        // Use the first file for analysis
        let path = &paths[0];
        let file_path = Path::new(path);

        // Detect format from extension and content
        let format = self.detect_format(file_path)?;

        match format {
            FileFormat::Csv | FileFormat::Tsv => self.analyze_csv(path, format),
            FileFormat::Json => self.analyze_json(path),
            FileFormat::Ndjson => self.analyze_ndjson(path),
            FileFormat::Parquet => self.analyze_parquet(path),
            FileFormat::Unknown => Err(ParserLabError::UnsupportedFormat(
                file_path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "no extension".to_string()),
            )),
        }
    }

    /// Detect file format from extension and content
    fn detect_format(&self, path: &Path) -> super::Result<FileFormat> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());

        match extension.as_deref() {
            Some("csv") => Ok(FileFormat::Csv),
            Some("tsv") | Some("tab") => Ok(FileFormat::Tsv),
            Some("json") => {
                // Check if it's JSON array or NDJSON
                let file =
                    File::open(path).map_err(|e| ParserLabError::ReadError(e.to_string()))?;
                let mut reader = BufReader::new(file);
                let mut first_line = String::new();
                reader
                    .read_line(&mut first_line)
                    .map_err(|e| ParserLabError::ReadError(e.to_string()))?;

                let trimmed = first_line.trim();
                if trimmed.starts_with('[') {
                    Ok(FileFormat::Json)
                } else if trimmed.starts_with('{') {
                    Ok(FileFormat::Ndjson)
                } else {
                    Ok(FileFormat::Json)
                }
            }
            Some("ndjson") | Some("jsonl") => Ok(FileFormat::Ndjson),
            Some("parquet") | Some("pq") => Ok(FileFormat::Parquet),
            Some(_) | None => self.detect_format_from_content(path),
        }
    }

    /// Detect format by examining file content
    fn detect_format_from_content(&self, path: &Path) -> super::Result<FileFormat> {
        let file = File::open(path).map_err(|e| ParserLabError::ReadError(e.to_string()))?;
        let mut reader = BufReader::new(file);
        let mut first_line = String::new();
        reader
            .read_line(&mut first_line)
            .map_err(|e| ParserLabError::ReadError(e.to_string()))?;

        let trimmed = first_line.trim();

        // Check for JSON
        if trimmed.starts_with('[') || trimmed.starts_with('{') {
            if trimmed.starts_with('[') {
                return Ok(FileFormat::Json);
            } else {
                return Ok(FileFormat::Ndjson);
            }
        }

        // Check for Parquet magic bytes
        if first_line.as_bytes().starts_with(b"PAR1") {
            return Ok(FileFormat::Parquet);
        }

        // Try to detect CSV vs TSV by delimiter
        let comma_count = first_line.matches(',').count();
        let tab_count = first_line.matches('\t').count();

        if tab_count > comma_count && tab_count > 0 {
            Ok(FileFormat::Tsv)
        } else if comma_count > 0 {
            Ok(FileFormat::Csv)
        } else {
            Ok(FileFormat::Unknown)
        }
    }

    /// Analyze CSV/TSV file
    fn analyze_csv(&self, path: &str, format: FileFormat) -> super::Result<SampleAnalysis> {
        let file = File::open(path).map_err(|e| ParserLabError::ReadError(e.to_string()))?;
        let reader = BufReader::new(file);

        let delimiter = if format == FileFormat::Tsv { '\t' } else { ',' };
        let mut lines = reader.lines();
        let mut sample_rows: Vec<Vec<String>> = Vec::new();
        let mut total_lines = 0;

        // Read first line for header
        let header_line = lines
            .next()
            .ok_or_else(|| ParserLabError::ReadError("Empty file".to_string()))?
            .map_err(|e| ParserLabError::ReadError(e.to_string()))?;

        let headers: Vec<String> = self.split_csv_line(&header_line, delimiter);
        let has_header = self.detect_header(&headers);

        if !has_header {
            sample_rows.push(headers.clone());
        }

        // Read sample rows
        for line_result in lines.take(self.max_sample_rows) {
            let line = line_result.map_err(|e| ParserLabError::ReadError(e.to_string()))?;
            let fields = self.split_csv_line(&line, delimiter);
            sample_rows.push(fields);
            total_lines += 1;
        }

        // Infer column types
        let column_names = if has_header {
            headers.clone()
        } else {
            self.generate_column_names(headers.len())
        };
        let columns = self.infer_column_types(&column_names, &sample_rows);

        Ok(SampleAnalysis {
            format,
            delimiter: Some(delimiter),
            sample_row_count: sample_rows.len(),
            total_rows: Some(total_lines + 1),
            columns,
            has_header,
            encoding: Some("utf-8".to_string()),
            sample_rows,
        })
    }

    /// Split CSV line respecting quoted fields
    fn split_csv_line(&self, line: &str, delimiter: char) -> Vec<String> {
        let mut fields = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;

        for ch in line.chars() {
            if ch == '"' {
                in_quotes = !in_quotes;
            } else if ch == delimiter && !in_quotes {
                fields.push(current.trim().to_string());
                current = String::new();
            } else {
                current.push(ch);
            }
        }
        fields.push(current.trim().to_string());
        fields
    }

    /// Detect if first row is a header
    fn detect_header(&self, first_row: &[String]) -> bool {
        // Heuristics:
        // 1. All values are strings (no pure numbers)
        // 2. No empty values
        // 3. Values look like identifiers

        for value in first_row {
            if value.is_empty() {
                return false;
            }
            // If it parses as a number, probably not a header
            if value.parse::<f64>().is_ok() && !value.chars().any(|c| c.is_alphabetic()) {
                return false;
            }
        }
        true
    }

    /// Generate column names for files without headers
    fn generate_column_names(&self, count: usize) -> Vec<String> {
        (0..count).map(|i| format!("column_{}", i + 1)).collect()
    }

    /// Analyze JSON file
    fn analyze_json(&self, path: &str) -> super::Result<SampleAnalysis> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ParserLabError::ReadError(e.to_string()))?;

        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| ParserLabError::ReadError(format!("Invalid JSON: {}", e)))?;

        match value {
            serde_json::Value::Array(arr) => self.analyze_json_array(&arr),
            serde_json::Value::Object(obj) => {
                // Single object - treat as one row
                self.analyze_json_array(&[serde_json::Value::Object(obj)])
            }
            _ => Err(ParserLabError::SchemaInferenceFailed(
                "JSON must be an array or object".to_string(),
            )),
        }
    }

    /// Analyze JSON array
    fn analyze_json_array(&self, arr: &[serde_json::Value]) -> super::Result<SampleAnalysis> {
        let sample_count = arr.len().min(self.max_sample_rows);
        let mut columns: Vec<ColumnInfo> = Vec::new();
        let mut sample_rows: Vec<Vec<String>> = Vec::new();

        // Collect all keys from sampled objects
        let mut all_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        for value in arr.iter().take(sample_count) {
            if let serde_json::Value::Object(obj) = value {
                for key in obj.keys() {
                    all_keys.insert(key.clone());
                }
            }
        }

        // Sort keys for consistent ordering
        let mut keys: Vec<String> = all_keys.into_iter().collect();
        keys.sort();

        // Analyze each column
        for key in &keys {
            let mut sample_values: Vec<String> = Vec::new();
            let mut nullable = false;
            let mut types: Vec<&str> = Vec::new();

            for value in arr.iter().take(sample_count) {
                if let serde_json::Value::Object(obj) = value {
                    match obj.get(key) {
                        Some(serde_json::Value::Null) | None => {
                            nullable = true;
                            sample_values.push("null".to_string());
                        }
                        Some(v) => {
                            let type_str = self.json_type(v);
                            types.push(type_str);
                            sample_values.push(self.json_value_to_string(v));
                        }
                    }
                }
            }

            let data_type = self.most_common_type(&types);

            columns.push(ColumnInfo {
                name: key.clone(),
                data_type: data_type.to_string(),
                nullable,
                sample_values: sample_values.into_iter().take(5).collect(),
                format: None,
            });
        }

        // Build sample rows
        for value in arr.iter().take(sample_count) {
            if let serde_json::Value::Object(obj) = value {
                let row: Vec<String> = keys
                    .iter()
                    .map(|k| match obj.get(k) {
                        Some(serde_json::Value::Null) | None => "null".to_string(),
                        Some(v) => self.json_value_to_string(v),
                    })
                    .collect();
                sample_rows.push(row);
            }
        }

        Ok(SampleAnalysis {
            format: FileFormat::Json,
            delimiter: None,
            sample_row_count: sample_count,
            total_rows: Some(arr.len()),
            columns,
            has_header: true,
            encoding: Some("utf-8".to_string()),
            sample_rows,
        })
    }

    /// Get JSON value type
    fn json_type(&self, value: &serde_json::Value) -> &'static str {
        match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            serde_json::Value::Number(n) => {
                if n.is_i64() {
                    "int64"
                } else {
                    "float64"
                }
            }
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "list",
            serde_json::Value::Object(_) => "struct",
        }
    }

    /// Convert JSON value to string
    fn json_value_to_string(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            _ => value.to_string(),
        }
    }

    /// Analyze NDJSON file
    fn analyze_ndjson(&self, path: &str) -> super::Result<SampleAnalysis> {
        let file = File::open(path).map_err(|e| ParserLabError::ReadError(e.to_string()))?;
        let reader = BufReader::new(file);

        let mut objects: Vec<serde_json::Value> = Vec::new();
        for line_result in reader.lines().take(self.max_sample_rows) {
            let line = line_result.map_err(|e| ParserLabError::ReadError(e.to_string()))?;
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| ParserLabError::ReadError(format!("Invalid JSON line: {}", e)))?;
            objects.push(value);
        }

        let mut analysis = self.analyze_json_array(&objects)?;
        analysis.format = FileFormat::Ndjson;
        Ok(analysis)
    }

    /// Analyze Parquet file (limited support without polars)
    fn analyze_parquet(&self, _path: &str) -> super::Result<SampleAnalysis> {
        // For now, return a basic analysis indicating parquet format
        // Full parquet reading would require polars dependency
        Ok(SampleAnalysis {
            format: FileFormat::Parquet,
            delimiter: None,
            sample_row_count: 0,
            total_rows: None,
            columns: vec![],
            has_header: true,
            encoding: None,
            sample_rows: vec![],
        })
    }

    /// Infer column types from sample data
    fn infer_column_types(
        &self,
        headers: &[String],
        sample_rows: &[Vec<String>],
    ) -> Vec<ColumnInfo> {
        headers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let values: Vec<&str> = sample_rows
                    .iter()
                    .filter_map(|row| row.get(i).map(|s| s.as_str()))
                    .collect();

                let (data_type, format) = self.infer_type(&values);
                let nullable = values
                    .iter()
                    .any(|v| v.is_empty() || *v == "null" || *v == "NULL" || *v == "NA");
                let sample_values: Vec<String> =
                    values.iter().take(5).map(|s| s.to_string()).collect();

                ColumnInfo {
                    name: name.clone(),
                    data_type,
                    nullable,
                    sample_values,
                    format,
                }
            })
            .collect()
    }

    /// Infer type from sample values
    fn infer_type(&self, values: &[&str]) -> (String, Option<String>) {
        let non_empty: Vec<&str> = values
            .iter()
            .copied()
            .filter(|v| !v.is_empty() && v != &"null" && v != &"NULL" && v != &"NA")
            .collect();

        if non_empty.is_empty() {
            return ("string".to_string(), None);
        }

        // Check for boolean
        if non_empty.iter().all(|v| {
            let lower = v.to_lowercase();
            lower == "true" || lower == "false" || *v == "0" || *v == "1"
        }) {
            return ("boolean".to_string(), None);
        }

        // Check for integer
        if non_empty.iter().all(|v| v.parse::<i64>().is_ok()) {
            return ("int64".to_string(), None);
        }

        // Check for float
        if non_empty.iter().all(|v| v.parse::<f64>().is_ok()) {
            return ("float64".to_string(), None);
        }

        // Check for date patterns
        if let Some(format) = self.detect_date_format(&non_empty) {
            return ("date".to_string(), Some(format));
        }

        // Check for timestamp patterns
        if let Some(format) = self.detect_timestamp_format(&non_empty) {
            return ("timestamp".to_string(), Some(format));
        }

        ("string".to_string(), None)
    }

    /// Detect date format from sample values
    fn detect_date_format(&self, values: &[&str]) -> Option<String> {
        let formats = [
            ("%Y-%m-%d", r"^\d{4}-\d{2}-\d{2}$"),
            ("%d/%m/%Y", r"^\d{2}/\d{2}/\d{4}$"),
            ("%m/%d/%Y", r"^\d{2}/\d{2}/\d{4}$"),
            ("%Y/%m/%d", r"^\d{4}/\d{2}/\d{2}$"),
            ("%d-%m-%Y", r"^\d{2}-\d{2}-\d{4}$"),
            ("%m-%d-%Y", r"^\d{2}-\d{2}-\d{4}$"),
        ];

        for (format, pattern) in formats {
            let re = regex::Regex::new(pattern).ok()?;
            if values.iter().all(|v| re.is_match(v)) {
                return Some(format.to_string());
            }
        }

        None
    }

    /// Detect timestamp format from sample values
    fn detect_timestamp_format(&self, values: &[&str]) -> Option<String> {
        let formats = [
            ("%Y-%m-%dT%H:%M:%S", r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}"),
            ("%Y-%m-%d %H:%M:%S", r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}"),
            ("%d/%m/%Y %H:%M:%S", r"^\d{2}/\d{2}/\d{4} \d{2}:\d{2}:\d{2}"),
        ];

        for (format, pattern) in formats {
            let re = regex::Regex::new(pattern).ok()?;
            if values.iter().all(|v| re.is_match(v)) {
                return Some(format.to_string());
            }
        }

        None
    }

    /// Get most common type from a list
    fn most_common_type<'a>(&self, types: &[&'a str]) -> &'a str {
        if types.is_empty() {
            return "string";
        }

        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for t in types {
            *counts.entry(t).or_insert(0) += 1;
        }

        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(t, _)| t)
            .unwrap_or("string")
    }
}

impl Default for SampleReader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_csv_format() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.csv");
        std::fs::write(&path, "a,b,c\n1,2,3\n4,5,6").unwrap();

        let reader = SampleReader::new();
        let format = reader.detect_format(&path).unwrap();
        assert_eq!(format, FileFormat::Csv);
    }

    #[test]
    fn test_detect_json_format() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.json");
        std::fs::write(&path, r#"[{"a": 1}, {"a": 2}]"#).unwrap();

        let reader = SampleReader::new();
        let format = reader.detect_format(&path).unwrap();
        assert_eq!(format, FileFormat::Json);
    }

    #[test]
    fn test_analyze_csv() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.csv");
        std::fs::write(&path, "name,age,active\nAlice,30,true\nBob,25,false").unwrap();

        let reader = SampleReader::new();
        let analysis = reader
            .analyze(&[path.to_string_lossy().to_string()])
            .unwrap();

        assert_eq!(analysis.format, FileFormat::Csv);
        assert!(analysis.has_header);
        assert_eq!(analysis.columns.len(), 3);
        assert_eq!(analysis.columns[0].name, "name");
        assert_eq!(analysis.columns[0].data_type, "string");
        assert_eq!(analysis.columns[1].name, "age");
        assert_eq!(analysis.columns[1].data_type, "int64");
        assert_eq!(analysis.columns[2].name, "active");
        assert_eq!(analysis.columns[2].data_type, "boolean");
    }

    #[test]
    fn test_infer_date_type() {
        let reader = SampleReader::new();
        let values = vec!["2024-01-15", "2024-02-20", "2024-03-10"];
        let (data_type, format) = reader.infer_type(&values);
        assert_eq!(data_type, "date");
        assert_eq!(format, Some("%Y-%m-%d".to_string()));
    }

    #[test]
    fn test_split_csv_with_quotes() {
        let reader = SampleReader::new();
        let line = r#"hello,"world, test",foo"#;
        let fields = reader.split_csv_line(line, ',');
        assert_eq!(fields, vec!["hello", "world, test", "foo"]);
    }
}
