//! Semantic structure detection from file paths
//!
//! Analyzes a set of paths to detect common semantic structure.

use super::{
    primitives::{check_consistency, extraction_pattern_for_primitive, glob_pattern_for_primitive, match_segment},
    DetectedSegment, ExtractableField, SemanticPathResult, SemanticPrimitive,
};
use std::path::Path;

/// Minimum number of paths needed for reliable detection
const MIN_PATHS: usize = 2;

/// Detect semantic structure from a set of paths
pub fn detect_semantic_structure(paths: &[&Path]) -> Option<SemanticPathResult> {
    if paths.len() < MIN_PATHS {
        return None;
    }

    // Split all paths into segments
    let all_segments: Vec<Vec<&str>> = paths
        .iter()
        .filter_map(|p| {
            let components: Vec<&str> = p.iter()
                .filter_map(|c| c.to_str())
                .collect();
            if components.is_empty() { None } else { Some(components) }
        })
        .collect();

    if all_segments.is_empty() {
        return None;
    }

    // Find common depth (number of segments from end that are consistent)
    let min_depth = all_segments.iter().map(|s| s.len()).min().unwrap_or(0);
    if min_depth == 0 {
        return None;
    }

    // Analyze each segment position from the end
    let mut detected_segments = Vec::new();
    let mut total_confidence = 0.0;
    let mut segment_count = 0;

    for depth in 1..=min_depth {
        let position = -(depth as i32);

        // Collect all values at this position
        let values_at_position: Vec<&str> = all_segments
            .iter()
            .filter_map(|s| s.get(s.len() - depth))
            .copied()
            .collect();

        if values_at_position.is_empty() {
            continue;
        }

        // Check consistency across all paths at this position
        let consistency = check_consistency(&values_at_position);

        // Get the primitive match for the first example
        let (primitive, conf, parameter, date_format) = match_segment(values_at_position[0]);

        // Combined confidence
        let combined_conf = (conf * 0.6 + consistency * 0.4).min(1.0);

        let detected = DetectedSegment {
            position,
            primitive,
            confidence: combined_conf,
            parameter: parameter.clone(),
            date_format,
            examples: values_at_position.iter().take(5).map(|s| s.to_string()).collect(),
            pattern: extraction_pattern_for_primitive(primitive, parameter.as_deref()),
        };

        total_confidence += combined_conf;
        segment_count += 1;
        detected_segments.push(detected);
    }

    if detected_segments.is_empty() {
        return None;
    }

    // Reverse to get root-to-leaf order
    detected_segments.reverse();

    // Calculate overall confidence
    let avg_confidence = total_confidence / segment_count as f32;

    // Generate expression string
    let expression = generate_expression(&detected_segments);

    // Generate glob pattern
    let glob_pattern = generate_glob_pattern(&detected_segments);

    // Extract fields
    let extractable_fields = generate_extractable_fields(&detected_segments);

    // Suggest tag name based on detected entities
    let suggested_tag = suggest_tag_name(&detected_segments);

    Some(SemanticPathResult {
        segments: detected_segments,
        confidence: avg_confidence,
        expression,
        glob_pattern,
        suggested_tag,
        extractable_fields,
    })
}

/// Generate semantic expression string (e.g., "entity_folder(mission) > dated_hierarchy(iso) > files")
fn generate_expression(segments: &[DetectedSegment]) -> String {
    segments
        .iter()
        .map(|s| {
            let name = s.primitive.expr_name();
            match (&s.parameter, &s.date_format) {
                (Some(param), _) => format!("{}({})", name, param),
                (_, Some(df)) => format!("{}({})", name, df.as_str()),
                _ => name.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" > ")
}

/// Generate glob pattern for matching files
fn generate_glob_pattern(segments: &[DetectedSegment]) -> String {
    let patterns: Vec<&str> = segments
        .iter()
        .map(|s| glob_pattern_for_primitive(s.primitive, s.date_format))
        .collect();

    // Join with "/" and prefix with "**/"
    format!("**/{}", patterns.join("/"))
}

/// Generate extractable fields from detected segments
fn generate_extractable_fields(segments: &[DetectedSegment]) -> Vec<ExtractableField> {
    let mut fields = Vec::new();

    for segment in segments {
        match segment.primitive {
            SemanticPrimitive::EntityFolder => {
                let field_name = segment.parameter.clone()
                    .map(|p| format!("{}_id", p))
                    .unwrap_or_else(|| "entity_id".to_string());

                fields.push(ExtractableField {
                    name: field_name,
                    from_segment: segment.position,
                    pattern: segment.pattern.clone(),
                    data_type: Some("string".to_string()),
                });
            }
            SemanticPrimitive::DatedHierarchy => {
                fields.push(ExtractableField {
                    name: "date".to_string(),
                    from_segment: segment.position,
                    pattern: None,
                    data_type: Some(segment.date_format.map(|f| f.as_str()).unwrap_or("date").to_string()),
                });
            }
            SemanticPrimitive::YearFolder => {
                fields.push(ExtractableField {
                    name: "year".to_string(),
                    from_segment: segment.position,
                    pattern: segment.pattern.clone(),
                    data_type: Some("integer".to_string()),
                });
            }
            SemanticPrimitive::MonthFolder => {
                fields.push(ExtractableField {
                    name: "month".to_string(),
                    from_segment: segment.position,
                    pattern: None,
                    data_type: Some("string".to_string()),
                });
            }
            SemanticPrimitive::VersionFolder => {
                fields.push(ExtractableField {
                    name: "version".to_string(),
                    from_segment: segment.position,
                    pattern: Some(r"v?(\d+(?:\.\d+)*)".to_string()),
                    data_type: Some("string".to_string()),
                });
            }
            _ => {} // No extraction for Files, Category, etc.
        }
    }

    fields
}

/// Suggest a tag name based on detected entities
fn suggest_tag_name(segments: &[DetectedSegment]) -> Option<String> {
    // Look for entity folder with parameter
    for segment in segments {
        if segment.primitive == SemanticPrimitive::EntityFolder {
            if let Some(ref param) = segment.parameter {
                return Some(format!("{}_data", param));
            }
        }
    }

    // Look for category folder
    for segment in segments {
        if segment.primitive == SemanticPrimitive::CategoryFolder {
            if let Some(ref param) = segment.parameter {
                return Some(param.clone());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_mission_structure() {
        let paths: Vec<PathBuf> = vec![
            PathBuf::from("/data/mission_042/2024-01-15/telemetry.csv"),
            PathBuf::from("/data/mission_043/2024-01-16/readings.csv"),
            PathBuf::from("/data/mission_044/2024-01-17/sensor_log.csv"),
        ];
        let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();

        let result = detect_semantic_structure(&path_refs).unwrap();

        // Should detect: entity_folder(mission) > dated_hierarchy(iso) > files
        assert!(result.expression.contains("entity_folder(mission)"));
        assert!(result.expression.contains("dated_hierarchy(iso)"));
        assert!(result.expression.contains("files"));

        // Should have good confidence
        assert!(result.confidence > 0.7);

        // Should extract mission_id and date
        assert!(result.extractable_fields.iter().any(|f| f.name == "mission_id"));
        assert!(result.extractable_fields.iter().any(|f| f.name == "date"));
    }

    #[test]
    fn test_detect_year_structure() {
        let paths: Vec<PathBuf> = vec![
            PathBuf::from("/archive/2023/reports/q1.pdf"),
            PathBuf::from("/archive/2024/reports/q2.pdf"),
        ];
        let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();

        let result = detect_semantic_structure(&path_refs).unwrap();

        // Should detect year folder
        assert!(result.expression.contains("year_folder"));
    }

    #[test]
    fn test_glob_pattern_generation() {
        let paths: Vec<PathBuf> = vec![
            PathBuf::from("/data/client_abc/2024-01-15/invoice.csv"),
            PathBuf::from("/data/client_xyz/2024-01-16/invoice.csv"),
        ];
        let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();

        let result = detect_semantic_structure(&path_refs).unwrap();

        // Glob should use wildcards for entity and date
        assert!(result.glob_pattern.contains("*"));
        assert!(result.glob_pattern.contains("????-??-??"));
    }
}
