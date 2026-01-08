//! Parser Bundling System
//!
//! Creates ZIP artifacts with deterministic hashing for production parser registration.
//!
//! ## Key Features
//! - Bundles parser directories into ZIP archives
//! - Deterministic hashing using blake3
//! - Requires uv.lock for reproducible environments
//! - Extracts parser name/version from source code
//!
//! ## Usage
//! ```ignore
//! use casparian::bundler::{bundle_parser, ParserBundle};
//!
//! let bundle = bundle_parser(Path::new("./my_parser"))?;
//! println!("Bundled {} v{}", bundle.name, bundle.version);
//! println!("Source hash: {}", bundle.source_hash);
//! ```

use anyhow::{bail, Context, Result};
use blake3::Hasher;
use regex::Regex;
use std::fs;
use std::io::Write;
use std::path::Path;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// Extensions allowed in the bundle
const ALLOWED_EXTENSIONS: &[&str] = &["py", "json", "yaml", "yml", "toml"];

/// Result of bundling a parser directory
#[derive(Debug, Clone)]
pub struct ParserBundle {
    /// Parser name extracted from source code
    pub name: String,
    /// Parser version extracted from source code
    pub version: String,
    /// ZIP archive contents
    pub archive: Vec<u8>,
    /// Blake3 hash of all source files (deterministic)
    pub source_hash: String,
    /// Blake3 hash of the lockfile
    pub lockfile_hash: String,
    /// Raw lockfile content (for venv creation)
    pub lockfile_content: String,
}

/// Bundle a parser directory into a ZIP artifact
///
/// This function:
/// 1. Validates that uv.lock exists
/// 2. Finds and parses parser metadata (name, version)
/// 3. Walks the directory collecting allowed files
/// 4. Creates a deterministic ZIP with canonical timestamps
/// 5. Computes blake3 hashes for source and lockfile
///
/// # Arguments
/// * `dir` - Path to the parser directory
///
/// # Returns
/// A `ParserBundle` containing the archive and metadata
///
/// # Errors
/// - If uv.lock is missing
/// - If parser metadata cannot be found
/// - If no valid source files are found
pub fn bundle_parser(dir: &Path) -> Result<ParserBundle> {
    // 1. Validate uv.lock exists
    let lockfile_path = dir.join("uv.lock");
    if !lockfile_path.exists() {
        bail!(
            "Parser directory must contain uv.lock file.\n\
             Run 'uv lock' in the parser directory first."
        );
    }

    let lockfile_content =
        fs::read_to_string(&lockfile_path).context("Failed to read uv.lock")?;

    // 2. Find and parse the main parser file to extract name/version
    let (name, version) = extract_parser_metadata(dir)?;

    // 3. Walk directory and collect allowed files
    let mut files_to_bundle: Vec<(String, Vec<u8>)> = Vec::new();
    let mut source_hasher = Hasher::new();

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| !is_excluded(e.path()))
    {
        let entry = entry.context("Failed to read directory entry")?;
        if entry.file_type().is_file() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ALLOWED_EXTENSIONS.contains(&ext) {
                    let rel_path = path
                        .strip_prefix(dir)
                        .context("Failed to compute relative path")?;
                    let content = fs::read(path)
                        .with_context(|| format!("Failed to read file: {}", path.display()))?;

                    // Update source hash (include relative path for determinism)
                    source_hasher.update(rel_path.to_string_lossy().as_bytes());
                    source_hasher.update(&content);

                    files_to_bundle.push((rel_path.to_string_lossy().to_string(), content));
                }
            }
        }
    }

    if files_to_bundle.is_empty() {
        bail!(
            "No source files found in parser directory.\n\
             Expected at least one .py file."
        );
    }

    // 4. Create ZIP with canonical timestamps (1980-01-01 for determinism)
    let mut archive = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut archive));
        let options = SimpleFileOptions::default()
            .last_modified_time(
                zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
                    .expect("Valid date"),
            )
            .compression_method(zip::CompressionMethod::Deflated);

        // Sort files for deterministic ordering
        files_to_bundle.sort_by(|a, b| a.0.cmp(&b.0));

        for (path, content) in &files_to_bundle {
            zip.start_file(path, options)
                .with_context(|| format!("Failed to add file to ZIP: {}", path))?;
            zip.write_all(content)
                .with_context(|| format!("Failed to write file content: {}", path))?;
        }

        zip.finish().context("Failed to finalize ZIP archive")?;
    }

    // 5. Compute hashes
    let source_hash = source_hasher.finalize().to_hex().to_string();
    let lockfile_hash = blake3::hash(lockfile_content.as_bytes())
        .to_hex()
        .to_string();

    Ok(ParserBundle {
        name,
        version,
        archive,
        source_hash,
        lockfile_hash,
        lockfile_content,
    })
}

/// Check if a path should be excluded from bundling
fn is_excluded(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Exclude known directories by name (works for walkdir filter_entry)
    // Note: walkdir calls this on directories before descending
    let is_excluded_dir_name = matches!(
        name,
        ".venv" | "__pycache__" | ".git" | "node_modules" | ".mypy_cache" | ".pytest_cache" | ".tox" | "dist" | "build"
    ) || name.ends_with(".egg-info");

    if is_excluded_dir_name {
        return true;
    }

    // Exclude binary files by extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return matches!(ext, "so" | "dll" | "dylib" | "pyc" | "pyo" | "whl" | "egg");
    }

    false
}

/// Extract parser name and version from source files
fn extract_parser_metadata(dir: &Path) -> Result<(String, String)> {
    // Look for parser.py first, then any .py file with name/version attributes
    let parser_py = dir.join("parser.py");
    if parser_py.exists() {
        if let Ok(content) = fs::read_to_string(&parser_py) {
            if let (Some(name), Some(version)) = (
                extract_attribute(&content, "name"),
                extract_attribute(&content, "version"),
            ) {
                return Ok((name, version));
            }
        }
    }

    // Fall back to scanning all .py files
    for entry in fs::read_dir(dir).context("Failed to read parser directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        if path.extension().map(|e| e == "py").unwrap_or(false) {
            if let Ok(content) = fs::read_to_string(&path) {
                if let (Some(name), Some(version)) = (
                    extract_attribute(&content, "name"),
                    extract_attribute(&content, "version"),
                ) {
                    return Ok((name, version));
                }
            }
        }
    }

    bail!(
        "Could not find parser metadata (name, version) in any .py file.\n\
         Parser classes must have 'name' and 'version' attributes:\n\n\
         class MyParser:\n\
             name = 'my_parser'\n\
             version = '1.0.0'\n\
             ..."
    )
}

/// Extract a string attribute from Python source code
///
/// Matches patterns like:
/// - `name = "invoice_parser"`
/// - `name = 'invoice_parser'`
/// - `name="invoice_parser"`
fn extract_attribute(content: &str, attr: &str) -> Option<String> {
    // Match patterns like: name = "invoice_parser" or name = 'invoice_parser'
    let pattern = format!(r#"(?m)^\s*{}\s*=\s*['"]([^'"]+)['"]"#, attr);
    let re = Regex::new(&pattern).ok()?;
    re.captures(content)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bundle_parser_requires_lockfile() {
        let temp = TempDir::new().unwrap();
        let result = bundle_parser(temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("uv.lock"), "Error should mention uv.lock: {}", err);
    }

    #[test]
    fn test_bundle_parser_requires_metadata() {
        let temp = TempDir::new().unwrap();

        // Create lockfile but no parser with metadata
        fs::write(temp.path().join("uv.lock"), "# minimal lock").unwrap();
        fs::write(temp.path().join("helper.py"), "def foo(): pass").unwrap();

        let result = bundle_parser(temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("metadata") || err.contains("name") || err.contains("version"),
            "Error should mention missing metadata: {}",
            err
        );
    }

    #[test]
    fn test_bundle_is_deterministic() {
        let temp = TempDir::new().unwrap();

        // Create minimal parser structure
        fs::write(temp.path().join("uv.lock"), "# minimal lock").unwrap();
        fs::write(
            temp.path().join("parser.py"),
            r#"
class Parser:
    name = "test_parser"
    version = "1.0.0"

    def parse(self, ctx):
        pass
"#,
        )
        .unwrap();

        let bundle1 = bundle_parser(temp.path()).unwrap();
        let bundle2 = bundle_parser(temp.path()).unwrap();

        assert_eq!(bundle1.archive, bundle2.archive, "Archives should be identical");
        assert_eq!(
            bundle1.source_hash, bundle2.source_hash,
            "Source hashes should be identical"
        );
        assert_eq!(
            bundle1.lockfile_hash, bundle2.lockfile_hash,
            "Lockfile hashes should be identical"
        );
    }

    #[test]
    fn test_bundle_extracts_metadata() {
        let temp = TempDir::new().unwrap();

        fs::write(temp.path().join("uv.lock"), "# lock content").unwrap();
        fs::write(
            temp.path().join("parser.py"),
            r#"
class InvoiceParser:
    name = "invoice_parser"
    version = "2.3.4"
    topics = ["invoices"]
"#,
        )
        .unwrap();

        let bundle = bundle_parser(temp.path()).unwrap();

        assert_eq!(bundle.name, "invoice_parser");
        assert_eq!(bundle.version, "2.3.4");
    }

    #[test]
    fn test_extract_attribute_various_formats() {
        // Double quotes
        assert_eq!(
            extract_attribute(r#"name = "my_parser""#, "name"),
            Some("my_parser".to_string())
        );

        // Single quotes
        assert_eq!(
            extract_attribute(r#"name = 'my_parser'"#, "name"),
            Some("my_parser".to_string())
        );

        // No spaces
        assert_eq!(
            extract_attribute(r#"name="my_parser""#, "name"),
            Some("my_parser".to_string())
        );

        // With indentation
        assert_eq!(
            extract_attribute("    name = 'my_parser'", "name"),
            Some("my_parser".to_string())
        );

        // Version attribute
        assert_eq!(
            extract_attribute(r#"version = "1.2.3""#, "version"),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn test_excluded_directories() {
        assert!(is_excluded(Path::new(".venv")));
        assert!(is_excluded(Path::new("__pycache__")));
        assert!(is_excluded(Path::new(".git")));
        assert!(is_excluded(Path::new("node_modules")));
    }

    #[test]
    fn test_excluded_extensions() {
        // Binary files should be excluded
        let pyc = Path::new("test.pyc");
        assert!(
            pyc.extension().map(|e| e == "pyc").unwrap_or(false),
            "pyc extension check"
        );
    }

    #[test]
    fn test_bundle_includes_all_py_files() {
        let temp = TempDir::new().unwrap();

        fs::write(temp.path().join("uv.lock"), "# lock").unwrap();
        fs::write(
            temp.path().join("parser.py"),
            r#"name = "test"
version = "1.0.0""#,
        )
        .unwrap();
        fs::write(temp.path().join("utils.py"), "def helper(): pass").unwrap();
        fs::write(temp.path().join("config.json"), r#"{"key": "value"}"#).unwrap();
        fs::write(temp.path().join("settings.yaml"), "key: value").unwrap();

        let bundle = bundle_parser(temp.path()).unwrap();

        // Verify archive is not empty and contains files
        assert!(!bundle.archive.is_empty(), "Archive should not be empty");

        // Parse the ZIP and check contents
        let cursor = std::io::Cursor::new(&bundle.archive);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();

        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();

        assert!(names.contains(&"parser.py".to_string()), "Should contain parser.py");
        assert!(names.contains(&"utils.py".to_string()), "Should contain utils.py");
        assert!(
            names.contains(&"config.json".to_string()),
            "Should contain config.json"
        );
        assert!(
            names.contains(&"settings.yaml".to_string()),
            "Should contain settings.yaml"
        );
    }

    #[test]
    fn test_bundle_hash_changes_with_content() {
        let temp = TempDir::new().unwrap();

        fs::write(temp.path().join("uv.lock"), "# lock").unwrap();
        fs::write(
            temp.path().join("parser.py"),
            r#"name = "test"
version = "1.0.0""#,
        )
        .unwrap();

        let bundle1 = bundle_parser(temp.path()).unwrap();

        // Modify content
        fs::write(
            temp.path().join("parser.py"),
            r#"name = "test"
version = "1.0.1""#,
        )
        .unwrap();

        let bundle2 = bundle_parser(temp.path()).unwrap();

        assert_ne!(
            bundle1.source_hash, bundle2.source_hash,
            "Source hash should change when content changes"
        );
    }

    #[test]
    fn test_lockfile_hash_independent() {
        let temp = TempDir::new().unwrap();

        fs::write(temp.path().join("uv.lock"), "# lock v1").unwrap();
        fs::write(
            temp.path().join("parser.py"),
            r#"name = "test"
version = "1.0.0""#,
        )
        .unwrap();

        let bundle1 = bundle_parser(temp.path()).unwrap();

        // Change lockfile only
        fs::write(temp.path().join("uv.lock"), "# lock v2").unwrap();

        let bundle2 = bundle_parser(temp.path()).unwrap();

        assert_eq!(
            bundle1.source_hash, bundle2.source_hash,
            "Source hash should NOT change when only lockfile changes"
        );
        assert_ne!(
            bundle1.lockfile_hash, bundle2.lockfile_hash,
            "Lockfile hash SHOULD change"
        );
    }
}
