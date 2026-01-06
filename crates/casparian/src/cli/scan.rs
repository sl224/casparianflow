//! Scan command - Discover files in a directory
//!
//! This is a standalone command that works without a database.
//! It scans a directory for files matching specified criteria.

use crate::cli::error::HelpfulError;
use crate::cli::output::{color_for_extension, format_size, format_time, parse_size, print_table_colored};
use comfy_table::Color;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use walkdir::WalkDir;

/// Arguments for the scan command
#[derive(Debug)]
pub struct ScanArgs {
    pub path: PathBuf,
    pub types: Vec<String>,
    pub recursive: bool,
    pub depth: Option<usize>,
    pub min_size: Option<String>,
    pub max_size: Option<String>,
    pub json: bool,
    pub stats: bool,
    pub quiet: bool,
}

/// Discovered file information
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub name: String,
    pub extension: String,
    pub size: u64,
    #[serde(with = "system_time_serde")]
    pub modified: SystemTime,
}

/// Summary statistics
#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub total_files: usize,
    pub total_size: u64,
    pub files_by_type: HashMap<String, usize>,
    pub size_by_type: HashMap<String, u64>,
    pub directories_scanned: usize,
}

/// Complete scan result
#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub files: Vec<DiscoveredFile>,
    pub summary: ScanSummary,
    pub scan_path: PathBuf,
}

// Custom serialization for SystemTime
mod system_time_serde {
    use serde::{Serialize, Serializer};
    use std::time::SystemTime;

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(serde::ser::Error::custom)?;
        duration.as_secs().serialize(serializer)
    }
}

/// Execute the scan command
pub fn run(args: ScanArgs) -> anyhow::Result<()> {
    // Validate path exists
    if !args.path.exists() {
        return Err(HelpfulError::path_not_found(&args.path).into());
    }

    // Validate path is a directory
    if !args.path.is_dir() {
        return Err(HelpfulError::not_a_directory(&args.path).into());
    }

    // Parse size filters
    let min_size = args
        .min_size
        .as_ref()
        .map(|s| parse_size(s))
        .transpose()
        .map_err(|e| HelpfulError::invalid_size_format(&e))?;

    let max_size = args
        .max_size
        .as_ref()
        .map(|s| parse_size(s))
        .transpose()
        .map_err(|e| HelpfulError::invalid_size_format(&e))?;

    // Normalize type filters to lowercase
    let type_filters: Vec<String> = args.types.iter().map(|t| t.to_lowercase()).collect();

    // Build walker
    let mut walker = WalkDir::new(&args.path);

    if !args.recursive {
        walker = walker.max_depth(1);
    } else if let Some(depth) = args.depth {
        walker = walker.max_depth(depth);
    }

    // Collect files
    let mut files: Vec<DiscoveredFile> = Vec::new();
    let mut directories_scanned = 0;

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        if path.is_dir() {
            directories_scanned += 1;
            continue;
        }

        // Get file metadata
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue, // Skip files we can't read
        };

        let size = metadata.len();

        // Apply size filters
        if let Some(min) = min_size {
            if size < min {
                continue;
            }
        }
        if let Some(max) = max_size {
            if size > max {
                continue;
            }
        }

        // Get extension
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Apply type filter
        if !type_filters.is_empty() && !type_filters.contains(&extension) {
            continue;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        files.push(DiscoveredFile {
            path: path.to_path_buf(),
            name,
            extension,
            size,
            modified,
        });
    }

    // Sort by path for consistent output
    files.sort_by(|a, b| a.path.cmp(&b.path));

    // Build summary
    let summary = build_summary(&files, directories_scanned);

    let result = ScanResult {
        files,
        summary,
        scan_path: args.path.clone(),
    };

    // Output based on format
    if args.json {
        output_json(&result)?;
    } else if args.stats {
        output_stats(&result);
    } else if args.quiet {
        output_quiet(&result);
    } else {
        output_table(&result);
    }

    Ok(())
}

/// Build summary statistics from discovered files
fn build_summary(files: &[DiscoveredFile], directories_scanned: usize) -> ScanSummary {
    let mut files_by_type: HashMap<String, usize> = HashMap::new();
    let mut size_by_type: HashMap<String, u64> = HashMap::new();
    let mut total_size: u64 = 0;

    for file in files {
        total_size += file.size;

        let ext = if file.extension.is_empty() {
            "(no ext)".to_string()
        } else {
            file.extension.clone()
        };

        *files_by_type.entry(ext.clone()).or_insert(0) += 1;
        *size_by_type.entry(ext).or_insert(0) += file.size;
    }

    ScanSummary {
        total_files: files.len(),
        total_size,
        files_by_type,
        size_by_type,
        directories_scanned,
    }
}

/// Output as JSON
fn output_json(result: &ScanResult) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    println!("{}", json);
    Ok(())
}

/// Output as statistics summary
fn output_stats(result: &ScanResult) {
    let summary = &result.summary;

    println!("Scan: {}", result.scan_path.display());
    println!();
    println!("Files:       {}", summary.total_files);
    println!("Total Size:  {}", format_size(summary.total_size));
    println!("Directories: {}", summary.directories_scanned);
    println!();

    if !summary.files_by_type.is_empty() {
        println!("By Type:");

        // Sort by count descending
        let mut types: Vec<_> = summary.files_by_type.iter().collect();
        types.sort_by(|a, b| b.1.cmp(a.1));

        for (ext, count) in types {
            let size = summary.size_by_type.get(ext).copied().unwrap_or(0);
            println!("  {:<12} {:>6} files  {:>10}", ext, count, format_size(size));
        }
    }
}

/// Output just file paths (quiet mode)
fn output_quiet(result: &ScanResult) {
    for file in &result.files {
        println!("{}", file.path.display());
    }
}

/// Output as formatted table
fn output_table(result: &ScanResult) {
    if result.files.is_empty() {
        println!("No files found in: {}", result.scan_path.display());
        return;
    }

    println!(
        "Found {} files in {} ({} total)",
        result.summary.total_files,
        result.scan_path.display(),
        format_size(result.summary.total_size)
    );
    println!();

    let headers = &["Name", "Type", "Size", "Modified", "Path"];

    let rows: Vec<Vec<(String, Option<Color>)>> = result
        .files
        .iter()
        .map(|file| {
            let ext_color = color_for_extension(&file.extension);
            let ext_display = if file.extension.is_empty() {
                "-".to_string()
            } else {
                file.extension.clone()
            };

            // Get relative path from scan root
            let display_path = file
                .path
                .strip_prefix(&result.scan_path)
                .unwrap_or(&file.path)
                .display()
                .to_string();

            vec![
                (file.name.clone(), None),
                (ext_display, Some(ext_color)),
                (format_size(file.size), None),
                (format_time(file.modified), None),
                (display_path, Some(Color::Grey)),
            ]
        })
        .collect();

    print_table_colored(headers, rows);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_files(dir: &Path) {
        // Create test files
        File::create(dir.join("test.csv"))
            .unwrap()
            .write_all(b"id,name\n1,foo")
            .unwrap();
        File::create(dir.join("data.json"))
            .unwrap()
            .write_all(b"{}")
            .unwrap();
        File::create(dir.join("readme.txt"))
            .unwrap()
            .write_all(b"Hello")
            .unwrap();

        // Create nested directory
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        File::create(nested.join("deep.csv"))
            .unwrap()
            .write_all(b"a,b\n1,2")
            .unwrap();
    }

    #[test]
    fn test_scan_basic() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(temp_dir.path());

        let args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            types: vec![],
            recursive: true,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
        };

        run(args).unwrap();
    }

    #[test]
    fn test_scan_type_filter() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(temp_dir.path());

        let args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            types: vec!["csv".to_string()],
            recursive: true,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
        };

        run(args).unwrap();
    }

    #[test]
    fn test_scan_non_recursive() {
        let temp_dir = TempDir::new().unwrap();
        create_test_files(temp_dir.path());

        let args = ScanArgs {
            path: temp_dir.path().to_path_buf(),
            types: vec![],
            recursive: false,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
        };

        run(args).unwrap();
    }

    #[test]
    fn test_scan_nonexistent_path() {
        let args = ScanArgs {
            path: PathBuf::from("/nonexistent/path/that/does/not/exist"),
            types: vec![],
            recursive: false,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
        };

        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_file_instead_of_dir() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        File::create(&file_path)
            .unwrap()
            .write_all(b"test")
            .unwrap();

        let args = ScanArgs {
            path: file_path,
            types: vec![],
            recursive: false,
            depth: None,
            min_size: None,
            max_size: None,
            json: false,
            stats: false,
            quiet: true,
        };

        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_summary() {
        let files = vec![
            DiscoveredFile {
                path: PathBuf::from("test.csv"),
                name: "test.csv".to_string(),
                extension: "csv".to_string(),
                size: 100,
                modified: SystemTime::now(),
            },
            DiscoveredFile {
                path: PathBuf::from("data.csv"),
                name: "data.csv".to_string(),
                extension: "csv".to_string(),
                size: 200,
                modified: SystemTime::now(),
            },
            DiscoveredFile {
                path: PathBuf::from("info.json"),
                name: "info.json".to_string(),
                extension: "json".to_string(),
                size: 50,
                modified: SystemTime::now(),
            },
        ];

        let summary = build_summary(&files, 5);

        assert_eq!(summary.total_files, 3);
        assert_eq!(summary.total_size, 350);
        assert_eq!(summary.files_by_type.get("csv"), Some(&2));
        assert_eq!(summary.files_by_type.get("json"), Some(&1));
        assert_eq!(summary.size_by_type.get("csv"), Some(&300));
        assert_eq!(summary.directories_scanned, 5);
    }
}
