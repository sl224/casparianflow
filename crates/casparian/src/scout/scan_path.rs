use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum ScanPathError {
    NotFound(PathBuf),
    NotDirectory(PathBuf),
    NotReadable(PathBuf),
}

impl fmt::Display for ScanPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanPathError::NotFound(path) => write!(f, "Path not found: {}", path.display()),
            ScanPathError::NotDirectory(path) => write!(f, "Not a directory: {}", path.display()),
            ScanPathError::NotReadable(path) => write!(f, "Cannot read directory: {}", path.display()),
        }
    }
}

pub fn expand_scan_path(path: &Path) -> PathBuf {
    if path.starts_with("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(path.strip_prefix("~").unwrap_or(path));
        }
    }
    path.to_path_buf()
}

pub fn canonicalize_scan_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn validate_scan_path(path: &Path) -> Result<(), ScanPathError> {
    if !path.exists() {
        return Err(ScanPathError::NotFound(path.to_path_buf()));
    }
    if !path.is_dir() {
        return Err(ScanPathError::NotDirectory(path.to_path_buf()));
    }
    if std::fs::read_dir(path).is_err() {
        return Err(ScanPathError::NotReadable(path.to_path_buf()));
    }
    Ok(())
}
