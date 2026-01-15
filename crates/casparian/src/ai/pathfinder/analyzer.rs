//! Path Analyzer
//!
//! Detects patterns in file paths to enable automatic extraction rule generation.
//!
//! ## Pattern Detection
//!
//! The analyzer identifies:
//! - Fixed segments (constant strings)
//! - Variable segments (dates, IDs, names)
//! - Date patterns (YYYY-MM-DD, YYYY/MM/DD, etc.)
//! - Numeric sequences
//! - Entity prefixes (CLIENT-, MISSION_, etc.)

use regex::Regex;
use std::collections::HashSet;

/// A detected pattern in file paths
#[derive(Debug, Clone)]
pub struct PathPattern {
    /// The segments that make up the pattern
    pub segments: Vec<PatternSegment>,
    /// The glob pattern for matching
    pub glob: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Sample paths that match this pattern
    pub sample_paths: Vec<String>,
}

impl PathPattern {
    /// Extract field values from a path using this pattern
    pub fn extract(&self, path: &str) -> Vec<(String, String)> {
        let mut result = Vec::new();
        // Strip leading slash to match how analyze() processes paths
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();

        for (i, segment) in self.segments.iter().enumerate() {
            if let Some(part) = parts.get(i) {
                if let Some(ref field_name) = segment.field_name {
                    result.push((field_name.clone(), part.to_string()));
                }
            }
        }

        result
    }
}

/// A segment in a path pattern
#[derive(Debug, Clone)]
pub struct PatternSegment {
    /// The type of segment
    pub segment_type: SegmentType,
    /// Position in the path (0-indexed from root)
    pub position: usize,
    /// Field name if this is extractable
    pub field_name: Option<String>,
    /// Original values seen at this position
    pub seen_values: Vec<String>,
}

/// Type of pattern segment
#[derive(Debug, Clone, PartialEq)]
pub enum SegmentType {
    /// Fixed string (same in all paths)
    Fixed(String),
    /// Variable that matches a pattern
    Variable {
        /// Detected pattern type
        pattern_type: VariablePattern,
        /// Example values
        examples: Vec<String>,
    },
    /// Regex pattern for complex matching
    Regex(String),
    /// Filename (last segment)
    Filename {
        /// Extension if consistent
        extension: Option<String>,
    },
}

/// Types of variable patterns
#[derive(Debug, Clone, PartialEq)]
pub enum VariablePattern {
    /// Date in various formats
    Date { format: DateFormat },
    /// Numeric ID
    NumericId,
    /// Alphanumeric ID
    AlphanumericId,
    /// Entity with prefix (CLIENT-001, MISSION_alpha)
    EntityPrefix { prefix: String, separator: char },
    /// Year only
    Year,
    /// Month only
    Month,
    /// Free-form text
    FreeText,
}

/// Supported date formats
#[derive(Debug, Clone, PartialEq)]
pub enum DateFormat {
    /// YYYY-MM-DD
    IsoDate,
    /// YYYY/MM/DD
    SlashDate,
    /// YYYYMMDD
    CompactDate,
    /// DD-MM-YYYY
    EuropeanDate,
    /// MM-DD-YYYY
    AmericanDate,
    /// YYYY
    YearOnly,
    /// YYYY-MM
    YearMonth,
}

impl DateFormat {
    /// Get regex pattern for this format
    pub fn regex(&self) -> &'static str {
        match self {
            DateFormat::IsoDate => r"\d{4}-\d{2}-\d{2}",
            DateFormat::SlashDate => r"\d{4}/\d{2}/\d{2}",
            DateFormat::CompactDate => r"\d{8}",
            DateFormat::EuropeanDate => r"\d{2}-\d{2}-\d{4}",
            DateFormat::AmericanDate => r"\d{2}-\d{2}-\d{4}",
            DateFormat::YearOnly => r"\d{4}",
            DateFormat::YearMonth => r"\d{4}-\d{2}",
        }
    }

    /// Get strftime format string
    pub fn strftime(&self) -> &'static str {
        match self {
            DateFormat::IsoDate => "%Y-%m-%d",
            DateFormat::SlashDate => "%Y/%m/%d",
            DateFormat::CompactDate => "%Y%m%d",
            DateFormat::EuropeanDate => "%d-%m-%Y",
            DateFormat::AmericanDate => "%m-%d-%Y",
            DateFormat::YearOnly => "%Y",
            DateFormat::YearMonth => "%Y-%m",
        }
    }
}

/// Path pattern analyzer
pub struct PathAnalyzer {
    /// Regex patterns for detection
    date_patterns: Vec<(Regex, DateFormat)>,
    entity_prefix_pattern: Regex,
    numeric_pattern: Regex,
    #[allow(dead_code)]
    alphanum_pattern: Regex,
}

impl PathAnalyzer {
    /// Create a new path analyzer
    pub fn new() -> Self {
        Self {
            date_patterns: vec![
                (Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap(), DateFormat::IsoDate),
                (Regex::new(r"^\d{4}/\d{2}/\d{2}$").unwrap(), DateFormat::SlashDate),
                (Regex::new(r"^\d{8}$").unwrap(), DateFormat::CompactDate),
                (Regex::new(r"^\d{2}-\d{2}-\d{4}$").unwrap(), DateFormat::EuropeanDate),
                (Regex::new(r"^\d{4}-\d{2}$").unwrap(), DateFormat::YearMonth),
                // Note: YearOnly is NOT included here - we use VariablePattern::Year instead
                // for standalone 4-digit years to avoid confusion with date formats
            ],
            entity_prefix_pattern: Regex::new(r"^([A-Z]+)[-_](.+)$").unwrap(),
            numeric_pattern: Regex::new(r"^\d+$").unwrap(),
            alphanum_pattern: Regex::new(r"^[a-zA-Z0-9]+$").unwrap(),
        }
    }

    /// Analyze paths to detect patterns
    pub fn analyze(&self, paths: &[String]) -> super::Result<PathPattern> {
        if paths.is_empty() {
            return Err(super::PathfinderError::NoPaths);
        }

        // Split paths into segments
        let path_segments: Vec<Vec<&str>> = paths
            .iter()
            .map(|p| p.trim_start_matches('/').split('/').collect())
            .collect();

        // Find common structure
        let max_depth = path_segments.iter().map(|s| s.len()).max().unwrap_or(0);
        let _min_depth = path_segments.iter().map(|s| s.len()).min().unwrap_or(0);

        if max_depth == 0 {
            return Err(super::PathfinderError::NoPattern("Empty paths".to_string()));
        }

        // Analyze each position
        let mut segments = Vec::new();
        let mut glob_parts = Vec::new();

        for pos in 0..max_depth {
            let values: Vec<&str> = path_segments
                .iter()
                .filter_map(|s| s.get(pos).copied())
                .collect();

            if values.is_empty() {
                continue;
            }

            let segment = self.analyze_position(pos, &values, pos == max_depth - 1);

            // Build glob pattern
            let glob_part = match &segment.segment_type {
                SegmentType::Fixed(s) => s.clone(),
                SegmentType::Variable { .. } => "*".to_string(),
                SegmentType::Regex(_) => "*".to_string(),
                SegmentType::Filename { extension } => {
                    extension.as_ref()
                        .map(|e| format!("*.{}", e))
                        .unwrap_or_else(|| "*".to_string())
                }
            };
            glob_parts.push(glob_part);
            segments.push(segment);
        }

        // Calculate confidence
        let confidence = self.calculate_confidence(&segments, paths.len());

        Ok(PathPattern {
            segments,
            glob: glob_parts.join("/"),
            confidence,
            sample_paths: paths.iter().take(5).cloned().collect(),
        })
    }

    /// Analyze a single position across all paths
    fn analyze_position(&self, pos: usize, values: &[&str], is_filename: bool) -> PatternSegment {
        let unique_values: HashSet<&str> = values.iter().copied().collect();
        let seen_values: Vec<String> = unique_values.iter().take(10).map(|s| s.to_string()).collect();

        // If all values are the same, it's fixed
        if unique_values.len() == 1 {
            let value = unique_values.into_iter().next().unwrap();

            if is_filename {
                let extension = value.rsplit('.').next()
                    .filter(|e| e.len() <= 10 && !e.contains('/'))
                    .map(String::from);
                return PatternSegment {
                    segment_type: SegmentType::Filename { extension },
                    position: pos,
                    field_name: None,
                    seen_values,
                };
            }

            return PatternSegment {
                segment_type: SegmentType::Fixed(value.to_string()),
                position: pos,
                field_name: None,
                seen_values,
            };
        }

        // Check for date patterns
        if let Some((_, date_format)) = self.date_patterns.iter()
            .find(|(regex, _)| values.iter().all(|v| regex.is_match(v)))
        {
            let field_name = self.suggest_field_name_for_date(date_format);
            return PatternSegment {
                segment_type: SegmentType::Variable {
                    pattern_type: VariablePattern::Date { format: date_format.clone() },
                    examples: seen_values.clone(),
                },
                position: pos,
                field_name: Some(field_name),
                seen_values,
            };
        }

        // Check for entity prefix pattern
        if let Some(caps) = values.iter()
            .filter_map(|v| self.entity_prefix_pattern.captures(v))
            .next()
        {
            if let (Some(prefix), Some(_)) = (caps.get(1), caps.get(2)) {
                let prefix_str = prefix.as_str().to_string();
                // Verify most values match this pattern
                let matching = values.iter()
                    .filter(|v| v.starts_with(&format!("{}-", prefix_str)) ||
                               v.starts_with(&format!("{}_", prefix_str)))
                    .count();

                if matching as f64 / values.len() as f64 > 0.8 {
                    let separator = if values.iter().any(|v| v.contains('-')) { '-' } else { '_' };
                    return PatternSegment {
                        segment_type: SegmentType::Variable {
                            pattern_type: VariablePattern::EntityPrefix {
                                prefix: prefix_str.clone(),
                                separator,
                            },
                            examples: seen_values.clone(),
                        },
                        position: pos,
                        field_name: Some(prefix_str.to_lowercase()),
                        seen_values,
                    };
                }
            }
        }

        // Check for year-only (BEFORE numeric IDs to avoid false positives)
        if values.iter().all(|v| {
            v.len() == 4 && v.parse::<u32>().map(|y| y >= 1900 && y <= 2100).unwrap_or(false)
        }) {
            return PatternSegment {
                segment_type: SegmentType::Variable {
                    pattern_type: VariablePattern::Year,
                    examples: seen_values.clone(),
                },
                position: pos,
                field_name: Some("year".to_string()),
                seen_values,
            };
        }

        // Check for pure numeric IDs
        if values.iter().all(|v| self.numeric_pattern.is_match(v)) {
            return PatternSegment {
                segment_type: SegmentType::Variable {
                    pattern_type: VariablePattern::NumericId,
                    examples: seen_values.clone(),
                },
                position: pos,
                field_name: Some(format!("id_{}", pos)),
                seen_values,
            };
        }

        // Filename handling
        if is_filename {
            // Check for consistent extension
            let extensions: HashSet<&str> = values.iter()
                .filter_map(|v| v.rsplit('.').next())
                .collect();

            let extension = if extensions.len() == 1 {
                extensions.into_iter().next().map(String::from)
            } else {
                None
            };

            return PatternSegment {
                segment_type: SegmentType::Filename { extension },
                position: pos,
                field_name: Some("filename".to_string()),
                seen_values,
            };
        }

        // Default to free text
        PatternSegment {
            segment_type: SegmentType::Variable {
                pattern_type: VariablePattern::FreeText,
                examples: seen_values.clone(),
            },
            position: pos,
            field_name: Some(format!("field_{}", pos)),
            seen_values,
        }
    }

    /// Suggest a field name for a date pattern
    fn suggest_field_name_for_date(&self, format: &DateFormat) -> String {
        match format {
            DateFormat::YearOnly => "year".to_string(),
            DateFormat::YearMonth => "year_month".to_string(),
            _ => "date".to_string(),
        }
    }

    /// Calculate confidence score for the pattern
    fn calculate_confidence(&self, segments: &[PatternSegment], path_count: usize) -> f64 {
        if segments.is_empty() || path_count == 0 {
            return 0.0;
        }

        let mut score = 0.0;
        let total_weight = segments.len() as f64;

        for segment in segments {
            let segment_score = match &segment.segment_type {
                SegmentType::Fixed(_) => 1.0,
                SegmentType::Variable { pattern_type, .. } => {
                    match pattern_type {
                        VariablePattern::Date { .. } => 0.95,
                        VariablePattern::Year => 0.95,
                        VariablePattern::Month => 0.9,
                        VariablePattern::EntityPrefix { .. } => 0.9,
                        VariablePattern::NumericId => 0.85,
                        VariablePattern::AlphanumericId => 0.8,
                        VariablePattern::FreeText => 0.6,
                    }
                }
                SegmentType::Regex(_) => 0.7,
                SegmentType::Filename { extension } => {
                    if extension.is_some() { 0.9 } else { 0.7 }
                }
            };
            score += segment_score;
        }

        // Adjust for path count (more paths = more confidence)
        let count_factor = (path_count as f64).ln().min(3.0) / 3.0;
        let base_confidence = score / total_weight;

        (base_confidence * 0.7 + count_factor * 0.3).min(1.0)
    }
}

impl Default for PathAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_segment_detection() {
        let analyzer = PathAnalyzer::new();
        let paths = vec![
            "/data/reports/2024/file1.csv".to_string(),
            "/data/reports/2024/file2.csv".to_string(),
            "/data/reports/2024/file3.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();

        // "data" and "reports" should be fixed
        assert!(matches!(pattern.segments[0].segment_type, SegmentType::Fixed(ref s) if s == "data"));
        assert!(matches!(pattern.segments[1].segment_type, SegmentType::Fixed(ref s) if s == "reports"));
    }

    #[test]
    fn test_date_pattern_detection() {
        let analyzer = PathAnalyzer::new();
        let paths = vec![
            "/data/2024-01-15/file.csv".to_string(),
            "/data/2024-02-20/file.csv".to_string(),
            "/data/2024-03-25/file.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();

        // Second segment should be a date
        assert!(matches!(
            &pattern.segments[1].segment_type,
            SegmentType::Variable { pattern_type: VariablePattern::Date { format: DateFormat::IsoDate }, .. }
        ));
        assert_eq!(pattern.segments[1].field_name, Some("date".to_string()));
    }

    #[test]
    fn test_year_only_detection() {
        let analyzer = PathAnalyzer::new();
        let paths = vec![
            "/archive/2022/data.csv".to_string(),
            "/archive/2023/data.csv".to_string(),
            "/archive/2024/data.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();

        assert!(matches!(
            &pattern.segments[1].segment_type,
            SegmentType::Variable { pattern_type: VariablePattern::Year, .. }
        ));
        assert_eq!(pattern.segments[1].field_name, Some("year".to_string()));
    }

    #[test]
    fn test_entity_prefix_detection() {
        let analyzer = PathAnalyzer::new();
        let paths = vec![
            "/projects/CLIENT-001/report.csv".to_string(),
            "/projects/CLIENT-002/report.csv".to_string(),
            "/projects/CLIENT-003/report.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();

        assert!(matches!(
            &pattern.segments[1].segment_type,
            SegmentType::Variable {
                pattern_type: VariablePattern::EntityPrefix { prefix, separator: '-' },
                ..
            } if prefix == "CLIENT"
        ));
    }

    #[test]
    fn test_numeric_id_detection() {
        let analyzer = PathAnalyzer::new();
        let paths = vec![
            "/data/12345/file.csv".to_string(),
            "/data/67890/file.csv".to_string(),
            "/data/11111/file.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();

        assert!(matches!(
            &pattern.segments[1].segment_type,
            SegmentType::Variable { pattern_type: VariablePattern::NumericId, .. }
        ));
    }

    #[test]
    fn test_glob_generation() {
        let analyzer = PathAnalyzer::new();
        let paths = vec![
            "/data/2024/report.csv".to_string(),
            "/data/2023/report.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();

        // Glob should be "data/*/report.csv" or similar
        assert!(pattern.glob.contains("data"));
        assert!(pattern.glob.contains("*"));
    }

    #[test]
    fn test_extraction() {
        let analyzer = PathAnalyzer::new();
        let paths = vec![
            "/data/2024/report.csv".to_string(),
            "/data/2023/report.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();
        let extracted = pattern.extract("/data/2024/report.csv");

        // Should extract the year
        assert!(extracted.iter().any(|(name, val)| name == "year" && val == "2024"));
    }

    #[test]
    fn test_confidence_calculation() {
        let analyzer = PathAnalyzer::new();

        // More specific patterns should have higher confidence
        let specific_paths = vec![
            "/data/2024-01-01/CLIENT-001/report.csv".to_string(),
            "/data/2024-01-02/CLIENT-002/report.csv".to_string(),
            "/data/2024-01-03/CLIENT-003/report.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&specific_paths).unwrap();
        assert!(pattern.confidence > 0.7);
    }

    #[test]
    fn test_filename_extension_detection() {
        let analyzer = PathAnalyzer::new();
        let paths = vec![
            "/data/file1.csv".to_string(),
            "/data/file2.csv".to_string(),
            "/data/file3.csv".to_string(),
        ];

        let pattern = analyzer.analyze(&paths).unwrap();

        // Last segment should be filename with csv extension
        assert!(matches!(
            &pattern.segments.last().unwrap().segment_type,
            SegmentType::Filename { extension: Some(ext) } if ext == "csv"
        ));
    }
}
