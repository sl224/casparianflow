//! Semantic primitive pattern matching
//!
//! Recognizes common folder naming patterns without AI.

use super::{DateFormat, SemanticPrimitive};
use regex::Regex;
use std::sync::LazyLock;

/// Pattern matchers for each primitive type
struct PrimitivePatterns {
    /// ISO date: YYYY-MM-DD
    iso_date: Regex,
    /// US date: MM-DD-YYYY
    us_date: Regex,
    /// European date: DD-MM-YYYY
    eu_date: Regex,
    /// Year-month: YYYY-MM
    year_month: Regex,
    /// Compact date: YYYYMMDD
    compact_date: Regex,
    /// Year only: 19xx, 20xx, FY20xx
    year_only: Regex,
    /// Month name or number
    month: Regex,
    /// Day number
    day: Regex,
    /// Version pattern: v1, v2.3, release-1.0
    version: Regex,
    /// Entity pattern: prefix_id (mission_042, client_abc)
    entity: Regex,
    /// File extension
    file_ext: Regex,
}

static PATTERNS: LazyLock<PrimitivePatterns> = LazyLock::new(|| PrimitivePatterns {
    iso_date: Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap(),
    us_date: Regex::new(r"^\d{2}-\d{2}-\d{4}$").unwrap(),
    eu_date: Regex::new(r"^\d{2}-\d{2}-\d{4}$").unwrap(),
    year_month: Regex::new(r"^\d{4}-\d{2}$").unwrap(),
    compact_date: Regex::new(r"^\d{8}$").unwrap(),
    year_only: Regex::new(r"^(?:FY)?(?:19|20)\d{2}$").unwrap(),
    month: Regex::new(r"^(?:0[1-9]|1[0-2]|January|February|March|April|May|June|July|August|September|October|November|December|Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)$").unwrap(),
    day: Regex::new(r"^(?:0[1-9]|[12]\d|3[01]|Day\d{1,2})$").unwrap(),
    version: Regex::new(r"^v?\d+(?:\.\d+)*$|^release-\d+(?:\.\d+)*$").unwrap(),
    entity: Regex::new(r"^[a-zA-Z]+[_-]\w+$").unwrap(),
    file_ext: Regex::new(r"\.\w+$").unwrap(),
});

/// Common category folder names
const CATEGORY_FOLDERS: &[&str] = &[
    "reports", "logs", "data", "output", "input", "raw", "processed",
    "archive", "backup", "temp", "cache", "config", "docs", "src",
    "bin", "lib", "test", "tests", "spec", "specs", "export", "import",
];

/// Match a segment against known primitives
///
/// Returns (primitive, confidence, optional parameter, optional date format)
pub fn match_segment(segment: &str) -> (SemanticPrimitive, f32, Option<String>, Option<DateFormat>) {
    // Check for file (has extension)
    if PATTERNS.file_ext.is_match(segment) {
        return (SemanticPrimitive::Files, 1.0, None, None);
    }

    // Check dated hierarchy patterns
    if PATTERNS.iso_date.is_match(segment) {
        return (SemanticPrimitive::DatedHierarchy, 0.95, None, Some(DateFormat::Iso));
    }

    if PATTERNS.compact_date.is_match(segment) {
        return (SemanticPrimitive::DatedHierarchy, 0.85, None, Some(DateFormat::Compact));
    }

    if PATTERNS.year_month.is_match(segment) {
        return (SemanticPrimitive::DatedHierarchy, 0.90, None, Some(DateFormat::YearMonth));
    }

    // US vs European date: need context to distinguish
    // For now, assume ISO ordering month-first is US if month <= 12
    if PATTERNS.us_date.is_match(segment) || PATTERNS.eu_date.is_match(segment) {
        // Parse first two digits - if > 12, it's definitely day-first (European)
        if let Ok(first) = segment[..2].parse::<u32>() {
            if first > 12 {
                return (SemanticPrimitive::DatedHierarchy, 0.85, None, Some(DateFormat::European));
            }
        }
        // Ambiguous - assume US but lower confidence
        return (SemanticPrimitive::DatedHierarchy, 0.70, None, Some(DateFormat::Us));
    }

    // Year folder
    if PATTERNS.year_only.is_match(segment) {
        return (SemanticPrimitive::YearFolder, 0.90, None, None);
    }

    // Month folder
    if PATTERNS.month.is_match(segment) {
        return (SemanticPrimitive::MonthFolder, 0.85, None, None);
    }

    // Day folder
    if PATTERNS.day.is_match(segment) {
        return (SemanticPrimitive::DayFolder, 0.80, None, None);
    }

    // Version folder
    if PATTERNS.version.is_match(segment) {
        return (SemanticPrimitive::VersionFolder, 0.90, None, None);
    }

    // Category folder (exact match)
    let lower = segment.to_lowercase();
    if CATEGORY_FOLDERS.contains(&lower.as_str()) {
        return (SemanticPrimitive::CategoryFolder, 0.95, Some(lower), None);
    }

    // Entity folder (prefix_id pattern)
    if PATTERNS.entity.is_match(segment) {
        // Extract the prefix as the field name
        if let Some(idx) = segment.find(|c| c == '_' || c == '-') {
            let prefix = segment[..idx].to_lowercase();
            return (SemanticPrimitive::EntityFolder, 0.80, Some(prefix), None);
        }
    }

    // Unknown
    (SemanticPrimitive::Unknown, 0.0, None, None)
}

/// Check if multiple segments consistently match the same primitive
pub fn check_consistency(segments: &[&str]) -> f32 {
    if segments.is_empty() {
        return 0.0;
    }

    let matches: Vec<_> = segments.iter().map(|s| match_segment(s)).collect();

    // Check if all match the same primitive
    let first_primitive = matches[0].0;
    let all_same = matches.iter().all(|(p, _, _, _)| *p == first_primitive);

    if !all_same {
        // Not all the same primitive - low confidence
        return 0.3;
    }

    // Average confidence of all matches
    let avg_confidence: f32 = matches.iter().map(|(_, c, _, _)| c).sum::<f32>() / matches.len() as f32;

    // Boost if we have many consistent examples
    let count_bonus = (segments.len() as f32 / 10.0).min(0.1);

    (avg_confidence + count_bonus).min(1.0)
}

/// Generate a glob pattern for a segment based on its primitive
pub fn glob_pattern_for_primitive(primitive: SemanticPrimitive, date_format: Option<DateFormat>) -> &'static str {
    match primitive {
        SemanticPrimitive::DatedHierarchy => {
            match date_format {
                Some(DateFormat::Iso) => "????-??-??",
                Some(DateFormat::Compact) => "????????",
                Some(DateFormat::YearMonth) => "????-??",
                Some(DateFormat::Us) | Some(DateFormat::European) => "??-??-????",
                None => "*",
            }
        }
        SemanticPrimitive::YearFolder => "????",
        SemanticPrimitive::MonthFolder => "*",
        SemanticPrimitive::DayFolder => "*",
        SemanticPrimitive::EntityFolder => "*",
        SemanticPrimitive::CategoryFolder => "*",
        SemanticPrimitive::VersionFolder => "v*",
        SemanticPrimitive::Files => "*",
        SemanticPrimitive::Unknown => "*",
    }
}

/// Generate extraction regex for a segment
pub fn extraction_pattern_for_primitive(primitive: SemanticPrimitive, parameter: Option<&str>) -> Option<String> {
    match primitive {
        SemanticPrimitive::EntityFolder => {
            // Extract the ID part after the prefix
            if let Some(prefix) = parameter {
                Some(format!("{}[_-](.*)", regex::escape(prefix)))
            } else {
                Some(r"[a-zA-Z]+[_-](.*)".to_string())
            }
        }
        SemanticPrimitive::DatedHierarchy => None, // Use type: date instead
        SemanticPrimitive::YearFolder => Some(r"(?:FY)?(\d{4})".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iso_date_match() {
        let (primitive, conf, _, date_fmt) = match_segment("2024-01-15");
        assert_eq!(primitive, SemanticPrimitive::DatedHierarchy);
        assert!(conf > 0.9);
        assert_eq!(date_fmt, Some(DateFormat::Iso));
    }

    #[test]
    fn test_entity_folder_match() {
        let (primitive, conf, param, _) = match_segment("mission_042");
        assert_eq!(primitive, SemanticPrimitive::EntityFolder);
        assert!(conf > 0.7);
        assert_eq!(param, Some("mission".to_string()));
    }

    #[test]
    fn test_year_folder_match() {
        let (primitive, conf, _, _) = match_segment("2024");
        assert_eq!(primitive, SemanticPrimitive::YearFolder);
        assert!(conf > 0.8);
    }

    #[test]
    fn test_file_match() {
        let (primitive, conf, _, _) = match_segment("telemetry.csv");
        assert_eq!(primitive, SemanticPrimitive::Files);
        assert_eq!(conf, 1.0);
    }

    #[test]
    fn test_category_folder_match() {
        let (primitive, conf, param, _) = match_segment("reports");
        assert_eq!(primitive, SemanticPrimitive::CategoryFolder);
        assert!(conf > 0.9);
        assert_eq!(param, Some("reports".to_string()));
    }

    #[test]
    fn test_consistency_check() {
        let segments = vec!["2024-01-15", "2024-01-16", "2024-01-17"];
        let conf = check_consistency(&segments);
        assert!(conf > 0.9);
    }
}
