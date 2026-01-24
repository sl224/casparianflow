//! Telemetry schema v1 definitions and hashing utilities.
//!
//! Event names are intended for Tape domain events (privacy-safe).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Telemetry event names (Tape domain events).
pub mod events {
    /// Scan lifecycle events.
    pub const SCAN_START: &str = "scan.start";
    pub const SCAN_PROGRESS: &str = "scan.progress";
    pub const SCAN_COMPLETE: &str = "scan.complete";
    pub const SCAN_FAIL: &str = "scan.fail";

    /// Run lifecycle events.
    pub const RUN_START: &str = "run.start";
    pub const RUN_PROGRESS: &str = "run.progress";
    pub const RUN_COMPLETE: &str = "run.complete";
    pub const RUN_FAIL: &str = "run.fail";

    /// Query execution event.
    pub const QUERY_EXEC: &str = "query.exec";
}

/// Context for correlating telemetry across spans/events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryContext {
    pub run_id: Option<String>,
    pub source_id: Option<String>,
    pub job_id: Option<String>,
}

/// Scan configuration snapshot for telemetry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfigTelemetry {
    pub threads: usize,
    pub batch_size: usize,
    pub progress_interval: usize,
    pub follow_symlinks: bool,
    pub include_hidden: bool,
    pub compute_stats: bool,
}

/// Scan start payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStarted {
    pub run_id: String,
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_hash: Option<String>,
    pub started_at: DateTime<Utc>,
    pub config: ScanConfigTelemetry,
}

/// Scan progress payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub run_id: String,
    pub source_id: String,
    pub elapsed_ms: u64,
    pub files_found: usize,
    pub files_persisted: usize,
    pub dirs_scanned: usize,
    pub files_per_sec: f64,
    pub stalled: bool,
}

/// Scan completion payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCompleted {
    pub run_id: String,
    pub source_id: String,
    pub duration_ms: u64,
    pub files_discovered: u64,
    pub files_persisted: u64,
    pub files_new: u64,
    pub files_changed: u64,
    pub files_deleted: u64,
    pub dirs_scanned: u64,
    pub bytes_scanned: u64,
    pub errors: u64,
}

/// Scan failure payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanFailed {
    pub run_id: String,
    pub source_id: String,
    pub duration_ms: u64,
    pub error_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_kind: Option<String>,
}

/// Run start payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStarted {
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sink_hash: Option<String>,
    pub started_at: DateTime<Utc>,
}

/// Run completion payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCompleted {
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub duration_ms: u64,
    pub total_rows: u64,
    pub outputs: usize,
}

/// Run failure payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunFailed {
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub duration_ms: u64,
    pub error_class: String,
}

/// Telemetry hashing utility with a persistent machine-local salt.
#[derive(Debug, Clone)]
pub struct TelemetryHasher {
    salt: [u8; 32],
}

impl TelemetryHasher {
    /// Load the machine-local salt, creating it if missing or invalid.
    pub fn load_or_create() -> std::io::Result<Self> {
        let path = telemetry_salt_path();
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Some(bytes) = decode_hex(contents.trim()) {
                if bytes.len() == 32 {
                    let mut salt = [0u8; 32];
                    salt.copy_from_slice(&bytes);
                    return Ok(Self { salt });
                }
            }
        }

        let salt = generate_salt();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, encode_hex(&salt))?;
        Ok(Self { salt })
    }

    /// Hash a path (canonicalized when possible).
    pub fn hash_path(&self, path: &Path) -> String {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.hash_str(&canonical.to_string_lossy())
    }

    /// Hash an arbitrary string.
    pub fn hash_str(&self, value: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.salt);
        hasher.update(value.as_bytes());
        hasher.finalize().to_hex()[..16].to_string()
    }
}

fn telemetry_salt_path() -> PathBuf {
    casparian_home().join("telemetry").join("salt")
}

fn casparian_home() -> PathBuf {
    if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        return PathBuf::from(override_path);
    }
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(home).join(".casparian_flow");
    }
    PathBuf::from(".").join(".casparian_flow")
}

fn generate_salt() -> [u8; 32] {
    let uuid1 = uuid::Uuid::new_v4();
    let uuid2 = uuid::Uuid::new_v4();
    let mut salt = [0u8; 32];
    salt[..16].copy_from_slice(uuid1.as_bytes());
    salt[16..].copy_from_slice(uuid2.as_bytes());
    salt
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(s.len() / 2);
    let iter = s.as_bytes().chunks(2);
    for pair in iter {
        let hex = std::str::from_utf8(pair).ok()?;
        let value = u8::from_str_radix(hex, 16).ok()?;
        bytes.push(value);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    struct EnvGuard {
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(home: &Path) -> Self {
            let prev = std::env::var("CASPARIAN_HOME").ok();
            std::env::set_var("CASPARIAN_HOME", home);
            Self { prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.prev.take() {
                std::env::set_var("CASPARIAN_HOME", value);
            } else {
                std::env::remove_var("CASPARIAN_HOME");
            }
        }
    }

    #[test]
    fn test_hash_path_redacts_raw_values() {
        let temp = TempDir::new().unwrap();
        let _env = EnvGuard::set(temp.path());
        let hasher = TelemetryHasher::load_or_create().unwrap();
        let path = Path::new("/Users/alice/Documents/secret.csv");
        let hash = hasher.hash_path(path);

        assert_eq!(hash.len(), 16);
        assert!(!hash.contains("alice"));
        assert!(!hash.contains("secret"));
    }

    #[test]
    fn test_salt_persists_across_loads() {
        let temp = TempDir::new().unwrap();
        let _env = EnvGuard::set(temp.path());
        let hasher1 = TelemetryHasher::load_or_create().unwrap();
        let hash1 = hasher1.hash_str("value");

        let hasher2 = TelemetryHasher::load_or_create().unwrap();
        let hash2 = hasher2.hash_str("value");

        assert_eq!(hash1, hash2);
    }
}
