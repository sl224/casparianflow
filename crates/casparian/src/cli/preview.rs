//! Preview command - Preview file contents and infer schema
//!
//! Supports multiple file formats:
//! - CSV/TSV: Parse with delimiter detection and type inference
//! - JSON: Single object or array
//! - NDJSON/JSONL: Line-delimited JSON
//! - Parquet: Read schema and sample rows
//! - Text/Log: Line-based preview
//! - Binary: Hex dump

use crate::cli::error::HelpfulError;
use crate::cli::output::{format_size, print_table};
use arrow::array::ArrayRef;
use arrow::datatypes::DataType;
use csv::ReaderBuilder;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use arrow::error::ArrowError;
use parquet::errors::ParquetError;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

/// Arguments for the preview command
#[derive(Debug)]
pub struct PreviewArgs {
    pub file: PathBuf,
    pub rows: usize,
    pub schema: bool,
    pub raw: bool,
    pub head: Option<usize>,
    pub delimiter: Option<char>,
    pub json: bool,
}

/// Detected file type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FileType {
    Csv,
    Tsv,
    Json,
    NdJson,
    Parquet,
    Text,
    Binary,
}

impl FileType {
    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "csv" => Some(FileType::Csv),
            "tsv" => Some(FileType::Tsv),
            "json" => Some(FileType::Json),
            "jsonl" | "ndjson" => Some(FileType::NdJson),
            "parquet" | "pq" => Some(FileType::Parquet),
            "txt" | "log" | "md" | "yml" | "yaml" | "toml" | "ini" | "cfg" | "conf" => {
                Some(FileType::Text)
            }
            _ => None,
        }
    }

    fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
    }
}

/// Inferred column schema
#[derive(Debug, Clone, Serialize)]
pub struct ColumnSchema {
    pub name: String,
    pub inferred_type: String,
    pub nullable: bool,
    pub sample_values: Vec<String>,
}

/// Complete preview result
#[derive(Debug, Serialize)]
pub struct PreviewResult {
    pub file_path: PathBuf,
    pub file_type: FileType,
    pub file_size: u64,
    pub schema: Option<Vec<ColumnSchema>>,
    pub row_count: usize,
    pub preview_rows: Vec<Vec<String>>,
    pub headers: Vec<String>,
}

/// Execute the preview command
pub fn run(args: PreviewArgs) -> anyhow::Result<()> {
    // Validate file exists
    if !args.file.exists() {
        return Err(HelpfulError::file_not_found(&args.file).into());
    }

    // Validate it's a file, not a directory
    if args.file.is_dir() {
        return Err(HelpfulError::new(format!("Not a file: {}", args.file.display()))
            .with_context("The preview command expects a file, not a directory")
            .with_suggestion(format!(
                "TRY: Use 'scan' to list files: casparian scan {}",
                args.file.display()
            ))
            .into());
    }

    // Get file size
    let metadata = fs::metadata(&args.file)?;
    let file_size = metadata.len();

    // Handle raw mode
    if args.raw {
        return preview_raw(&args.file, args.head.unwrap_or(100));
    }

    // Detect file type
    let file_type = FileType::from_path(&args.file).unwrap_or_else(|| detect_file_type(&args.file));

    // Handle head mode for text files
    if let Some(head_lines) = args.head {
        return preview_head(&args.file, head_lines);
    }

    // Preview based on file type
    let result = match file_type {
        FileType::Csv => preview_csv(&args.file, args.rows, args.delimiter.unwrap_or(','))?,
        FileType::Tsv => preview_csv(&args.file, args.rows, args.delimiter.unwrap_or('\t'))?,
        FileType::Json => preview_json(&args.file, args.rows)?,
        FileType::NdJson => preview_ndjson(&args.file, args.rows)?,
        FileType::Parquet => preview_parquet(&args.file, args.rows)?,
        FileType::Text => preview_text(&args.file, args.rows)?,
        FileType::Binary => {
            return preview_raw(&args.file, args.head.unwrap_or(100));
        }
    };

    // Add file metadata to result
    let result = PreviewResult {
        file_path: args.file.clone(),
        file_type,
        file_size,
        ..result
    };

    // Output
    if args.json {
        output_json(&result)?;
    } else if args.schema {
        output_schema(&result);
    } else {
        output_preview(&result);
    }

    Ok(())
}

/// Detect file type by examining content
fn detect_file_type(path: &Path) -> FileType {
    // Read first few bytes
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return FileType::Binary,
    };

    let mut buffer = [0u8; 1024];
    let bytes_read = match file.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return FileType::Binary,
    };

    if bytes_read == 0 {
        return FileType::Text;
    }

    let content = &buffer[..bytes_read];

    // Check for Parquet magic bytes
    if bytes_read >= 4 && &content[..4] == b"PAR1" {
        return FileType::Parquet;
    }

    // Check if it's valid UTF-8
    if let Ok(text) = std::str::from_utf8(content) {
        let trimmed = text.trim();

        // Check for JSON
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return FileType::Json;
        }

        // Check for NDJSON (multiple JSON lines)
        let lines: Vec<&str> = text.lines().take(3).collect();
        if lines.len() > 1
            && lines.iter().all(|l| {
                let t = l.trim();
                t.starts_with('{') && t.ends_with('}')
            })
        {
            return FileType::NdJson;
        }

        // Check for CSV-like content (contains commas or tabs)
        if text.contains(',') && text.contains('\n') {
            return FileType::Csv;
        }
        if text.contains('\t') && text.contains('\n') {
            return FileType::Tsv;
        }

        return FileType::Text;
    }

    FileType::Binary
}

/// Preview CSV file
fn preview_csv(path: &Path, rows: usize, delimiter: char) -> anyhow::Result<PreviewResult> {
    let file = File::open(path)
        .map_err(|e| HelpfulError::cannot_read_file(path, &e.to_string()))?;

    let mut reader = ReaderBuilder::new()
        .delimiter(delimiter as u8)
        .flexible(true)
        .from_reader(file);

    // Get headers
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e: csv::Error| HelpfulError::csv_parse_error(path, 1, &e.to_string()))?
        .iter()
        .map(|s: &str| s.to_string())
        .collect();

    // Read rows and track types
    let mut preview_rows: Vec<Vec<String>> = Vec::new();
    let mut type_trackers: Vec<TypeTracker> = vec![TypeTracker::new(); headers.len()];

    for (idx, result) in reader.records().take(rows).enumerate() {
        let record = result.map_err(|e: csv::Error| HelpfulError::csv_parse_error(path, idx + 2, &e.to_string()))?;

        let row: Vec<String> = record.iter().map(|s: &str| s.to_string()).collect();

        // Update type trackers
        for (i, value) in row.iter().enumerate() {
            if i < type_trackers.len() {
                type_trackers[i].observe(value);
            }
        }

        preview_rows.push(row);
    }

    // Build schema
    let schema: Vec<ColumnSchema> = headers
        .iter()
        .enumerate()
        .map(|(i, name): (usize, &String)| {
            let tracker: TypeTracker = type_trackers.get(i).cloned().unwrap_or_default();
            ColumnSchema {
                name: name.clone(),
                inferred_type: tracker.inferred_type(),
                nullable: tracker.has_nulls,
                sample_values: tracker.samples,
            }
        })
        .collect();

    Ok(PreviewResult {
        file_path: path.to_path_buf(),
        file_type: FileType::Csv,
        file_size: 0,
        schema: Some(schema),
        row_count: preview_rows.len(),
        preview_rows,
        headers,
    })
}

/// Preview JSON file
fn preview_json(path: &Path, rows: usize) -> anyhow::Result<PreviewResult> {
    let content = fs::read_to_string(path)
        .map_err(|e| HelpfulError::cannot_read_file(path, &e.to_string()))?;

    let value: JsonValue = serde_json::from_str(&content)
        .map_err(|e| HelpfulError::json_parse_error(path, &e.to_string()))?;

    match value {
        JsonValue::Array(arr) => preview_json_array(path, &arr, rows),
        JsonValue::Object(obj) => preview_json_object(path, &JsonValue::Object(obj)),
        _ => Err(HelpfulError::json_parse_error(
            path,
            "Expected JSON object or array at root",
        )
        .into()),
    }
}

/// Preview JSON array
fn preview_json_array(path: &Path, arr: &[JsonValue], rows: usize) -> anyhow::Result<PreviewResult> {
    if arr.is_empty() {
        return Ok(PreviewResult {
            file_path: path.to_path_buf(),
            file_type: FileType::Json,
            file_size: 0,
            schema: Some(vec![]),
            row_count: 0,
            preview_rows: vec![],
            headers: vec![],
        });
    }

    // Collect all unique keys from the first N objects
    let mut all_keys: Vec<String> = Vec::new();
    for item in arr.iter().take(rows) {
        if let JsonValue::Object(obj) = item {
            for key in obj.keys() {
                if !all_keys.contains(key) {
                    all_keys.push(key.clone());
                }
            }
        }
    }

    // Build rows
    let mut preview_rows: Vec<Vec<String>> = Vec::new();
    let mut type_trackers: Vec<TypeTracker> = vec![TypeTracker::new(); all_keys.len()];

    for item in arr.iter().take(rows) {
        if let JsonValue::Object(obj) = item {
            let row: Vec<String> = all_keys
                .iter()
                .enumerate()
                .map(|(i, key): (usize, &String)| {
                    let value = obj.get(key).cloned().unwrap_or(JsonValue::Null);
                    let str_val = json_value_to_string(&value);
                    type_trackers[i].observe(&str_val);
                    str_val
                })
                .collect();
            preview_rows.push(row);
        }
    }

    // Build schema
    let schema: Vec<ColumnSchema> = all_keys
        .iter()
        .enumerate()
        .map(|(i, name): (usize, &String)| {
            let tracker: TypeTracker = type_trackers.get(i).cloned().unwrap_or_default();
            ColumnSchema {
                name: name.clone(),
                inferred_type: tracker.inferred_type(),
                nullable: tracker.has_nulls,
                sample_values: tracker.samples,
            }
        })
        .collect();

    Ok(PreviewResult {
        file_path: path.to_path_buf(),
        file_type: FileType::Json,
        file_size: 0,
        schema: Some(schema),
        row_count: preview_rows.len(),
        preview_rows,
        headers: all_keys,
    })
}

/// Preview a single JSON object
fn preview_json_object(path: &Path, obj: &JsonValue) -> anyhow::Result<PreviewResult> {
    if let JsonValue::Object(map) = obj {
        let headers: Vec<String> = map.keys().cloned().collect();
        let row: Vec<String> = map.values().map(json_value_to_string).collect();

        let schema: Vec<ColumnSchema> = headers
            .iter()
            .zip(map.values())
            .map(|(name, value): (&String, &JsonValue)| {
                let str_val = json_value_to_string(value);
                ColumnSchema {
                    name: name.clone(),
                    inferred_type: json_type_to_string(value),
                    nullable: value.is_null(),
                    sample_values: vec![str_val],
                }
            })
            .collect();

        Ok(PreviewResult {
            file_path: path.to_path_buf(),
            file_type: FileType::Json,
            file_size: 0,
            schema: Some(schema),
            row_count: 1,
            preview_rows: vec![row],
            headers,
        })
    } else {
        Err(HelpfulError::json_parse_error(path, "Expected JSON object").into())
    }
}

/// Preview NDJSON/JSONL file
fn preview_ndjson(path: &Path, rows: usize) -> anyhow::Result<PreviewResult> {
    let file = File::open(path)
        .map_err(|e| HelpfulError::cannot_read_file(path, &e.to_string()))?;
    let reader = BufReader::new(file);

    let mut objects: Vec<JsonValue> = Vec::new();

    for (line_num, line_result) in reader.lines().take(rows).enumerate() {
        let line = line_result.map_err(|e| HelpfulError::cannot_read_file(path, &e.to_string()))?;

        if line.trim().is_empty() {
            continue;
        }

        let value: JsonValue = serde_json::from_str(&line)
            .map_err(|e| HelpfulError::json_parse_error(path, &format!("Line {}: {}", line_num + 1, e)))?;

        objects.push(value);
    }

    preview_json_array(path, &objects, rows)
}

/// Preview Parquet file
fn preview_parquet(path: &Path, rows: usize) -> anyhow::Result<PreviewResult> {
    let file = File::open(path)
        .map_err(|e| HelpfulError::cannot_read_file(path, &e.to_string()))?;

    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e: ParquetError| HelpfulError::parquet_error(path, &e.to_string()))?;

    let arrow_schema = builder.schema().clone();
    let mut reader = builder
        .with_batch_size(rows)
        .build()
        .map_err(|e: ParquetError| HelpfulError::parquet_error(path, &e.to_string()))?;

    // Get headers from schema
    let headers: Vec<String> = arrow_schema
        .fields()
        .iter()
        .map(|f: &std::sync::Arc<arrow::datatypes::Field>| f.name().clone())
        .collect();

    // Build schema info
    let schema: Vec<ColumnSchema> = arrow_schema
        .fields()
        .iter()
        .map(|field: &std::sync::Arc<arrow::datatypes::Field>| ColumnSchema {
            name: field.name().clone(),
            inferred_type: arrow_type_to_string(field.data_type()),
            nullable: field.is_nullable(),
            sample_values: vec![],
        })
        .collect();

    // Read rows
    let mut preview_rows: Vec<Vec<String>> = Vec::new();
    let mut total_rows = 0;

    while let Some(batch_result) = reader.next() {
        let batch = batch_result.map_err(|e: ArrowError| HelpfulError::parquet_error(path, &e.to_string()))?;

        for row_idx in 0..batch.num_rows() {
            if total_rows >= rows {
                break;
            }

            let row: Vec<String> = batch
                .columns()
                .iter()
                .map(|col: &ArrayRef| array_value_to_string(col, row_idx))
                .collect();

            preview_rows.push(row);
            total_rows += 1;
        }

        if total_rows >= rows {
            break;
        }
    }

    Ok(PreviewResult {
        file_path: path.to_path_buf(),
        file_type: FileType::Parquet,
        file_size: 0,
        schema: Some(schema),
        row_count: preview_rows.len(),
        preview_rows,
        headers,
    })
}

/// Preview text file
fn preview_text(path: &Path, rows: usize) -> anyhow::Result<PreviewResult> {
    let file = File::open(path)
        .map_err(|e| HelpfulError::cannot_read_file(path, &e.to_string()))?;
    let reader = BufReader::new(file);

    let lines: Vec<Vec<String>> = reader
        .lines()
        .take(rows)
        .filter_map(|l| l.ok())
        .map(|l| vec![l])
        .collect();

    Ok(PreviewResult {
        file_path: path.to_path_buf(),
        file_type: FileType::Text,
        file_size: 0,
        schema: None,
        row_count: lines.len(),
        preview_rows: lines,
        headers: vec!["Line".to_string()],
    })
}

/// Preview raw bytes as hex dump
fn preview_raw(path: &Path, bytes: usize) -> anyhow::Result<()> {
    let mut file = File::open(path)
        .map_err(|e| HelpfulError::cannot_read_file(path, &e.to_string()))?;

    let mut buffer = vec![0u8; bytes];
    let bytes_read = file.read(&mut buffer)?;
    buffer.truncate(bytes_read);

    println!("Raw preview of {} ({} bytes):", path.display(), bytes_read);
    println!();

    // Print hex dump
    for (offset, chunk) in buffer.chunks(16).enumerate() {
        let addr = offset * 16;
        print!("{:08x}  ", addr);

        // Hex values
        for (i, byte) in chunk.iter().enumerate() {
            if i == 8 {
                print!(" ");
            }
            print!("{:02x} ", byte);
        }

        // Padding for incomplete lines
        for i in chunk.len()..16 {
            if i == 8 {
                print!(" ");
            }
            print!("   ");
        }

        // ASCII representation
        print!(" |");
        for byte in chunk {
            if *byte >= 0x20 && *byte < 0x7f {
                print!("{}", *byte as char);
            } else {
                print!(".");
            }
        }
        println!("|");
    }

    Ok(())
}

/// Preview file head (first N lines)
fn preview_head(path: &Path, lines: usize) -> anyhow::Result<()> {
    let file = File::open(path)
        .map_err(|e| HelpfulError::cannot_read_file(path, &e.to_string()))?;
    let reader = BufReader::new(file);

    println!("First {} lines of {}:", lines, path.display());
    println!();

    for (idx, line_result) in reader.lines().take(lines).enumerate() {
        let line = line_result?;
        println!("{:>4} | {}", idx + 1, line);
    }

    Ok(())
}

// === Helper functions ===

fn json_value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Array(arr) => format!("[{} items]", arr.len()),
        JsonValue::Object(obj) => format!("{{{} keys}}", obj.len()),
    }
}

fn json_type_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(_) => "boolean".to_string(),
        JsonValue::Number(n) => {
            if n.is_i64() {
                "integer".to_string()
            } else {
                "float".to_string()
            }
        }
        JsonValue::String(_) => "string".to_string(),
        JsonValue::Array(_) => "array".to_string(),
        JsonValue::Object(_) => "object".to_string(),
    }
}

fn arrow_type_to_string(dt: &DataType) -> String {
    match dt {
        DataType::Boolean => "boolean".to_string(),
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => "integer".to_string(),
        DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => "unsigned".to_string(),
        DataType::Float16 | DataType::Float32 | DataType::Float64 => "float".to_string(),
        DataType::Utf8 | DataType::LargeUtf8 => "string".to_string(),
        DataType::Binary | DataType::LargeBinary => "binary".to_string(),
        DataType::Date32 | DataType::Date64 => "date".to_string(),
        DataType::Timestamp(_, _) => "timestamp".to_string(),
        DataType::Time32(_) | DataType::Time64(_) => "time".to_string(),
        DataType::Duration(_) => "duration".to_string(),
        DataType::List(_) | DataType::LargeList(_) => "list".to_string(),
        DataType::Struct(_) => "struct".to_string(),
        DataType::Map(_, _) => "map".to_string(),
        _ => format!("{:?}", dt),
    }
}

fn array_value_to_string(array: &ArrayRef, row: usize) -> String {
    use arrow::array::*;

    if array.is_null(row) {
        return "null".to_string();
    }

    match array.data_type() {
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            arr.value(row).to_string()
        }
        DataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            arr.value(row).to_string()
        }
        DataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            arr.value(row).to_string()
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            arr.value(row).to_string()
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            arr.value(row).to_string()
        }
        DataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            arr.value(row).to_string()
        }
        DataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            arr.value(row).to_string()
        }
        DataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            arr.value(row).to_string()
        }
        DataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            arr.value(row).to_string()
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            format!("{:.6}", arr.value(row))
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            format!("{:.6}", arr.value(row))
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            arr.value(row).to_string()
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            arr.value(row).to_string()
        }
        _ => format!("<{:?}>", array.data_type()),
    }
}

/// Type tracker for inferring column types from sample values
#[derive(Debug, Clone, Default)]
struct TypeTracker {
    has_nulls: bool,
    has_integers: bool,
    has_floats: bool,
    has_booleans: bool,
    has_strings: bool,
    samples: Vec<String>,
}

impl TypeTracker {
    fn new() -> Self {
        Self::default()
    }

    fn observe(&mut self, value: &str) {
        let trimmed = value.trim();

        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
            self.has_nulls = true;
            return;
        }

        // Keep up to 3 samples
        if self.samples.len() < 3 && !self.samples.contains(&trimmed.to_string()) {
            self.samples.push(trimmed.to_string());
        }

        // Check for boolean
        if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
            self.has_booleans = true;
            return;
        }

        // Check for integer
        if trimmed.parse::<i64>().is_ok() {
            self.has_integers = true;
            return;
        }

        // Check for float
        if trimmed.parse::<f64>().is_ok() {
            self.has_floats = true;
            return;
        }

        // Otherwise it's a string
        self.has_strings = true;
    }

    fn inferred_type(&self) -> String {
        if self.has_strings {
            return "string".to_string();
        }
        if self.has_booleans && !self.has_integers && !self.has_floats {
            return "boolean".to_string();
        }
        if self.has_floats {
            return "float".to_string();
        }
        if self.has_integers {
            return "integer".to_string();
        }
        "unknown".to_string()
    }
}

// === Output functions ===

fn output_json(result: &PreviewResult) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    println!("{}", json);
    Ok(())
}

fn output_schema(result: &PreviewResult) {
    println!("Schema for: {}", result.file_path.display());
    println!("File type:  {:?}", result.file_type);
    println!("File size:  {}", format_size(result.file_size));
    println!();

    if let Some(schema) = &result.schema {
        if schema.is_empty() {
            println!("(empty schema)");
            return;
        }

        let headers = &["Column", "Type", "Nullable", "Samples"];
        let rows: Vec<Vec<String>> = schema
            .iter()
            .map(|col| {
                vec![
                    col.name.clone(),
                    col.inferred_type.clone(),
                    if col.nullable { "yes" } else { "no" }.to_string(),
                    col.sample_values.join(", "),
                ]
            })
            .collect();

        print_table(headers, rows);
    } else {
        println!("(no schema available for this file type)");
    }
}

fn output_preview(result: &PreviewResult) {
    println!(
        "Preview: {} ({:?}, {})",
        result.file_path.display(),
        result.file_type,
        format_size(result.file_size)
    );
    println!("{} rows", result.row_count);
    println!();

    if result.preview_rows.is_empty() {
        println!("(no data)");
        return;
    }

    let headers: Vec<&str> = result.headers.iter().map(|s| s.as_str()).collect();
    print_table(&headers, result.preview_rows.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_preview_csv() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test.csv");
        let mut file = File::create(&csv_path).unwrap();
        writeln!(file, "id,name,value").unwrap();
        writeln!(file, "1,foo,100").unwrap();
        writeln!(file, "2,bar,200").unwrap();

        let result = preview_csv(&csv_path, 10, ',').unwrap();

        assert_eq!(result.headers, vec!["id", "name", "value"]);
        assert_eq!(result.row_count, 2);
        assert!(result.schema.is_some());

        let schema = result.schema.unwrap();
        assert_eq!(schema[0].inferred_type, "integer");
        assert_eq!(schema[1].inferred_type, "string");
        assert_eq!(schema[2].inferred_type, "integer");
    }

    #[test]
    fn test_preview_json_array() {
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("test.json");
        let mut file = File::create(&json_path).unwrap();
        writeln!(file, r#"[{{"id": 1, "name": "foo"}}, {{"id": 2, "name": "bar"}}]"#).unwrap();

        let result = preview_json(&json_path, 10).unwrap();

        assert_eq!(result.row_count, 2);
        assert!(result.headers.contains(&"id".to_string()));
        assert!(result.headers.contains(&"name".to_string()));
    }

    #[test]
    fn test_preview_ndjson() {
        let temp_dir = TempDir::new().unwrap();
        let ndjson_path = temp_dir.path().join("test.jsonl");
        let mut file = File::create(&ndjson_path).unwrap();
        writeln!(file, r#"{{"id": 1, "value": 100}}"#).unwrap();
        writeln!(file, r#"{{"id": 2, "value": 200}}"#).unwrap();

        let result = preview_ndjson(&ndjson_path, 10).unwrap();

        assert_eq!(result.row_count, 2);
    }

    #[test]
    fn test_type_tracker() {
        let mut tracker = TypeTracker::new();
        tracker.observe("100");
        tracker.observe("200");
        assert_eq!(tracker.inferred_type(), "integer");

        let mut tracker = TypeTracker::new();
        tracker.observe("1.5");
        tracker.observe("2.5");
        assert_eq!(tracker.inferred_type(), "float");

        let mut tracker = TypeTracker::new();
        tracker.observe("hello");
        tracker.observe("world");
        assert_eq!(tracker.inferred_type(), "string");

        let mut tracker = TypeTracker::new();
        tracker.observe("true");
        tracker.observe("false");
        assert_eq!(tracker.inferred_type(), "boolean");

        // Mixed types default to string
        let mut tracker = TypeTracker::new();
        tracker.observe("100");
        tracker.observe("hello");
        assert_eq!(tracker.inferred_type(), "string");
    }

    #[test]
    fn test_file_type_detection() {
        assert_eq!(FileType::from_extension("csv"), Some(FileType::Csv));
        assert_eq!(FileType::from_extension("JSON"), Some(FileType::Json));
        assert_eq!(FileType::from_extension("parquet"), Some(FileType::Parquet));
        assert_eq!(FileType::from_extension("txt"), Some(FileType::Text));
        assert_eq!(FileType::from_extension("xyz"), None);
    }
}
