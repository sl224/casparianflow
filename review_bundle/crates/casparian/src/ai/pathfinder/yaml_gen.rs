//! YAML Rule Generator
//!
//! Generates extraction rules in YAML format from detected path patterns.
//! This is the preferred output format as YAML rules are declarative and
//! don't require code execution.

use super::analyzer::{DateFormat, PathPattern, SegmentType, VariablePattern};
use super::{PathfinderError, Result};

/// A generated YAML extraction rule
#[derive(Debug, Clone)]
pub struct GeneratedRule {
    /// Rule name (derived from pattern)
    pub name: String,
    /// Glob pattern for matching files
    pub glob: String,
    /// Fields to extract
    pub extract: Vec<ExtractField>,
    /// Optional filter conditions
    pub filter: Option<String>,
    /// Tags to apply
    pub tags: Vec<String>,
}

impl GeneratedRule {
    /// Convert to YAML string
    pub fn to_yaml(&self) -> String {
        let mut yaml = String::new();

        yaml.push_str(&format!("name: {}\n", self.name));
        yaml.push_str(&format!("glob: \"{}\"\n", self.glob));

        if !self.extract.is_empty() {
            yaml.push_str("extract:\n");
            for field in &self.extract {
                yaml.push_str(&format!("  {}:\n", field.name));
                yaml.push_str(&format!("    segment: {}\n", field.segment));

                if let Some(ref pattern) = field.pattern {
                    yaml.push_str(&format!("    pattern: \"{}\"\n", pattern));
                }

                if let Some(ref transform) = field.transform {
                    yaml.push_str(&format!("    transform: {}\n", transform));
                }

                if let Some(ref default) = field.default_value {
                    yaml.push_str(&format!("    default: \"{}\"\n", default));
                }
            }
        }

        if let Some(ref filter) = self.filter {
            yaml.push_str(&format!("filter: \"{}\"\n", filter));
        }

        if !self.tags.is_empty() {
            yaml.push_str("tags:\n");
            for tag in &self.tags {
                yaml.push_str(&format!("  - {}\n", tag));
            }
        }

        yaml
    }
}

/// A field extraction definition
#[derive(Debug, Clone)]
pub struct ExtractField {
    /// Field name
    pub name: String,
    /// Segment index (0-indexed from path start)
    pub segment: usize,
    /// Optional regex pattern for extraction
    pub pattern: Option<String>,
    /// Optional transform (e.g., "parse_date", "lowercase")
    pub transform: Option<String>,
    /// Default value if extraction fails
    pub default_value: Option<String>,
}

/// YAML rule generator
pub struct YamlRuleGenerator {
    /// Whether to add comments to output
    add_comments: bool,
}

impl YamlRuleGenerator {
    /// Create a new generator
    pub fn new() -> Self {
        Self { add_comments: true }
    }

    /// Create without comments
    pub fn without_comments(mut self) -> Self {
        self.add_comments = false;
        self
    }

    /// Generate a YAML rule from a path pattern
    pub fn generate(&self, pattern: &PathPattern, hints: Option<&str>) -> Result<GeneratedRule> {
        if pattern.segments.is_empty() {
            return Err(PathfinderError::GenerationFailed(
                "Pattern has no segments".to_string(),
            ));
        }

        // Derive rule name from pattern
        let name = self.derive_rule_name(pattern, hints);

        // Generate extract fields
        let extract = self.generate_extract_fields(pattern);

        // Generate tags
        let tags = self.generate_tags(pattern, hints);

        Ok(GeneratedRule {
            name,
            glob: pattern.glob.clone(),
            extract,
            filter: None,
            tags,
        })
    }

    /// Derive a rule name from the pattern
    fn derive_rule_name(&self, pattern: &PathPattern, hints: Option<&str>) -> String {
        // If hint provided, use it as base
        if let Some(hint) = hints {
            let clean = hint
                .to_lowercase()
                .replace(' ', "_")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>();
            if !clean.is_empty() {
                return format!("{}_extractor", clean);
            }
        }

        // Otherwise derive from fixed segments
        let fixed_parts: Vec<&str> = pattern
            .segments
            .iter()
            .filter_map(|s| {
                if let SegmentType::Fixed(ref val) = s.segment_type {
                    Some(val.as_str())
                } else {
                    None
                }
            })
            .collect();

        if fixed_parts.is_empty() {
            "path_extractor".to_string()
        } else {
            format!("{}_extractor", fixed_parts.join("_"))
        }
    }

    /// Generate extract field definitions
    fn generate_extract_fields(&self, pattern: &PathPattern) -> Vec<ExtractField> {
        pattern
            .segments
            .iter()
            .filter_map(|segment| {
                let field_name = segment.field_name.as_ref()?;

                let (pattern_str, transform) = match &segment.segment_type {
                    SegmentType::Variable {
                        pattern_type,
                        examples: _,
                    } => match pattern_type {
                        VariablePattern::Date { format } => (
                            Some(format.regex().to_string()),
                            Some(self.date_transform(format)),
                        ),
                        VariablePattern::Year => (Some(r"\d{4}".to_string()), None),
                        VariablePattern::Month => (Some(r"\d{2}".to_string()), None),
                        VariablePattern::NumericId => (Some(r"\d+".to_string()), None),
                        VariablePattern::AlphanumericId => {
                            (Some(r"[a-zA-Z0-9]+".to_string()), None)
                        }
                        VariablePattern::EntityPrefix { prefix, separator } => {
                            let sep_escaped = if *separator == '-' { "-" } else { "_" };
                            (Some(format!("{}{}(.+)", prefix, sep_escaped)), None)
                        }
                        VariablePattern::FreeText => (None, None),
                    },
                    SegmentType::Filename { extension } => {
                        let pattern = extension
                            .as_ref()
                            .map(|ext| format!(r"(.+)\.{}", regex::escape(ext)));
                        (pattern, None)
                    }
                    _ => return None, // Fixed segments don't need extraction
                };

                Some(ExtractField {
                    name: field_name.clone(),
                    segment: segment.position,
                    pattern: pattern_str,
                    transform,
                    default_value: None,
                })
            })
            .collect()
    }

    /// Get the appropriate date transform function name
    fn date_transform(&self, format: &DateFormat) -> String {
        match format {
            DateFormat::IsoDate => "parse_iso_date".to_string(),
            DateFormat::SlashDate => "parse_slash_date".to_string(),
            DateFormat::CompactDate => "parse_compact_date".to_string(),
            DateFormat::EuropeanDate => "parse_european_date".to_string(),
            DateFormat::AmericanDate => "parse_american_date".to_string(),
            DateFormat::YearOnly => "parse_year".to_string(),
            DateFormat::YearMonth => "parse_year_month".to_string(),
        }
    }

    /// Generate tags from the pattern
    fn generate_tags(&self, pattern: &PathPattern, hints: Option<&str>) -> Vec<String> {
        let mut tags = Vec::new();

        // Add hint as a tag if provided
        if let Some(hint) = hints {
            let clean = hint
                .to_lowercase()
                .replace(' ', "_")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>();
            if !clean.is_empty() {
                tags.push(clean);
            }
        }

        // Add tags based on detected patterns
        for segment in &pattern.segments {
            if let SegmentType::Variable {
                pattern_type,
                examples: _,
            } = &segment.segment_type
            {
                match pattern_type {
                    VariablePattern::Date { .. } => {
                        if !tags.contains(&"dated".to_string()) {
                            tags.push("dated".to_string());
                        }
                    }
                    VariablePattern::EntityPrefix { prefix, .. } => {
                        let tag = prefix.to_lowercase();
                        if !tags.contains(&tag) {
                            tags.push(tag);
                        }
                    }
                    _ => {}
                }
            }
        }

        tags
    }
}

impl Default for YamlRuleGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::pathfinder::analyzer::PathAnalyzer;

    #[test]
    fn test_basic_yaml_generation() {
        let analyzer = PathAnalyzer::new();
        let generator = YamlRuleGenerator::new();

        let paths = vec![
            "/data/2024/report.csv".to_string(),
            "/data/2023/report.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();
        let rule = generator.generate(&pattern, None).unwrap();

        assert!(rule.glob.contains("*"));
        assert!(!rule.extract.is_empty());
    }

    #[test]
    fn test_rule_name_from_hint() {
        let analyzer = PathAnalyzer::new();
        let generator = YamlRuleGenerator::new();

        let paths = vec!["/data/2024/report.csv".to_string()];
        let pattern = analyzer.analyze(&paths).unwrap();
        let rule = generator.generate(&pattern, Some("sales reports")).unwrap();

        assert_eq!(rule.name, "sales_reports_extractor");
    }

    #[test]
    fn test_rule_name_from_fixed_segments() {
        let analyzer = PathAnalyzer::new();
        let generator = YamlRuleGenerator::new();

        let paths = vec![
            "/archive/reports/2024/data.csv".to_string(),
            "/archive/reports/2023/data.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();
        let rule = generator.generate(&pattern, None).unwrap();

        assert!(rule.name.contains("archive"));
        assert!(rule.name.contains("reports"));
    }

    #[test]
    fn test_date_field_extraction() {
        let analyzer = PathAnalyzer::new();
        let generator = YamlRuleGenerator::new();

        let paths = vec![
            "/data/2024-01-15/file.csv".to_string(),
            "/data/2024-02-20/file.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();
        let rule = generator.generate(&pattern, None).unwrap();

        let date_field = rule.extract.iter().find(|f| f.name == "date");
        assert!(date_field.is_some());
        assert!(date_field.unwrap().transform.is_some());
    }

    #[test]
    fn test_yaml_output_format() {
        let analyzer = PathAnalyzer::new();
        let generator = YamlRuleGenerator::new();

        let paths = vec![
            "/data/2024/file.csv".to_string(),
            "/data/2023/file.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();
        let rule = generator.generate(&pattern, Some("yearly data")).unwrap();
        let yaml = rule.to_yaml();

        assert!(yaml.contains("name:"));
        assert!(yaml.contains("glob:"));
        assert!(yaml.contains("extract:"));
        assert!(yaml.contains("tags:"));
    }

    #[test]
    fn test_entity_prefix_tags() {
        let analyzer = PathAnalyzer::new();
        let generator = YamlRuleGenerator::new();

        let paths = vec![
            "/projects/CLIENT-001/data.csv".to_string(),
            "/projects/CLIENT-002/data.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();
        let rule = generator.generate(&pattern, None).unwrap();

        // Should have "client" tag
        assert!(rule.tags.contains(&"client".to_string()));
    }
}
