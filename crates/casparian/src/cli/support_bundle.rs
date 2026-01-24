//! `casparian support-bundle` command - export debug bundle for support.
//!
//! Creates a zip file containing:
//! - Tape files (session recordings)
//! - Log files (rolling app logs)
//! - Redacted configuration
//! - System metadata (version, git hash, platform)
//!
//! # Usage
//!
//! ```bash
//! # Basic usage - creates bundle at specified path
//! casparian support-bundle ./debug_bundle.zip
//!
//! # With custom tape directory
//! casparian support-bundle ./debug_bundle.zip --tape-dir ~/.casparian_flow/tapes
//!
//! # Exclude tapes
//! casparian support-bundle ./debug_bundle.zip --no-tapes
//!
//! # Exclude config
//! casparian support-bundle ./debug_bundle.zip --no-config
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Args;
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use super::config::casparian_home;

/// Arguments for the `support-bundle` command
#[derive(Debug, Args)]
pub struct SupportBundleArgs {
    /// Output path for the zip bundle
    #[arg(value_name = "OUTPUT_PATH")]
    pub output: PathBuf,

    /// Include tape files (default: true)
    #[arg(long = "no-tapes", action = clap::ArgAction::SetFalse)]
    pub include_tapes: bool,

    /// Include configuration (redacted) (default: true)
    #[arg(long = "no-config", action = clap::ArgAction::SetFalse)]
    pub include_config: bool,

    /// Include log files (default: true)
    #[arg(long = "no-logs", action = clap::ArgAction::SetFalse)]
    pub include_logs: bool,

    /// Directory containing tape files
    /// (default: ~/.casparian_flow/tapes)
    #[arg(long = "tape-dir")]
    pub tape_dir: Option<PathBuf>,

    /// Directory containing log files
    /// (default: ~/.casparian_flow/logs)
    #[arg(long = "log-dir")]
    pub log_dir: Option<PathBuf>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Bundle manifest metadata
#[derive(Debug, Serialize)]
struct BundleManifest {
    /// Schema version for the bundle format
    version: &'static str,
    /// When the bundle was created
    created_at: DateTime<Utc>,
    /// Casparian version (from Cargo.toml)
    casparian_version: &'static str,
    /// Git commit hash (if available)
    git_hash: Option<String>,
    /// Redaction mode used for config
    redaction_mode: &'static str,
    /// Platform information
    platform: PlatformInfo,
    /// Contents summary
    contents: BundleContents,
}

/// Platform information for debugging
#[derive(Debug, Serialize)]
struct PlatformInfo {
    os: String,
    arch: String,
    rust_version: &'static str,
}

/// Summary of bundle contents
#[derive(Debug, Serialize)]
struct BundleContents {
    /// List of tape files included
    tapes: Vec<String>,
    /// List of log files included
    logs: Vec<String>,
    /// Whether config was included
    config: bool,
}

/// Result of bundle creation
#[derive(Debug, Serialize)]
pub struct BundleResult {
    /// Path to the created bundle
    output_path: String,
    /// Size of the bundle in bytes
    size_bytes: u64,
    /// Number of tape files included
    tape_count: usize,
    /// Number of log files included
    log_count: usize,
    /// Whether config was included
    config_included: bool,
}

/// Redacted configuration for support bundle
#[derive(Debug, Serialize)]
struct RedactedConfig {
    /// Database backend type
    database_backend: String,
    /// Whether database file exists
    database_exists: bool,
    /// Whether output directory exists
    output_dir_exists: bool,
    /// Whether venvs directory exists
    venvs_dir_exists: bool,
    /// Whether parsers directory exists
    parsers_dir_exists: bool,
    /// Hash of the home directory path (for correlation)
    home_path_hash: String,
}

/// Get the default tape directory
fn default_tape_dir() -> PathBuf {
    casparian_home().join("tapes")
}

/// Get the default log directory
fn default_log_dir() -> PathBuf {
    super::config::logs_dir()
}

/// Get git commit hash if available
fn get_git_hash() -> Option<String> {
    // Try to read from git
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Hash a string for redaction (first 16 hex chars of blake3)
fn redact_hash(s: &str) -> String {
    let hash = blake3::hash(s.as_bytes());
    hash.to_hex()[..16].to_string()
}

/// Create a support bundle
pub struct SupportBundle {
    output_path: PathBuf,
    include_tapes: bool,
    include_config: bool,
    include_logs: bool,
    tape_dir: PathBuf,
    log_dir: PathBuf,
}

impl SupportBundle {
    /// Create a new SupportBundle builder
    pub fn new(output_path: PathBuf) -> Self {
        Self {
            output_path,
            include_tapes: true,
            include_config: true,
            include_logs: true,
            tape_dir: default_tape_dir(),
            log_dir: default_log_dir(),
        }
    }

    /// Set whether to include tapes
    pub fn with_tapes(mut self, include: bool) -> Self {
        self.include_tapes = include;
        self
    }

    /// Set whether to include config
    pub fn with_config(mut self, include: bool) -> Self {
        self.include_config = include;
        self
    }

    /// Set whether to include logs
    pub fn with_logs(mut self, include: bool) -> Self {
        self.include_logs = include;
        self
    }

    /// Set the tape directory
    pub fn with_tape_dir(mut self, dir: PathBuf) -> Self {
        self.tape_dir = dir;
        self
    }

    /// Set the log directory
    pub fn with_log_dir(mut self, dir: PathBuf) -> Self {
        self.log_dir = dir;
        self
    }

    /// Create the support bundle
    pub fn create(&self) -> Result<BundleResult> {
        let file = File::create(&self.output_path)
            .with_context(|| format!("Failed to create bundle file: {}", self.output_path.display()))?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(6));

        let mut tape_names = Vec::new();
        let mut log_names = Vec::new();

        // Add tapes if enabled
        if self.include_tapes && self.tape_dir.exists() {
            tape_names = self.add_tapes(&mut zip, options)?;
        }

        // Add logs if enabled
        if self.include_logs && self.log_dir.exists() {
            log_names = self.add_logs(&mut zip, options)?;
        }

        // Add config if enabled
        if self.include_config {
            self.add_config(&mut zip, options)?;
        }

        // Add manifest
        let manifest = BundleManifest {
            version: "1.0",
            created_at: Utc::now(),
            casparian_version: env!("CARGO_PKG_VERSION"),
            git_hash: get_git_hash(),
            redaction_mode: "hash",
            platform: PlatformInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                rust_version: env!("CARGO_PKG_RUST_VERSION"),
            },
            contents: BundleContents {
                tapes: tape_names.clone(),
                logs: log_names.clone(),
                config: self.include_config,
            },
        };
        self.add_manifest(&mut zip, options, &manifest)?;

        zip.finish()?;

        // Get the file size
        let metadata = std::fs::metadata(&self.output_path)?;

        Ok(BundleResult {
            output_path: self.output_path.display().to_string(),
            size_bytes: metadata.len(),
            tape_count: tape_names.len(),
            log_count: log_names.len(),
            config_included: self.include_config,
        })
    }

    /// Add tape files to the zip
    fn add_tapes(
        &self,
        zip: &mut ZipWriter<File>,
        options: SimpleFileOptions,
    ) -> Result<Vec<String>> {
        let mut tape_names = Vec::new();

        if !self.tape_dir.exists() {
            return Ok(tape_names);
        }

        for entry in std::fs::read_dir(&self.tape_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only include .tape files
            if path.extension().map_or(false, |ext| ext == "tape") {
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                if !file_name.is_empty() {
                    let archive_path = format!("tapes/{}", file_name);
                    zip.start_file(&archive_path, options)?;

                    let mut file = File::open(&path)?;
                    let mut buffer = Vec::new();
                    file.read_to_end(&mut buffer)?;
                    zip.write_all(&buffer)?;

                    tape_names.push(file_name);
                }
            }
        }

        Ok(tape_names)
    }

    /// Add log files to the zip
    fn add_logs(
        &self,
        zip: &mut ZipWriter<File>,
        options: SimpleFileOptions,
    ) -> Result<Vec<String>> {
        let mut log_names = Vec::new();

        if !self.log_dir.exists() {
            return Ok(log_names);
        }

        for entry in std::fs::read_dir(&self.log_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if file_name.is_empty() || !file_name.contains(".log") {
                continue;
            }

            let archive_path = format!("logs/{}", file_name);
            zip.start_file(&archive_path, options)?;

            let mut file = File::open(&path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;

            log_names.push(file_name);
        }

        Ok(log_names)
    }

    /// Add redacted configuration to the zip
    fn add_config(
        &self,
        zip: &mut ZipWriter<File>,
        options: SimpleFileOptions,
    ) -> Result<()> {
        let home = casparian_home();
        let config = RedactedConfig {
            database_backend: super::config::default_db_backend().as_str().to_string(),
            database_exists: super::config::active_db_path().exists(),
            output_dir_exists: super::config::output_dir().exists(),
            venvs_dir_exists: super::config::venvs_dir().exists(),
            parsers_dir_exists: super::config::parsers_dir().exists(),
            home_path_hash: redact_hash(&home.display().to_string()),
        };

        let config_json = serde_json::to_string_pretty(&config)?;

        zip.start_file("config/redacted_config.json", options)?;
        zip.write_all(config_json.as_bytes())?;

        Ok(())
    }

    /// Add manifest to the zip
    fn add_manifest(
        &self,
        zip: &mut ZipWriter<File>,
        options: SimpleFileOptions,
        manifest: &BundleManifest,
    ) -> Result<()> {
        let manifest_json = serde_json::to_string_pretty(manifest)?;

        zip.start_file("bundle.json", options)?;
        zip.write_all(manifest_json.as_bytes())?;

        Ok(())
    }
}

/// Execute the support-bundle command
pub fn run(args: SupportBundleArgs) -> Result<()> {
    let tape_dir = args.tape_dir.unwrap_or_else(default_tape_dir);
    let log_dir = args.log_dir.unwrap_or_else(default_log_dir);

    let bundle = SupportBundle::new(args.output)
        .with_tapes(args.include_tapes)
        .with_config(args.include_config)
        .with_logs(args.include_logs)
        .with_tape_dir(tape_dir)
        .with_log_dir(log_dir);

    let result = bundle.create()?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Support bundle created: {}", result.output_path);
        println!();
        println!("Contents:");
        println!("  Tape files: {}", result.tape_count);
        println!("  Log files: {}", result.log_count);
        println!("  Config included: {}", if result.config_included { "yes" } else { "no" });
        println!();
        println!("Bundle size: {} bytes", result.size_bytes);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bundle_creation_empty() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("bundle.zip");
        let tape_dir = temp_dir.path().join("tapes");
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir_all(&tape_dir).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();

        let bundle = SupportBundle::new(output_path.clone())
            .with_tape_dir(tape_dir)
            .with_log_dir(log_dir);

        let result = bundle.create().unwrap();

        assert!(output_path.exists());
        assert_eq!(result.tape_count, 0);
        assert_eq!(result.log_count, 0);
        assert!(result.config_included);
        assert!(result.size_bytes > 0);
    }

    #[test]
    fn test_bundle_with_tapes() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("bundle.zip");
        let tape_dir = temp_dir.path().join("tapes");
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir_all(&tape_dir).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();

        // Create test tape files
        std::fs::write(tape_dir.join("session_001.tape"), r#"{"event": "test1"}"#).unwrap();
        std::fs::write(tape_dir.join("session_002.tape"), r#"{"event": "test2"}"#).unwrap();
        // Non-tape file should be ignored
        std::fs::write(tape_dir.join("other.txt"), "ignored").unwrap();

        let bundle = SupportBundle::new(output_path.clone())
            .with_tape_dir(tape_dir)
            .with_log_dir(log_dir);

        let result = bundle.create().unwrap();

        assert!(output_path.exists());
        assert_eq!(result.tape_count, 2);
        assert_eq!(result.log_count, 0);
    }

    #[test]
    fn test_bundle_no_tapes() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("bundle.zip");
        let tape_dir = temp_dir.path().join("tapes");
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir_all(&tape_dir).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();

        // Create test tape file
        std::fs::write(tape_dir.join("session.tape"), r#"{"event": "test"}"#).unwrap();

        let bundle = SupportBundle::new(output_path.clone())
            .with_tape_dir(tape_dir)
            .with_log_dir(log_dir)
            .with_tapes(false);

        let result = bundle.create().unwrap();

        assert_eq!(result.tape_count, 0);
        assert_eq!(result.log_count, 0);
    }

    #[test]
    fn test_bundle_no_config() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("bundle.zip");
        let tape_dir = temp_dir.path().join("tapes");
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir_all(&tape_dir).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();

        let bundle = SupportBundle::new(output_path.clone())
            .with_tape_dir(tape_dir)
            .with_log_dir(log_dir)
            .with_config(false);

        let result = bundle.create().unwrap();

        assert!(!result.config_included);
    }

    #[test]
    fn test_bundle_manifest_contents() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("bundle.zip");
        let tape_dir = temp_dir.path().join("tapes");
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir_all(&tape_dir).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();

        std::fs::write(tape_dir.join("test.tape"), r#"{"event": "test"}"#).unwrap();

        let bundle = SupportBundle::new(output_path.clone())
            .with_tape_dir(tape_dir)
            .with_log_dir(log_dir);

        bundle.create().unwrap();

        // Read the zip and verify manifest
        let file = File::open(&output_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();

        // Find and read bundle.json
        let mut manifest_file = archive.by_name("bundle.json").unwrap();
        let mut contents = String::new();
        manifest_file.read_to_string(&mut contents).unwrap();

        let manifest: serde_json::Value = serde_json::from_str(&contents).unwrap();

        assert_eq!(manifest["version"], "1.0");
        assert_eq!(manifest["redaction_mode"], "hash");
        assert!(manifest["created_at"].is_string());
        assert!(manifest["casparian_version"].is_string());
        assert!(manifest["contents"]["tapes"].as_array().unwrap().len() == 1);
        assert!(manifest["contents"]["logs"].as_array().unwrap().len() == 0);
        assert_eq!(manifest["contents"]["config"], true);
    }

    #[test]
    fn test_redact_hash() {
        let hash1 = redact_hash("/Users/test/path");
        let hash2 = redact_hash("/Users/test/path");
        let hash3 = redact_hash("/Users/other/path");

        // Same input = same hash
        assert_eq!(hash1, hash2);
        // Different input = different hash
        assert_ne!(hash1, hash3);
        // Hash length is 16 characters
        assert_eq!(hash1.len(), 16);
        // Hash is hex characters
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_nonexistent_tape_dir() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("bundle.zip");
        let tape_dir = temp_dir.path().join("nonexistent_tapes");
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();

        let bundle = SupportBundle::new(output_path.clone())
            .with_tape_dir(tape_dir)
            .with_log_dir(log_dir);

        let result = bundle.create().unwrap();

        // Should succeed with 0 tapes
        assert_eq!(result.tape_count, 0);
        assert_eq!(result.log_count, 0);
    }

    #[test]
    fn test_bundle_with_logs() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("bundle.zip");
        let tape_dir = temp_dir.path().join("tapes");
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir_all(&tape_dir).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();

        std::fs::write(log_dir.join("casparian.log.20260124"), "log1").unwrap();
        std::fs::write(log_dir.join("casparian-sentinel.log.20260124"), "log2").unwrap();
        std::fs::write(log_dir.join("readme.txt"), "ignored").unwrap();

        let bundle = SupportBundle::new(output_path.clone())
            .with_tape_dir(tape_dir)
            .with_log_dir(log_dir);

        let result = bundle.create().unwrap();

        assert_eq!(result.log_count, 2);
    }
}
