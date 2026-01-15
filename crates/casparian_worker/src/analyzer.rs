//! Heuristic file format analyzer for the Shredder.
//!
//! Reads only 16KB of file head to detect format and shred strategy.
//! Designed to handle 90% of cases deterministically without LLM.

use casparian_protocol::{AnalysisResult, DetectionConfidence, ShredStrategy};
use lasso::{Rodeo, Spur};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use thiserror::Error;

/// Maximum bytes to read for analysis
const HEAD_SIZE: usize = 16384; // 16KB

/// Threshold for high cardinality warning
const HIGH_CARDINALITY_THRESHOLD: usize = 100;

#[derive(Error, Debug)]
pub enum AnalyzerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File too small for analysis: {0} bytes")]
    FileTooSmall(usize),
    #[error("UTF-8 decode error in file head")]
    Utf8Error,
}

/// Analyze a file's head to determine format and shred strategy.
///
/// Reads only 16KB to minimize I/O. Returns detection confidence
/// so user knows when to trust the result.
pub fn analyze_file_head(path: &Path) -> Result<AnalysisResult, AnalyzerError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut head = vec![0u8; HEAD_SIZE];
    let bytes_read = reader.read(&mut head)?;
    head.truncate(bytes_read);

    if bytes_read < 10 {
        return Err(AnalyzerError::FileTooSmall(bytes_read));
    }

    // Try heuristics in order of specificity
    if let Some(result) = try_json_detection(&head, bytes_read) {
        return Ok(result);
    }

    if let Some(result) = try_csv_detection(&head, bytes_read) {
        return Ok(result);
    }

    // Fallback: Unknown - needs user/LLM help
    Ok(AnalysisResult {
        strategy: ShredStrategy::Passthrough,
        confidence: DetectionConfidence::Unknown,
        sample_keys: vec![],
        estimated_shard_count: 1,
        head_bytes: bytes_read,
        reasoning: "Could not detect file format. Please specify strategy manually.".into(),
        warning: None,
    })
}

/// Result of full file analysis
#[derive(Debug, Clone)]
pub struct FullAnalysisResult {
    /// All unique keys found in the file
    pub all_keys: HashMap<String, u64>,
    /// Total rows scanned
    pub total_rows: u64,
    /// Bytes scanned
    pub bytes_scanned: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Analyze entire file to get complete key distribution.
///
/// Unlike analyze_file_head which only reads 16KB, this scans the whole file
/// to find ALL unique shard keys. Use this for accurate counts when you need
/// to know exactly how many shards will be created.
///
/// This is slower but gives complete information - no surprises at runtime.
///
/// F-004-v2: Uses lasso string interning to avoid allocating duplicate keys.
/// For files with many rows but few unique keys, this significantly reduces
/// allocations from O(rows) to O(unique_keys).
pub fn analyze_file_full(
    path: &Path,
    strategy: &ShredStrategy,
) -> Result<FullAnalysisResult, AnalyzerError> {
    let start = std::time::Instant::now();

    let file = File::open(path)?;
    let file_size = file.metadata()?.len();
    let reader = BufReader::with_capacity(65536, file);

    // F-004-v2: Use string interning to avoid per-row allocations
    // Rodeo interns strings - duplicate keys reuse the same allocation
    let mut interner = Rodeo::default();
    let mut key_counts: HashMap<Spur, u64> = HashMap::new();
    let mut total_rows = 0u64;

    // Determine extraction method based on strategy
    match strategy {
        ShredStrategy::CsvColumn { delimiter, col_index, has_header } => {
            let skip_header = *has_header;
            let delim = *delimiter as char;

            for (i, line_result) in reader.lines().enumerate() {
                let line = line_result?;

                // Skip header if present
                if i == 0 && skip_header {
                    continue;
                }

                total_rows += 1;

                // Extract key from column - F-004-v2: intern instead of allocate
                if let Some(key_str) = extract_csv_key(&line, delim, *col_index) {
                    let key = interner.get_or_intern(key_str);
                    *key_counts.entry(key).or_insert(0) += 1;
                }
            }
        }
        ShredStrategy::JsonKey { key_path } => {
            for line_result in reader.lines() {
                let line = line_result?;
                total_rows += 1;

                // F-004-v2: intern JSON keys too
                if let Some(key_str) = extract_json_key(&line, key_path) {
                    let key = interner.get_or_intern(&key_str);
                    *key_counts.entry(key).or_insert(0) += 1;
                }
            }
        }
        ShredStrategy::Passthrough => {
            // No shredding - just count lines
            for _ in reader.lines() {
                total_rows += 1;
            }
            let key = interner.get_or_intern("_ALL");
            key_counts.insert(key, total_rows);
        }
        ShredStrategy::Regex { .. } => {
            // Regex not supported for full analysis yet
            return Err(AnalyzerError::Utf8Error);
        }
    }

    // F-004-v2: Convert interned keys back to owned strings for the result
    let all_keys: HashMap<String, u64> = key_counts
        .into_iter()
        .map(|(spur, count)| (interner.resolve(&spur).to_string(), count))
        .collect();

    Ok(FullAnalysisResult {
        all_keys,
        total_rows,
        bytes_scanned: file_size,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// F-004-v2: Extract key from CSV line (returns borrowed &str to avoid allocation)
fn extract_csv_key(line: &str, delim: char, col_index: usize) -> Option<&str> {
    line.split(delim).nth(col_index).map(|s| s.trim())
}

/// Extract key from JSON line
fn extract_json_key(line: &str, key_path: &str) -> Option<String> {
    let trimmed = line.trim();
    if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(trimmed) {
        // Simple single-level key extraction (key_path like "event_type")
        if let Some(serde_json::Value::String(s)) = obj.get(key_path) {
            return Some(s.clone());
        }
    }
    None
}

/// Detect JSON Lines format (newline-delimited JSON)
fn try_json_detection(head: &[u8], head_bytes: usize) -> Option<AnalysisResult> {
    let text = std::str::from_utf8(head).ok()?;
    let lines: Vec<&str> = text.lines().take(50).collect();

    if lines.len() < 2 {
        return None;
    }

    // Check if all lines parse as JSON objects
    let mut valid_json_count = 0;
    let mut key_values: HashMap<String, HashSet<String>> = HashMap::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(trimmed) {
            valid_json_count += 1;

            // Collect unique values for each string key
            for (key, value) in obj.iter() {
                if let serde_json::Value::String(s) = value {
                    key_values
                        .entry(key.clone())
                        .or_default()
                        .insert(s.clone());
                }
            }
        }
    }

    // Need at least 80% valid JSON
    let total_lines = lines.iter().filter(|l| !l.trim().is_empty()).count();
    if total_lines == 0 || (valid_json_count as f64 / total_lines as f64) < 0.8 {
        return None;
    }

    // Find key with highest cardinality (likely message type)
    let (best_key, unique_values) = key_values
        .iter()
        .max_by_key(|(_, values)| values.len())?;

    let sample_keys: Vec<String> = unique_values.iter().take(10).cloned().collect();
    let estimated_shard_count = unique_values.len();

    let warning = if estimated_shard_count > HIGH_CARDINALITY_THRESHOLD {
        Some(format!(
            "High cardinality detected: {} unique values in '{}'. Consider grouping.",
            estimated_shard_count, best_key
        ))
    } else {
        None
    };

    Some(AnalysisResult {
        strategy: ShredStrategy::JsonKey {
            key_path: best_key.clone(),
        },
        confidence: DetectionConfidence::High,
        sample_keys,
        estimated_shard_count,
        head_bytes,
        reasoning: format!(
            "Detected JSON Lines format. Key '{}' has {} unique values, suitable for sharding.",
            best_key, estimated_shard_count
        ),
        warning,
    })
}

/// Detect CSV format with delimiter inference
fn try_csv_detection(head: &[u8], head_bytes: usize) -> Option<AnalysisResult> {
    let text = std::str::from_utf8(head).ok()?;
    // Sample up to 200 lines to better detect high cardinality (>100 unique values)
    let lines: Vec<&str> = text.lines().take(200).collect();

    if lines.len() < 2 {
        return None;
    }

    // Try common delimiters
    for delim in [',', '\t', '|', ';'] {
        if let Some(result) = try_csv_with_delimiter(&lines, delim, head_bytes) {
            return Some(result);
        }
    }

    None
}

/// Try to detect CSV with a specific delimiter
fn try_csv_with_delimiter(lines: &[&str], delim: char, head_bytes: usize) -> Option<AnalysisResult> {
    // Count fields per line (trim CRLF/whitespace first)
    let counts: Vec<usize> = lines
        .iter()
        .map(|l| count_fields(l.trim_end_matches(['\r', '\n', ' ']), delim))
        .collect();

    // Need at least 2 fields
    if counts.is_empty() || counts.iter().all(|&c| c < 2) {
        return None;
    }

    // For multiplexed files, different message types may have different field counts.
    // Instead of requiring exact consistency, check:
    // 1. Most lines have many fields (>= min_fields threshold)
    // 2. The delimiter appears consistently
    let min_fields = 3;
    let lines_with_many_fields = counts.iter().filter(|&&c| c >= min_fields).count();

    // At least 80% of lines should have >= min_fields with this delimiter
    if (lines_with_many_fields as f64 / counts.len() as f64) < 0.80 {
        return None;
    }

    // Find the most common field count (mode) for reporting
    let mut count_freq: HashMap<usize, usize> = HashMap::new();
    for &c in &counts {
        *count_freq.entry(c).or_insert(0) += 1;
    }
    let expected = count_freq
        .iter()
        .max_by_key(|(_, freq)| *freq)
        .map(|(count, _)| *count)
        .unwrap_or(counts[0]);

    // Determine if first line is a header
    let has_header = looks_like_header(lines[0], delim);

    // Find column with most unique values (likely shard key)
    let data_lines: Vec<&str> = if has_header {
        lines.iter().skip(1).copied().collect()
    } else {
        lines.to_vec()
    };

    let (best_col, unique_values) = find_best_shard_column(&data_lines, delim)?;

    let sample_keys: Vec<String> = unique_values.iter().take(10).cloned().collect();
    let estimated_shard_count = unique_values.len();

    // Get column name if header exists
    let col_name = if has_header {
        get_column_name(lines[0], delim, best_col)
    } else {
        format!("column_{}", best_col)
    };

    let warning = if estimated_shard_count > HIGH_CARDINALITY_THRESHOLD {
        Some(format!(
            "High cardinality detected: {} unique values in '{}'. Consider grouping by prefix.",
            estimated_shard_count, col_name
        ))
    } else {
        None
    };

    Some(AnalysisResult {
        strategy: ShredStrategy::CsvColumn {
            delimiter: delim as u8,
            col_index: best_col,
            has_header,
        },
        confidence: DetectionConfidence::High,
        sample_keys,
        estimated_shard_count,
        head_bytes,
        reasoning: format!(
            "Detected {} CSV with {} columns. Column '{}' (index {}) has {} unique values, suitable for sharding.",
            if has_header { "headered" } else { "headerless" },
            expected,
            col_name,
            best_col,
            estimated_shard_count
        ),
        warning,
    })
}

/// Count fields in a CSV line (simple, doesn't handle quoted fields)
fn count_fields(line: &str, delim: char) -> usize {
    if line.is_empty() {
        return 0;
    }
    line.split(delim).count()
}

/// Check if first line looks like a header (non-numeric, unique values)
fn looks_like_header(line: &str, delim: char) -> bool {
    let fields: Vec<&str> = line.split(delim).collect();

    // Headers are usually non-numeric
    let non_numeric_count = fields
        .iter()
        .filter(|f| f.trim().parse::<f64>().is_err())
        .count();

    // At least 70% should be non-numeric for a header
    (non_numeric_count as f64 / fields.len() as f64) > 0.7
}

/// Find the column with highest cardinality (best shard key candidate)
fn find_best_shard_column(lines: &[&str], delim: char) -> Option<(usize, HashSet<String>)> {
    if lines.is_empty() {
        return None;
    }

    let num_cols = count_fields(lines[0], delim);
    let mut col_values: Vec<HashSet<String>> = vec![HashSet::new(); num_cols];

    for line in lines {
        let fields: Vec<&str> = line.split(delim).collect();
        for (i, field) in fields.iter().enumerate() {
            if i < num_cols {
                col_values[i].insert(field.trim().to_string());
            }
        }
    }

    // Find column with highest cardinality (but not too high - avoid IDs)
    let best = col_values
        .iter()
        .enumerate()
        .filter(|(_, values)| {
            // Filter out columns that are likely unique IDs (cardinality == row count)
            values.len() < lines.len() && values.len() > 1
        })
        .max_by_key(|(_, values)| values.len());

    best.map(|(idx, values)| (idx, values.clone()))
}

/// Get column name from header line
fn get_column_name(header: &str, delim: char, col_index: usize) -> String {
    header
        .split(delim)
        .nth(col_index)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| format!("column_{}", col_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_csv_detection_with_header() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "timestamp,message_type,value,status").unwrap();
        for i in 0..100 {
            let msg_type = match i % 3 {
                0 => "MCData__PFC",
                1 => "MCData__FAULT",
                _ => "MCData__STATUS",
            };
            writeln!(file, "2024-01-01 00:00:{:02},{},{:.2},OK", i, msg_type, i as f64 * 1.5).unwrap();
        }
        file.flush().unwrap();

        let result = analyze_file_head(file.path()).unwrap();

        assert!(matches!(result.confidence, DetectionConfidence::High));
        assert!(matches!(
            result.strategy,
            ShredStrategy::CsvColumn {
                delimiter: b',',
                has_header: true,
                ..
            }
        ));
        assert_eq!(result.estimated_shard_count, 3);
        assert!(result.reasoning.contains("message_type"));
    }

    #[test]
    fn test_csv_detection_without_header() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 0..100 {
            let msg_type = match i % 2 {
                0 => "TYPE_A",
                _ => "TYPE_B",
            };
            writeln!(file, "2024-01-01,{},{}", msg_type, i).unwrap();
        }
        file.flush().unwrap();

        let result = analyze_file_head(file.path()).unwrap();

        assert!(matches!(result.confidence, DetectionConfidence::High));
        if let ShredStrategy::CsvColumn { has_header, .. } = result.strategy {
            assert!(!has_header);
        } else {
            panic!("Expected CsvColumn strategy");
        }
    }

    #[test]
    fn test_json_lines_detection() {
        let mut file = NamedTempFile::new().unwrap();
        for i in 0..50 {
            let event_type = match i % 3 {
                0 => "login",
                1 => "logout",
                _ => "action",
            };
            writeln!(
                file,
                r#"{{"timestamp":"2024-01-01","event_type":"{}","user_id":{}}}"#,
                event_type, i
            )
            .unwrap();
        }
        file.flush().unwrap();

        let result = analyze_file_head(file.path()).unwrap();

        assert!(matches!(result.confidence, DetectionConfidence::High));
        assert!(matches!(result.strategy, ShredStrategy::JsonKey { .. }));
    }

    #[test]
    fn test_high_cardinality_warning() {
        let mut file = NamedTempFile::new().unwrap();
        // Create file with >100 unique types in first 100 lines
        // ID column varies, but type column has high cardinality
        // We use 110 types to trigger warning (>100 threshold)
        // But make sure we have more rows than types so it's not filtered as unique ID
        writeln!(file, "id,timestamp,type,value").unwrap();
        for i in 0..150 {
            // 110 unique types, each appearing ~1.4 times in first 150 rows
            writeln!(file, "{},2024-01-01,TYPE_{},{}.5", i, i % 110, i).unwrap();
        }
        file.flush().unwrap();

        let result = analyze_file_head(file.path()).unwrap();

        // Should detect as CSV with high cardinality warning
        assert!(
            matches!(result.confidence, DetectionConfidence::High),
            "Expected High confidence but got: {:?}", result
        );
        assert!(result.warning.is_some(), "Expected high cardinality warning but got: {:?}", result);
        assert!(result.warning.as_ref().unwrap().contains("High cardinality"));
    }

    #[test]
    fn test_unknown_format() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "This is just random text").unwrap();
        writeln!(file, "with no consistent structure").unwrap();
        writeln!(file, "at all whatsoever").unwrap();
        file.flush().unwrap();

        let result = analyze_file_head(file.path()).unwrap();

        assert!(matches!(result.confidence, DetectionConfidence::Unknown));
        assert!(matches!(result.strategy, ShredStrategy::Passthrough));
    }

    #[test]
    fn test_tab_delimited() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "timestamp\tmsg_type\tvalue").unwrap();
        for i in 0..50 {
            let msg_type = if i % 2 == 0 { "A" } else { "B" };
            writeln!(file, "2024-01-01\t{}\t{}", msg_type, i).unwrap();
        }
        file.flush().unwrap();

        let result = analyze_file_head(file.path()).unwrap();

        assert!(matches!(result.confidence, DetectionConfidence::High));
        if let ShredStrategy::CsvColumn { delimiter, .. } = result.strategy {
            assert_eq!(delimiter, b'\t');
        } else {
            panic!("Expected CsvColumn strategy with tab delimiter");
        }
    }
}
