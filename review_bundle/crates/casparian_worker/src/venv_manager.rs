//! Virtual Environment Manager
//!
//! Data-oriented design:
//! - Vec instead of HashMap (we have few venvs, linear search is fine)
//! - All I/O is synchronous (no async lies)
//! - Thread-safe via std::sync::Mutex (not async mutex)
//! - Plain functions where possible, minimal state

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tracing::{info, warn};

/// Default maximum number of venvs to keep cached
const DEFAULT_MAX_VENVS: usize = 50;

/// Default maximum age (days) for unused venvs before cleanup
const DEFAULT_MAX_AGE_DAYS: u32 = 30;

/// Threshold above which we trigger automatic cleanup
const CLEANUP_THRESHOLD: usize = 60;

/// Venv entry - plain data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VenvEntry {
    pub env_hash: String,
    pub created_at: String,
    pub last_used: String,
    pub size_bytes: u64,
}

/// Venv metadata - just a Vec, not a HashMap
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VenvMetadata {
    pub entries: Vec<VenvEntry>,
}

impl VenvMetadata {
    /// Find entry by hash (linear search is fine for ~20 items)
    #[cfg(test)]
    pub fn find(&self, env_hash: &str) -> Option<&VenvEntry> {
        self.entries.iter().find(|e| e.env_hash == env_hash)
    }

    /// Find entry by hash (mutable)
    pub fn find_mut(&mut self, env_hash: &str) -> Option<&mut VenvEntry> {
        self.entries.iter_mut().find(|e| e.env_hash == env_hash)
    }

    /// Add or update entry
    pub fn upsert(&mut self, entry: VenvEntry) {
        if let Some(existing) = self.find_mut(&entry.env_hash) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }
}

/// VenvManager - thread-safe via interior mutability
///
/// Uses std::sync::Mutex for metadata (not tokio::sync::Mutex).
/// This is intentional - all venv operations are blocking I/O,
/// so they should be called from spawn_blocking anyway.
pub struct VenvManager {
    pub venvs_dir: PathBuf,
    pub uv_path: PathBuf,
    metadata_path: PathBuf,
    metadata: Mutex<VenvMetadata>,
}

// VenvManager is automatically Send + Sync because:
// - PathBuf is Send + Sync
// - Mutex<T> is Send + Sync when T: Send
// No unsafe impl needed!

impl VenvManager {
    /// Create a new VenvManager with default path (~/.casparian_flow/venvs).
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("Could not determine home directory")?;

        let venvs_dir = PathBuf::from(&home).join(".casparian_flow/venvs");
        Self::with_path(venvs_dir)
    }

    /// Create a VenvManager with a custom venvs directory.
    /// Useful for testing with isolated temp directories.
    pub fn with_path(venvs_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&venvs_dir)?;

        let metadata_path = venvs_dir.join(".metadata.json");
        let metadata = load_metadata(&metadata_path);

        let uv_path = find_uv()?;

        info!(
            "VenvManager initialized at {}: {} cached envs",
            venvs_dir.display(),
            metadata.entries.len()
        );

        Ok(Self {
            venvs_dir,
            uv_path,
            metadata_path,
            metadata: Mutex::new(metadata),
        })
    }

    /// Get interpreter path for an env hash (no lock needed - pure computation)
    pub fn interpreter_path(&self, env_hash: &str) -> PathBuf {
        let venv_path = self.venvs_dir.join(env_hash);
        if cfg!(windows) {
            venv_path.join("Scripts/python.exe")
        } else {
            venv_path.join("bin/python")
        }
    }

    /// Get or create venv. Synchronous - call from spawn_blocking.
    /// Thread-safe via internal mutex.
    pub fn get_or_create(
        &self, // Note: &self, not &mut self - thread-safe
        env_hash: &str,
        lockfile_content: &str,
        python_version: Option<&str>,
    ) -> Result<PathBuf> {
        let interpreter = self.interpreter_path(env_hash);

        // Cache hit - quick check without heavy operations
        if interpreter.exists() {
            info!("VenvManager: cache hit for {}", truncate_hash(env_hash));
            self.touch(env_hash);
            return Ok(interpreter);
        }

        // Cache miss - create venv (this is the slow path)
        info!(
            "VenvManager: cache miss for {}, creating...",
            truncate_hash(env_hash)
        );

        let venv_path = self.venvs_dir.join(env_hash);
        create_venv(&self.uv_path, &venv_path, lockfile_content, python_version)?;

        // Record metadata (under lock)
        let size = dir_size(&venv_path);
        let now = chrono::Utc::now().to_rfc3339();
        let should_cleanup = {
            let mut metadata = self.metadata.lock().unwrap();
            metadata.upsert(VenvEntry {
                env_hash: env_hash.to_string(),
                created_at: now.clone(),
                last_used: now,
                size_bytes: size,
            });
            // Check if we should run cleanup after releasing lock
            metadata.entries.len() > CLEANUP_THRESHOLD
        };
        self.save_metadata();

        // Proactive cleanup when cache grows large (prevents unbounded memory/disk growth)
        if should_cleanup {
            info!("VenvManager: cache exceeds threshold, running cleanup...");
            self.cleanup(DEFAULT_MAX_VENVS, DEFAULT_MAX_AGE_DAYS);
        }

        info!("VenvManager: created venv for {}", truncate_hash(env_hash));
        Ok(interpreter)
    }

    /// Update last_used timestamp
    fn touch(&self, env_hash: &str) {
        let mut metadata = self.metadata.lock().unwrap();
        if let Some(entry) = metadata.find_mut(env_hash) {
            entry.last_used = chrono::Utc::now().to_rfc3339();
        }
        drop(metadata); // Release lock before I/O
        self.save_metadata();
    }

    /// Save metadata to disk atomically
    ///
    /// Holds the lock during the entire write to prevent race conditions.
    /// Uses atomic write (write to temp file, then rename) for crash safety.
    fn save_metadata(&self) {
        let metadata = self.metadata.lock().unwrap();
        if let Ok(json) = serde_json::to_string_pretty(&*metadata) {
            // Atomic write: write to temp file, then rename
            // This prevents partial writes and race conditions
            let temp_path = self.metadata_path.with_extension("json.tmp");
            if std::fs::write(&temp_path, &json).is_ok() {
                if let Err(e) = std::fs::rename(&temp_path, &self.metadata_path) {
                    warn!("Failed to rename metadata file: {}", e);
                    // Fallback: try direct write (non-atomic but better than nothing)
                    let _ = std::fs::write(&self.metadata_path, &json);
                }
            }
        }
        // Lock is held until function returns - prevents TOCTOU race
    }

    /// Get cache stats
    pub fn stats(&self) -> (usize, u64) {
        let metadata = self.metadata.lock().unwrap();
        let count = metadata.entries.len();
        let total_bytes: u64 = metadata.entries.iter().map(|e| e.size_bytes).sum();
        (count, total_bytes)
    }

    /// Clean up old venvs using LRU eviction
    ///
    /// Removes venvs that:
    /// 1. Haven't been used in `max_age_days` days, OR
    /// 2. Exceed `max_venvs` count (oldest by last_used are removed first)
    ///
    /// Returns the number of venvs removed.
    pub fn cleanup(&self, max_venvs: usize, max_age_days: u32) -> usize {
        let now = chrono::Utc::now();
        let max_age = chrono::Duration::days(max_age_days as i64);

        let mut metadata = self.metadata.lock().unwrap();
        let initial_count = metadata.entries.len();

        // Collect hashes to remove (stale by age)
        let mut to_remove: Vec<String> = metadata
            .entries
            .iter()
            .filter(|e| {
                if let Ok(last_used) = chrono::DateTime::parse_from_rfc3339(&e.last_used) {
                    now.signed_duration_since(last_used.with_timezone(&chrono::Utc)) > max_age
                } else {
                    false // Keep entries with unparseable dates
                }
            })
            .map(|e| e.env_hash.clone())
            .collect();

        // If still over limit after removing stale, remove oldest by last_used
        let remaining_after_stale = initial_count - to_remove.len();
        if remaining_after_stale > max_venvs {
            // Sort remaining entries by last_used (oldest first)
            let mut remaining: Vec<_> = metadata
                .entries
                .iter()
                .filter(|e| !to_remove.contains(&e.env_hash))
                .collect();

            remaining.sort_by(|a, b| a.last_used.cmp(&b.last_used));

            // Mark oldest for removal until we're under the limit
            let excess = remaining_after_stale - max_venvs;
            for entry in remaining.iter().take(excess) {
                to_remove.push(entry.env_hash.clone());
            }
        }

        // Remove from metadata
        metadata
            .entries
            .retain(|e| !to_remove.contains(&e.env_hash));

        // Release lock before I/O operations
        drop(metadata);

        // Delete venv directories from disk
        let mut removed_count = 0;
        for hash in &to_remove {
            let venv_path = self.venvs_dir.join(hash);
            if venv_path.exists() {
                if let Err(e) = std::fs::remove_dir_all(&venv_path) {
                    warn!("Failed to remove venv {}: {}", truncate_hash(hash), e);
                } else {
                    info!("Cleaned up stale venv: {}", truncate_hash(hash));
                    removed_count += 1;
                }
            } else {
                // Entry existed in metadata but not on disk - count as removed
                removed_count += 1;
            }
        }

        // Save updated metadata
        self.save_metadata();

        info!(
            "VenvManager cleanup: removed {} venvs ({} remaining)",
            removed_count,
            initial_count - to_remove.len()
        );

        removed_count
    }
}

// --- Free functions (no state needed) ---

/// Truncate hash for display
fn truncate_hash(hash: &str) -> &str {
    if hash.len() > 12 {
        &hash[..12]
    } else {
        hash
    }
}

fn load_metadata(path: &Path) -> VenvMetadata {
    if !path.exists() {
        return VenvMetadata::default();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
            warn!("Failed to parse venv metadata: {}", e);
            VenvMetadata::default()
        }),
        Err(_) => VenvMetadata::default(),
    }
}

fn find_uv() -> Result<PathBuf> {
    // Check PATH first
    if let Ok(path) = which::which("uv") {
        return Ok(path);
    }

    // Check common locations
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.cargo/bin/uv", home),
        format!("{}/.local/bin/uv", home),
        "/usr/local/bin/uv".to_string(),
    ];

    for candidate in candidates {
        let path = PathBuf::from(&candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    anyhow::bail!("uv not found. Install: curl -LsSf https://astral.sh/uv/install.sh | sh")
}

fn create_venv(
    uv_path: &Path,
    venv_path: &Path,
    lockfile_content: &str,
    python_version: Option<&str>,
) -> Result<()> {
    std::fs::create_dir_all(venv_path)?;

    // Write lockfile
    std::fs::write(venv_path.join("uv.lock"), lockfile_content)?;

    // Write minimal pyproject.toml
    let python_requires = python_version
        .map(|v| format!(">={}", v))
        .unwrap_or_else(|| ">=3.10".to_string());

    let pyproject = format!(
        r#"[project]
name = "casparian-bridge-env"
version = "0.0.1"
requires-python = "{}"
dependencies = []
"#,
        python_requires
    );
    std::fs::write(venv_path.join("pyproject.toml"), pyproject)?;

    // Create venv with uv
    let dot_venv = venv_path.join(".venv");
    let mut cmd = Command::new(uv_path);
    cmd.arg("venv").arg(&dot_venv).current_dir(venv_path);

    if let Some(version) = python_version {
        cmd.arg("--python").arg(version);
    }

    let output = cmd.output().context("Failed to run uv venv")?;
    if !output.status.success() {
        anyhow::bail!(
            "uv venv failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Move .venv contents up (flatter structure)
    if dot_venv.exists() {
        for entry in std::fs::read_dir(&dot_venv)? {
            let entry = entry?;
            std::fs::rename(entry.path(), venv_path.join(entry.file_name()))?;
        }
        std::fs::remove_dir(&dot_venv)?;
    }

    // Sync dependencies
    let output = Command::new(uv_path)
        .args(["sync", "--frozen", "--no-dev"])
        .current_dir(venv_path)
        .env("VIRTUAL_ENV", venv_path)
        .output()
        .context("Failed to run uv sync")?;

    if !output.status.success() {
        anyhow::bail!(
            "uv sync failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_find() {
        let mut meta = VenvMetadata::default();
        assert!(meta.find("abc123").is_none());

        meta.upsert(VenvEntry {
            env_hash: "abc123".to_string(),
            created_at: "2024-01-01".to_string(),
            last_used: "2024-01-01".to_string(),
            size_bytes: 1000,
        });

        assert!(meta.find("abc123").is_some());
        assert_eq!(meta.find("abc123").unwrap().size_bytes, 1000);
    }

    #[test]
    fn test_metadata_upsert_updates_existing() {
        let mut meta = VenvMetadata::default();

        meta.upsert(VenvEntry {
            env_hash: "abc".to_string(),
            created_at: "old".to_string(),
            last_used: "old".to_string(),
            size_bytes: 100,
        });

        meta.upsert(VenvEntry {
            env_hash: "abc".to_string(),
            created_at: "old".to_string(),
            last_used: "new".to_string(),
            size_bytes: 200,
        });

        // Should still be 1 entry, not 2
        assert_eq!(meta.entries.len(), 1);
        assert_eq!(meta.find("abc").unwrap().size_bytes, 200);
    }

    #[test]
    fn test_truncate_hash() {
        assert_eq!(truncate_hash("abc"), "abc");
        assert_eq!(truncate_hash("123456789012"), "123456789012");
        assert_eq!(truncate_hash("1234567890123"), "123456789012");
    }
}
