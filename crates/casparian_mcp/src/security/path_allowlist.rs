//! Path Allowlist - Validates File Paths
//!
//! Prevents path traversal attacks and symlink escapes by validating
//! all paths against configured allowed roots.
//!
//! # Security Model
//!
//! - All paths are canonicalized before validation
//! - ".." components are explicitly denied
//! - Symlinks are followed and validated
//! - Default root is current working directory

use super::SecurityError;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Path allowlist for validating file operations
#[derive(Debug, Clone)]
pub struct PathAllowlist {
    /// Canonicalized allowed root paths
    roots: Vec<PathBuf>,
}

impl PathAllowlist {
    /// Create a new allowlist with the given roots
    pub fn new(roots: Vec<PathBuf>) -> Self {
        // Canonicalize all roots at construction time
        let roots = roots
            .into_iter()
            .filter_map(|p| {
                match p.canonicalize() {
                    Ok(canonical) => Some(canonical),
                    Err(e) => {
                        warn!("Failed to canonicalize allowed path {:?}: {}", p, e);
                        None
                    }
                }
            })
            .collect();

        Self { roots }
    }

    /// Create an allowlist with the current working directory as the only root
    pub fn cwd_only() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new(vec![cwd])
    }

    /// Add a root to the allowlist
    pub fn add_root(&mut self, root: PathBuf) -> Result<(), SecurityError> {
        let canonical = root.canonicalize().map_err(|e| SecurityError::PathNotAllowed {
            path: format!("{}: {}", root.display(), e),
        })?;
        self.roots.push(canonical);
        Ok(())
    }

    /// Check if a path contains traversal attempts
    fn contains_traversal(path: &Path) -> bool {
        path.components().any(|c| matches!(c, std::path::Component::ParentDir))
    }

    /// Validate a path is within allowed roots
    ///
    /// Returns the canonicalized path if valid.
    pub fn validate(&self, path: &Path) -> Result<PathBuf, SecurityError> {
        // Check for explicit traversal attempts in the original path
        if Self::contains_traversal(path) {
            return Err(SecurityError::PathTraversal {
                path: path.display().to_string(),
            });
        }

        // Canonicalize to resolve symlinks and get absolute path
        let canonical = path.canonicalize().map_err(|_| SecurityError::PathNotAllowed {
            path: path.display().to_string(),
        })?;

        // Check against all allowed roots
        for root in &self.roots {
            if canonical.starts_with(root) {
                return Ok(canonical);
            }
        }

        // Path is not within any allowed root
        Err(SecurityError::PathNotAllowed {
            path: path.display().to_string(),
        })
    }

    /// Check if a path would be valid without canonicalizing
    ///
    /// Useful for checking paths that may not exist yet.
    pub fn would_be_allowed(&self, path: &Path) -> bool {
        // Check for traversal
        if Self::contains_traversal(path) {
            return false;
        }

        // Get absolute path without resolving symlinks
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            match std::env::current_dir() {
                Ok(cwd) => cwd.join(path),
                Err(_) => return false,
            }
        };

        // Check if it would be under an allowed root
        for root in &self.roots {
            if absolute.starts_with(root) {
                return true;
            }
        }

        false
    }

    /// Get the allowed roots (for display/debugging)
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_valid_path_within_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "test").unwrap();

        let allowlist = PathAllowlist::new(vec![root]);
        let result = allowlist.validate(&file);

        assert!(result.is_ok());
    }

    #[test]
    fn test_path_outside_root() {
        let temp = TempDir::new().unwrap();
        let other_temp = TempDir::new().unwrap();
        let other_file = other_temp.path().join("test.txt");
        std::fs::write(&other_file, "test").unwrap();

        let allowlist = PathAllowlist::new(vec![temp.path().to_path_buf()]);
        let result = allowlist.validate(&other_file);

        assert!(matches!(result, Err(SecurityError::PathNotAllowed { .. })));
    }

    #[test]
    fn test_traversal_attack() {
        let temp = TempDir::new().unwrap();
        let allowlist = PathAllowlist::new(vec![temp.path().to_path_buf()]);

        // Try to escape with ..
        let malicious = temp.path().join("subdir").join("..").join("..").join("etc").join("passwd");
        let result = allowlist.validate(&malicious);

        assert!(matches!(result, Err(SecurityError::PathTraversal { .. })));
    }

    #[test]
    fn test_nonexistent_path() {
        let temp = TempDir::new().unwrap();
        let allowlist = PathAllowlist::new(vec![temp.path().to_path_buf()]);

        let nonexistent = temp.path().join("does_not_exist.txt");
        let result = allowlist.validate(&nonexistent);

        // Should fail because canonicalize fails for nonexistent paths
        assert!(matches!(result, Err(SecurityError::PathNotAllowed { .. })));
    }

    #[test]
    fn test_would_be_allowed() {
        let temp = TempDir::new().unwrap();
        let allowlist = PathAllowlist::new(vec![temp.path().to_path_buf()]);

        // Path that doesn't exist but would be valid
        let future_file = temp.path().join("future.txt");
        assert!(allowlist.would_be_allowed(&future_file));

        // Traversal should fail
        let traversal = temp.path().join("..").join("escape");
        assert!(!allowlist.would_be_allowed(&traversal));
    }

    #[test]
    fn test_multiple_roots() {
        let temp1 = TempDir::new().unwrap();
        let temp2 = TempDir::new().unwrap();

        let file1 = temp1.path().join("test1.txt");
        let file2 = temp2.path().join("test2.txt");
        std::fs::write(&file1, "test1").unwrap();
        std::fs::write(&file2, "test2").unwrap();

        let allowlist = PathAllowlist::new(vec![
            temp1.path().to_path_buf(),
            temp2.path().to_path_buf(),
        ]);

        assert!(allowlist.validate(&file1).is_ok());
        assert!(allowlist.validate(&file2).is_ok());
    }
}
