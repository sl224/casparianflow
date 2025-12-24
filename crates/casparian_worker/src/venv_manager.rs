//! Virtual Environment Manager
//!
//! Data-oriented design:
//! - Vec instead of HashMap (we have few venvs, linear search is fine)
//! - All I/O is synchronous (no async lies)
//! - Plain functions where possible, minimal state

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

const UV_TIMEOUT_SECONDS: u64 = 300;

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

/// VenvManager - holds paths and metadata, created once at startup
pub struct VenvManager {
    pub venvs_dir: PathBuf,
    pub uv_path: PathBuf,
    metadata_path: PathBuf,
    metadata: VenvMetadata,
}

impl VenvManager {
    /// Create a new VenvManager. Call once at startup.
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("Could not determine home directory")?;

        let venvs_dir = PathBuf::from(&home).join(".casparian_flow/venvs");
        std::fs::create_dir_all(&venvs_dir)?;

        let metadata_path = venvs_dir.join(".metadata.json");
        let metadata = load_metadata(&metadata_path);

        let uv_path = find_uv()?;

        info!("VenvManager initialized: {} cached envs", metadata.entries.len());

        Ok(Self {
            venvs_dir,
            uv_path,
            metadata_path,
            metadata,
        })
    }

    /// Get interpreter path for an env hash
    pub fn interpreter_path(&self, env_hash: &str) -> PathBuf {
        let venv_path = self.venvs_dir.join(env_hash);
        if cfg!(windows) {
            venv_path.join("Scripts/python.exe")
        } else {
            venv_path.join("bin/python")
        }
    }

    /// Check if venv exists (just check if interpreter exists)
    pub fn exists(&self, env_hash: &str) -> bool {
        self.interpreter_path(env_hash).exists()
    }

    /// Get or create venv. Synchronous - call from spawn_blocking if needed.
    pub fn get_or_create(
        &mut self,
        env_hash: &str,
        lockfile_content: &str,
        python_version: Option<&str>,
    ) -> Result<PathBuf> {
        let interpreter = self.interpreter_path(env_hash);

        // Cache hit
        if interpreter.exists() {
            info!("VenvManager: cache hit for {}", &env_hash[..12]);
            self.touch(env_hash);
            return Ok(interpreter);
        }

        // Cache miss - create venv
        info!("VenvManager: cache miss for {}, creating...", &env_hash[..12]);

        let venv_path = self.venvs_dir.join(env_hash);
        create_venv(&self.uv_path, &venv_path, lockfile_content, python_version)?;

        // Record metadata
        let size = dir_size(&venv_path);
        let now = chrono::Utc::now().to_rfc3339();
        self.metadata.upsert(VenvEntry {
            env_hash: env_hash.to_string(),
            created_at: now.clone(),
            last_used: now,
            size_bytes: size,
        });
        self.save_metadata();

        info!("VenvManager: created venv for {}", &env_hash[..12]);
        Ok(interpreter)
    }

    /// Update last_used timestamp
    fn touch(&mut self, env_hash: &str) {
        if let Some(entry) = self.metadata.find_mut(env_hash) {
            entry.last_used = chrono::Utc::now().to_rfc3339();
            self.save_metadata();
        }
    }

    /// Save metadata to disk
    fn save_metadata(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.metadata) {
            let _ = std::fs::write(&self.metadata_path, json);
        }
    }

    /// Get cache stats
    pub fn stats(&self) -> (usize, u64) {
        let count = self.metadata.entries.len();
        let total_bytes: u64 = self.metadata.entries.iter().map(|e| e.size_bytes).sum();
        (count, total_bytes)
    }
}

// --- Free functions (no state needed) ---

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
        anyhow::bail!("uv venv failed: {}", String::from_utf8_lossy(&output.stderr));
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
        anyhow::bail!("uv sync failed: {}", String::from_utf8_lossy(&output.stderr));
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
    fn test_interpreter_path_unix() {
        let mgr = VenvManager {
            venvs_dir: PathBuf::from("/tmp/venvs"),
            uv_path: PathBuf::from("/usr/bin/uv"),
            metadata_path: PathBuf::from("/tmp/meta.json"),
            metadata: VenvMetadata::default(),
        };

        let path = mgr.interpreter_path("abc123");
        assert!(path.to_string_lossy().contains("abc123"));
        assert!(path.to_string_lossy().contains("python"));
    }
}
