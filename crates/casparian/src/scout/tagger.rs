//! File tagging based on patterns
//!
//! Matches files to tagging rules based on glob patterns.
//! Returns the tag to assign to each file.

use super::error::{Result, ScoutError};
use super::types::{ScannedFile, TaggingRule};
use glob::Pattern;

/// Compiled tagging rule for efficient matching
#[allow(dead_code)] // Used in tests
struct CompiledRule {
    rule: TaggingRule,
    pattern: Pattern,
}

/// Tagger that matches files to tags based on patterns
#[allow(dead_code)] // Used in tests
pub struct Tagger {
    rules: Vec<CompiledRule>,
}

#[allow(dead_code)] // Used in tests
impl Tagger {
    /// Create a new tagger with the given rules
    /// Rules should be pre-sorted by priority (highest first)
    pub fn new(rules: Vec<TaggingRule>) -> Result<Self> {
        let compiled: Result<Vec<CompiledRule>> = rules
            .into_iter()
            .filter(|r| r.enabled)
            .map(|rule| {
                let pattern = Pattern::new(&rule.pattern)
                    .map_err(|e| ScoutError::Pattern(format!("{}: {}", rule.pattern, e)))?;
                Ok(CompiledRule { rule, pattern })
            })
            .collect();

        Ok(Self { rules: compiled? })
    }

    /// Find the tag for a file based on matching rules
    /// Returns the first matching rule's tag (rules should be priority-ordered)
    pub fn get_tag(&self, file: &ScannedFile) -> Option<&str> {
        self.rules
            .iter()
            .find(|cr| {
                cr.rule.source_id == file.source_id && cr.pattern.matches(&file.rel_path)
            })
            .map(|cr| cr.rule.tag.as_str())
    }

    /// Find the tag and rule ID for a file based on matching rules
    /// Returns (tag, rule_id) for the first matching rule
    pub fn get_tag_with_rule_id(&self, file: &ScannedFile) -> Option<(&str, &str)> {
        self.rules
            .iter()
            .find(|cr| {
                cr.rule.source_id == file.source_id && cr.pattern.matches(&file.rel_path)
            })
            .map(|cr| (cr.rule.tag.as_str(), cr.rule.id.as_str()))
    }

    /// Find all matching rules for a file
    pub fn match_file(&self, file: &ScannedFile) -> Vec<&TaggingRule> {
        self.rules
            .iter()
            .filter(|cr| {
                cr.rule.source_id == file.source_id && cr.pattern.matches(&file.rel_path)
            })
            .map(|cr| &cr.rule)
            .collect()
    }

    /// Check if any rule matches a file
    pub fn has_match(&self, file: &ScannedFile) -> bool {
        self.rules.iter().any(|cr| {
            cr.rule.source_id == file.source_id && cr.pattern.matches(&file.rel_path)
        })
    }

    /// Get all rules
    pub fn rules(&self) -> impl Iterator<Item = &TaggingRule> {
        self.rules.iter().map(|cr| &cr.rule)
    }

    /// Get rules for a specific source
    pub fn rules_for_source<'a>(&'a self, source_id: &'a str) -> impl Iterator<Item = &'a TaggingRule> {
        self.rules
            .iter()
            .filter(move |cr| cr.rule.source_id == source_id)
            .map(|cr| &cr.rule)
    }

    /// Get unique tags for a source
    pub fn tags_for_source(&self, source_id: &str) -> Vec<&str> {
        let mut tags: Vec<&str> = self
            .rules
            .iter()
            .filter(|cr| cr.rule.source_id == source_id)
            .map(|cr| cr.rule.tag.as_str())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scout::types::ScannedFile;

    fn create_test_rule(id: &str, source_id: &str, pattern: &str, tag: &str, priority: i32) -> TaggingRule {
        TaggingRule {
            id: id.to_string(),
            name: id.to_string(),
            source_id: source_id.to_string(),
            pattern: pattern.to_string(),
            tag: tag.to_string(),
            priority,
            enabled: true,
        }
    }

    fn create_test_file(source_id: &str, rel_path: &str) -> ScannedFile {
        ScannedFile::new(source_id, &format!("/data/{}", rel_path), rel_path, 1000, 12345)
    }

    #[test]
    fn test_simple_pattern_match() {
        let rules = vec![create_test_rule("r1", "src-1", "*.csv", "csv_data", 10)];
        let tagger = Tagger::new(rules).unwrap();

        let file = create_test_file("src-1", "data.csv");
        assert_eq!(tagger.get_tag(&file), Some("csv_data"));
    }

    #[test]
    fn test_glob_star_pattern() {
        let rules = vec![create_test_rule("r1", "src-1", "**/*.csv", "csv_data", 10)];
        let tagger = Tagger::new(rules).unwrap();

        // Should match at root
        let file1 = create_test_file("src-1", "data.csv");
        assert_eq!(tagger.get_tag(&file1), Some("csv_data"));

        // Should match in subdirectory
        let file2 = create_test_file("src-1", "subdir/data.csv");
        assert_eq!(tagger.get_tag(&file2), Some("csv_data"));

        // Should match in nested subdirectory
        let file3 = create_test_file("src-1", "a/b/c/data.csv");
        assert_eq!(tagger.get_tag(&file3), Some("csv_data"));

        // Should not match non-csv
        let file4 = create_test_file("src-1", "data.json");
        assert!(tagger.get_tag(&file4).is_none());
    }

    #[test]
    fn test_source_filtering() {
        let rules = vec![
            create_test_rule("r1", "src-1", "*.csv", "src1_csv", 10),
            create_test_rule("r2", "src-2", "*.csv", "src2_csv", 10),
        ];
        let tagger = Tagger::new(rules).unwrap();

        // File from src-1 should get src1 tag
        let file1 = create_test_file("src-1", "data.csv");
        assert_eq!(tagger.get_tag(&file1), Some("src1_csv"));

        // File from src-2 should get src2 tag
        let file2 = create_test_file("src-2", "data.csv");
        assert_eq!(tagger.get_tag(&file2), Some("src2_csv"));
    }

    #[test]
    fn test_priority_order() {
        // Rules should be pre-sorted by priority (higher first)
        let rules = vec![
            create_test_rule("r1", "src-1", "data*.csv", "specific_data", 20),
            create_test_rule("r2", "src-1", "*.csv", "generic_csv", 10),
        ];
        let tagger = Tagger::new(rules).unwrap();

        // Should match higher priority rule first
        let file = create_test_file("src-1", "data_2024.csv");
        assert_eq!(tagger.get_tag(&file), Some("specific_data"));
    }

    #[test]
    fn test_no_match() {
        let rules = vec![create_test_rule("r1", "src-1", "*.csv", "csv_data", 10)];
        let tagger = Tagger::new(rules).unwrap();

        let file = create_test_file("src-1", "data.json");
        assert!(tagger.get_tag(&file).is_none());
    }

    #[test]
    fn test_disabled_rule_not_matched() {
        let mut rule = create_test_rule("r1", "src-1", "*.csv", "csv_data", 10);
        rule.enabled = false;
        let rules = vec![rule];
        let tagger = Tagger::new(rules).unwrap();

        let file = create_test_file("src-1", "data.csv");
        assert!(tagger.get_tag(&file).is_none());
    }

    #[test]
    fn test_tags_for_source() {
        let rules = vec![
            create_test_rule("r1", "src-1", "*.csv", "csv_data", 10),
            create_test_rule("r2", "src-1", "*.json", "json_data", 10),
            create_test_rule("r3", "src-1", "exports/*.csv", "csv_data", 20), // Same tag
            create_test_rule("r4", "src-2", "*.csv", "other_csv", 10),
        ];
        let tagger = Tagger::new(rules).unwrap();

        let tags = tagger.tags_for_source("src-1");
        assert_eq!(tags.len(), 2); // csv_data and json_data (deduplicated)
        assert!(tags.contains(&"csv_data"));
        assert!(tags.contains(&"json_data"));
    }

    #[test]
    fn test_invalid_pattern_error() {
        let rules = vec![create_test_rule("r1", "src-1", "[invalid", "tag", 10)];
        let result = Tagger::new(rules);
        assert!(result.is_err());
    }
}
