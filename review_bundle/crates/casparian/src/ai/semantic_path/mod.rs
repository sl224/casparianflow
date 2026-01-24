//! Semantic Path Wizard
//!
//! Recognizes folder structure patterns and generates extraction rules.
//!
//! Key concepts:
//! - Semantic primitives: `entity_folder`, `dated_hierarchy`, etc.
//! - Pre-detection algorithm: Fast algorithmic confidence scoring
//! - YAML rule generation: Declarative extraction rules

pub mod detector;
pub mod primitives;

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Recognized semantic primitive types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticPrimitive {
    /// Entity identifier folder (e.g., "mission_042", "client_abc")
    EntityFolder,
    /// Date-based hierarchy (ISO: YYYY-MM-DD, US: MM-DD-YYYY, etc.)
    DatedHierarchy,
    /// Year folder (e.g., "2024", "FY2024")
    YearFolder,
    /// Month folder (e.g., "01", "January", "Jan")
    MonthFolder,
    /// Day folder (e.g., "15", "Day15")
    DayFolder,
    /// Category folder (static values: "reports", "logs", "data")
    CategoryFolder,
    /// Version folder (e.g., "v1", "v2.3", "release-1.0")
    VersionFolder,
    /// Terminal files level
    Files,
    /// Unknown/custom segment
    Unknown,
}

impl SemanticPrimitive {
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            SemanticPrimitive::EntityFolder => "Entity identifier (e.g., mission_042, client_abc)",
            SemanticPrimitive::DatedHierarchy => "Date folder (YYYY-MM-DD, etc.)",
            SemanticPrimitive::YearFolder => "Year folder (2024, FY2024)",
            SemanticPrimitive::MonthFolder => "Month folder (01, January)",
            SemanticPrimitive::DayFolder => "Day folder (15, Day15)",
            SemanticPrimitive::CategoryFolder => "Category folder (reports, logs)",
            SemanticPrimitive::VersionFolder => "Version folder (v1, v2.3)",
            SemanticPrimitive::Files => "Terminal files",
            SemanticPrimitive::Unknown => "Unknown pattern",
        }
    }

    /// Get the expression name for YAML output
    pub fn expr_name(&self) -> &'static str {
        match self {
            SemanticPrimitive::EntityFolder => "entity_folder",
            SemanticPrimitive::DatedHierarchy => "dated_hierarchy",
            SemanticPrimitive::YearFolder => "year_folder",
            SemanticPrimitive::MonthFolder => "month_folder",
            SemanticPrimitive::DayFolder => "day_folder",
            SemanticPrimitive::CategoryFolder => "category_folder",
            SemanticPrimitive::VersionFolder => "version_folder",
            SemanticPrimitive::Files => "files",
            SemanticPrimitive::Unknown => "unknown",
        }
    }
}

/// Date format detected in dated hierarchy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    /// ISO 8601: YYYY-MM-DD
    Iso,
    /// US: MM-DD-YYYY
    Us,
    /// European: DD-MM-YYYY
    European,
    /// Year-Month: YYYY-MM
    YearMonth,
    /// Compact: YYYYMMDD
    Compact,
}

impl DateFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            DateFormat::Iso => "iso",
            DateFormat::Us => "us",
            DateFormat::European => "european",
            DateFormat::YearMonth => "year_month",
            DateFormat::Compact => "compact",
        }
    }
}

/// A detected segment in the path structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedSegment {
    /// Position from end (-1 = filename, -2 = parent, etc.)
    pub position: i32,
    /// Detected primitive type
    pub primitive: SemanticPrimitive,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Extracted parameter if applicable (e.g., field name for entity_folder)
    pub parameter: Option<String>,
    /// Date format if DatedHierarchy
    pub date_format: Option<DateFormat>,
    /// Example values seen
    pub examples: Vec<String>,
    /// Regex pattern for extraction
    pub pattern: Option<String>,
}

/// Result of semantic path analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticPathResult {
    /// Detected segments from root to leaf
    pub segments: Vec<DetectedSegment>,
    /// Overall confidence (0.0 - 1.0)
    pub confidence: f32,
    /// Generated semantic expression (e.g., "entity_folder(mission) > dated_hierarchy(iso) > files")
    pub expression: String,
    /// Generated glob pattern
    pub glob_pattern: String,
    /// Suggested tag name
    pub suggested_tag: Option<String>,
    /// Fields that can be extracted
    pub extractable_fields: Vec<ExtractableField>,
}

/// A field that can be extracted from the path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractableField {
    /// Field name (e.g., "mission_id", "date")
    pub name: String,
    /// Source segment position
    pub from_segment: i32,
    /// Extraction pattern (regex)
    pub pattern: Option<String>,
    /// Data type hint
    pub data_type: Option<String>,
}

/// Analyze paths and detect semantic structure
pub fn analyze_paths(paths: &[&Path]) -> Option<SemanticPathResult> {
    if paths.is_empty() {
        return None;
    }

    detector::detect_semantic_structure(paths)
}

/// Generate a YAML extraction rule from the analysis result
pub fn generate_yaml_rule(result: &SemanticPathResult, tag_name: &str) -> String {
    let mut yaml = String::new();

    yaml.push_str(&format!("name: \"{}_extraction_rule\"\n", tag_name));
    yaml.push_str(&format!("glob: \"{}\"\n", result.glob_pattern));
    yaml.push_str(&format!("tag: \"{}\"\n", tag_name));
    yaml.push_str("\n");

    if !result.extractable_fields.is_empty() {
        yaml.push_str("extract:\n");
        for field in &result.extractable_fields {
            yaml.push_str(&format!("  {}:\n", field.name));
            yaml.push_str(&format!("    from: segment({})\n", field.from_segment));
            if let Some(ref pattern) = field.pattern {
                yaml.push_str(&format!("    pattern: \"{}\"\n", pattern));
            }
            if let Some(ref dtype) = field.data_type {
                yaml.push_str(&format!("    type: {}\n", dtype));
            }
        }
    }

    yaml
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_semantic_primitive_descriptions() {
        assert!(!SemanticPrimitive::EntityFolder.description().is_empty());
        assert!(!SemanticPrimitive::DatedHierarchy.description().is_empty());
    }

    #[test]
    fn test_basic_analysis() {
        let paths: Vec<PathBuf> = vec![
            PathBuf::from("/data/mission_042/2024-01-15/telemetry.csv"),
            PathBuf::from("/data/mission_043/2024-01-16/readings.csv"),
            PathBuf::from("/data/mission_044/2024-01-17/sensor_log.csv"),
        ];
        let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();

        let result = analyze_paths(&path_refs);
        assert!(result.is_some());

        let result = result.unwrap();
        assert!(result.confidence > 0.5);
    }
}
