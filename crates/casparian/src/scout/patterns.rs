//! Shared glob pattern normalization and matching helpers.

use globset::{GlobBuilder, GlobMatcher};

/// Normalize a glob pattern for matching against relative paths.
///
/// Rules:
/// - Empty or "*" becomes "**/*" (match all)
/// - Leading slashes are stripped (relative matching)
/// - Patterns without a path separator get "**/" prefix
pub fn normalize_glob_pattern(raw: &str) -> String {
    let mut pattern = raw.trim().trim_start_matches('/').to_string();

    if pattern.is_empty() || pattern == "*" {
        pattern = "**/*".to_string();
    }

    if !pattern.contains('/') && !pattern.starts_with("**/") && pattern != "**/*" {
        pattern = format!("**/{}", pattern);
    }

    pattern
}

/// Build a case-insensitive glob matcher from a normalized pattern.
pub fn build_matcher(glob_pattern: &str) -> Result<GlobMatcher, String> {
    GlobBuilder::new(glob_pattern)
        .case_insensitive(true)
        .build()
        .map(|g| g.compile_matcher())
        .map_err(|_| "Invalid pattern".to_string())
}

/// Match a raw glob pattern against a path.
pub fn matches(raw_pattern: &str, path: &str) -> Result<bool, String> {
    let normalized = normalize_glob_pattern(raw_pattern);
    let matcher = build_matcher(&normalized)?;
    let candidate = path.trim_start_matches('/');
    Ok(matcher.is_match(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_basics() {
        assert_eq!(normalize_glob_pattern(""), "**/*");
        assert_eq!(normalize_glob_pattern("*"), "**/*");
        assert_eq!(normalize_glob_pattern("data.csv"), "**/data.csv");
        assert_eq!(normalize_glob_pattern("data/*.csv"), "data/*.csv");
        assert_eq!(normalize_glob_pattern("/data/*.csv"), "data/*.csv");
    }

    #[test]
    fn matches_relative_paths() {
        assert!(matches("*.csv", "data.csv").unwrap());
        assert!(matches("*.csv", "path/to/data.csv").unwrap());
        assert!(matches("data/*.csv", "data/file.csv").unwrap());
        assert!(matches("**/*.json", "deep/nested/file.json").unwrap());
        assert!(!matches("*.csv", "data.json").unwrap());
    }
}
