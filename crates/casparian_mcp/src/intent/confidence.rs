//! Deterministic confidence scoring for the Intent Pipeline.
//!
//! All confidence is computed deterministically from evidence.
//! The agent may explain but not override.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::types::{
    Confidence, ConfidenceLabel, DirPrefixEvidence, ExtensionEvidence, PathField,
    SchemaIntentColumn, SemanticTokenEvidence, TagCollisionEvidence,
};

// ============================================================================
// Confidence Score
// ============================================================================

/// Confidence score with deterministic computation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceScore {
    pub score: f64,
    pub label: ConfidenceLabel,
    pub signals: Vec<ConfidenceSignal>,
}

impl ConfidenceScore {
    pub fn from_signals(signals: Vec<ConfidenceSignal>) -> Self {
        let score = compute_combined_score(&signals);
        Self {
            score,
            label: ConfidenceLabel::from_score(score),
            signals,
        }
    }

    pub fn to_confidence(&self) -> Confidence {
        Confidence {
            score: self.score,
            label: self.label,
            reasons: self.signals.iter().map(|s| s.description.clone()).collect(),
        }
    }
}

/// A single confidence signal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceSignal {
    pub name: String,
    pub weight: f64,
    pub value: f64, // 0.0 to 1.0
    pub description: String,
}

impl ConfidenceSignal {
    pub fn new(
        name: impl Into<String>,
        weight: f64,
        value: f64,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            weight,
            value: value.clamp(0.0, 1.0),
            description: description.into(),
        }
    }

    pub fn weighted_contribution(&self) -> f64 {
        self.weight * self.value
    }
}

/// Compute combined score from signals (weighted average)
fn compute_combined_score(signals: &[ConfidenceSignal]) -> f64 {
    if signals.is_empty() {
        return 0.0;
    }

    let total_weight: f64 = signals.iter().map(|s| s.weight).sum();
    if total_weight == 0.0 {
        return 0.0;
    }

    let weighted_sum: f64 = signals.iter().map(|s| s.weighted_contribution()).sum();
    weighted_sum / total_weight
}

// ============================================================================
// Selection Confidence (§9.1)
// ============================================================================

/// Compute selection confidence from evidence
pub fn compute_selection_confidence(
    dir_prefixes: &[DirPrefixEvidence],
    extensions: &[ExtensionEvidence],
    semantic_tokens: &[SemanticTokenEvidence],
    tag_collisions: &[TagCollisionEvidence],
    total_files: u64,
) -> ConfidenceScore {
    let mut signals = Vec::new();

    // Signal 1: Directory concentration (entropy-based)
    // High concentration = matches are in a few directories = higher confidence
    if !dir_prefixes.is_empty() {
        let entropy = compute_entropy(dir_prefixes.iter().map(|d| d.count as f64).collect());
        let max_entropy = (dir_prefixes.len() as f64).ln();
        let concentration = if max_entropy > 0.0 {
            1.0 - (entropy / max_entropy)
        } else {
            1.0
        };
        signals.push(ConfidenceSignal::new(
            "dir_concentration",
            0.25,
            concentration,
            format!(
                "Files concentrated in {} directories (entropy: {:.2})",
                dir_prefixes.len(),
                entropy
            ),
        ));
    }

    // Signal 2: Extension narrowness
    // Fewer extensions = more specific selection = higher confidence
    if !extensions.is_empty() {
        let narrowness = if extensions.len() == 1 {
            1.0
        } else if extensions.len() <= 3 {
            0.8
        } else if extensions.len() <= 5 {
            0.5
        } else {
            0.2
        };
        signals.push(ConfidenceSignal::new(
            "extension_narrowness",
            0.20,
            narrowness,
            format!("Selection covers {} file extension(s)", extensions.len()),
        ));
    }

    // Signal 3: Semantic token presence
    // Intent-related tokens in paths = higher confidence
    if !semantic_tokens.is_empty() {
        let token_coverage =
            semantic_tokens.iter().map(|t| t.count).sum::<u64>() as f64 / total_files.max(1) as f64;
        let value = token_coverage.min(1.0);
        signals.push(ConfidenceSignal::new(
            "semantic_tokens",
            0.30,
            value,
            format!("Semantic tokens found in {:.0}% of paths", value * 100.0),
        ));
    }

    // Signal 4: Tag collision rate (negative signal)
    // More collisions = lower confidence
    if !tag_collisions.is_empty() {
        let collision_rate =
            tag_collisions.iter().map(|c| c.count).sum::<u64>() as f64 / total_files.max(1) as f64;
        let value = 1.0 - collision_rate.min(1.0);
        signals.push(ConfidenceSignal::new(
            "tag_collision",
            0.25,
            value,
            format!(
                "Collision with existing tags: {:.0}% of files",
                (1.0 - value) * 100.0
            ),
        ));
    } else {
        signals.push(ConfidenceSignal::new(
            "tag_collision",
            0.25,
            1.0,
            "No collisions with existing tags".to_string(),
        ));
    }

    ConfidenceScore::from_signals(signals)
}

// ============================================================================
// Path Field Confidence (§9.2)
// ============================================================================

/// Compute path field confidence
pub fn compute_path_field_confidence(field: &PathField, total_files: u64) -> ConfidenceScore {
    let mut signals = Vec::new();

    // Signal 1: Pattern type strength
    // key=value is strongest, then stable segment, then regex
    let pattern_strength = match &field.pattern {
        super::types::PathFieldPattern::KeyValue { .. } => 1.0,
        super::types::PathFieldPattern::PartitionDir { .. } => 0.9,
        super::types::PathFieldPattern::SegmentPosition { .. } => 0.7,
        super::types::PathFieldPattern::Regex { .. } => 0.5,
    };
    signals.push(ConfidenceSignal::new(
        "pattern_type",
        0.30,
        pattern_strength,
        format!("Pattern type: {:?}", field.pattern),
    ));

    // Signal 2: Coverage
    let coverage = field.coverage.matched_files as f64 / total_files.max(1) as f64;
    signals.push(ConfidenceSignal::new(
        "coverage",
        0.35,
        coverage,
        format!(
            "Matched {}/{} files ({:.0}%)",
            field.coverage.matched_files,
            total_files,
            coverage * 100.0
        ),
    ));

    // Signal 3: Value format validation
    // If dtype is date/timestamp, check if examples parse correctly
    let format_valid = match field.dtype {
        super::types::PathFieldDtype::Date | super::types::PathFieldDtype::Timestamp => {
            // Check if examples look like valid dates
            let valid_count = field.examples.iter().filter(|e| looks_like_date(e)).count();
            valid_count as f64 / field.examples.len().max(1) as f64
        }
        super::types::PathFieldDtype::Int => {
            let valid_count = field
                .examples
                .iter()
                .filter(|e| e.parse::<i64>().is_ok())
                .count();
            valid_count as f64 / field.examples.len().max(1) as f64
        }
        super::types::PathFieldDtype::String => 1.0,
    };
    signals.push(ConfidenceSignal::new(
        "format_validation",
        0.20,
        format_valid,
        format!(
            "Value format validation: {:.0}% valid",
            format_valid * 100.0
        ),
    ));

    // Signal 4: Uniqueness (distinct values)
    let uniqueness = if field.examples.len() > 1 {
        let unique: HashSet<_> = field.examples.iter().collect();
        unique.len() as f64 / field.examples.len() as f64
    } else {
        0.5 // Single example, neutral
    };
    signals.push(ConfidenceSignal::new(
        "value_uniqueness",
        0.15,
        uniqueness,
        format!(
            "{} unique values in {} examples",
            field.examples.iter().collect::<HashSet<_>>().len(),
            field.examples.len()
        ),
    ));

    ConfidenceScore::from_signals(signals)
}

// ============================================================================
// Schema Confidence (§9.3)
// ============================================================================

/// Compute schema column confidence
pub fn compute_schema_column_confidence(column: &SchemaIntentColumn) -> ConfidenceScore {
    let mut signals = Vec::new();

    // Signal 1: Inference method
    let method_confidence = match column.inference.method {
        super::types::InferenceMethod::ConstraintElimination => 1.0,
        super::types::InferenceMethod::AmbiguousRequiresHuman => 0.3,
    };
    signals.push(ConfidenceSignal::new(
        "inference_method",
        0.35,
        method_confidence,
        format!("Inference method: {:?}", column.inference.method),
    ));

    // Signal 2: Single candidate vs multiple
    let candidate_clarity = if column.inference.candidates.len() == 1 {
        1.0
    } else if column.inference.candidates.len() <= 2 {
        0.6
    } else {
        0.3
    };
    signals.push(ConfidenceSignal::new(
        "candidate_clarity",
        0.25,
        candidate_clarity,
        format!("{} type candidate(s)", column.inference.candidates.len()),
    ));

    // Signal 3: Null rate (lower is better for type inference)
    let null_stability = 1.0 - column.inference.evidence.null_rate.min(1.0);
    signals.push(ConfidenceSignal::new(
        "null_stability",
        0.20,
        null_stability,
        format!(
            "Null rate: {:.1}%",
            column.inference.evidence.null_rate * 100.0
        ),
    ));

    // Signal 4: Format hits (higher is better)
    let format_confidence = (column.inference.evidence.format_hits as f64 / 1000.0).min(1.0);
    signals.push(ConfidenceSignal::new(
        "format_hits",
        0.20,
        format_confidence,
        format!("{} format hits", column.inference.evidence.format_hits),
    ));

    ConfidenceScore::from_signals(signals)
}

// ============================================================================
// Helpers
// ============================================================================

/// Compute Shannon entropy
fn compute_entropy(values: Vec<f64>) -> f64 {
    let total: f64 = values.iter().sum();
    if total == 0.0 {
        return 0.0;
    }

    values
        .iter()
        .filter(|&&v| v > 0.0)
        .map(|&v| {
            let p = v / total;
            -p * p.ln()
        })
        .sum()
}

/// Check if a string looks like a date
fn looks_like_date(s: &str) -> bool {
    // Simple heuristics
    if s.len() < 6 || s.len() > 25 {
        return false;
    }

    // Contains date-like separators
    let has_separators = s.contains('-') || s.contains('/') || s.contains('.');

    // Contains digits
    let digit_count = s.chars().filter(|c| c.is_ascii_digit()).count();

    has_separators && digit_count >= 4
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_score_from_signals() {
        let signals = vec![
            ConfidenceSignal::new("signal1", 1.0, 0.8, "High signal"),
            ConfidenceSignal::new("signal2", 1.0, 0.6, "Medium signal"),
        ];

        let score = ConfidenceScore::from_signals(signals);
        assert!((score.score - 0.7).abs() < 0.001); // (0.8 + 0.6) / 2
        assert_eq!(score.label, ConfidenceLabel::Med);
    }

    #[test]
    fn test_weighted_signals() {
        let signals = vec![
            ConfidenceSignal::new("high_weight", 2.0, 1.0, "Important"),
            ConfidenceSignal::new("low_weight", 1.0, 0.0, "Less important"),
        ];

        let score = ConfidenceScore::from_signals(signals);
        // (2.0 * 1.0 + 1.0 * 0.0) / (2.0 + 1.0) = 0.667
        assert!((score.score - 0.667).abs() < 0.01);
    }

    #[test]
    fn test_selection_confidence() {
        let dir_prefixes = vec![DirPrefixEvidence {
            prefix: "/data/sales".to_string(),
            count: 100,
        }];
        let extensions = vec![ExtensionEvidence {
            ext: ".csv".to_string(),
            count: 100,
        }];
        let semantic_tokens = vec![SemanticTokenEvidence {
            token: "sales".to_string(),
            count: 80,
        }];
        let tag_collisions = vec![];

        let confidence = compute_selection_confidence(
            &dir_prefixes,
            &extensions,
            &semantic_tokens,
            &tag_collisions,
            100,
        );

        // Should be high confidence with single directory, single extension, good semantic coverage
        assert!(confidence.score > 0.7);
        assert_eq!(confidence.label, ConfidenceLabel::High);
    }

    #[test]
    fn test_entropy() {
        // Uniform distribution has maximum entropy
        let uniform = compute_entropy(vec![25.0, 25.0, 25.0, 25.0]);

        // Concentrated distribution has lower entropy
        let concentrated = compute_entropy(vec![97.0, 1.0, 1.0, 1.0]);

        assert!(concentrated < uniform);
    }

    #[test]
    fn test_looks_like_date() {
        assert!(looks_like_date("2024-01-15"));
        assert!(looks_like_date("01/15/2024"));
        assert!(looks_like_date("2024.01.15"));
        assert!(!looks_like_date("hello"));
        assert!(!looks_like_date("12345"));
    }
}
