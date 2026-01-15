//! Pathfinder Wizard
//!
//! Generates extraction rules from file paths. Primary output is YAML extraction
//! rules; Python extractors are only generated for complex logic that cannot be
//! expressed declaratively.
//!
//! ## Flow
//!
//! ```text
//! User selects file(s) → W → p (Pathfinder)
//!     │
//!     ▼
//! PathAnalyzer.analyze(paths)
//!     │
//!     ├─ YAML expressible? → YamlRuleGenerator → Show YAML result
//!     │                                              │
//!     │                                              ├─ [a] Approve → commit
//!     │                                              ├─ [h] Hint → regenerate
//!     │                                              └─ [Esc] Cancel
//!     │
//!     └─ Needs Python? → PythonGenerator (LLM) → Validator → Show result
//! ```

pub mod analyzer;
pub mod yaml_gen;
pub mod python_gen;
pub mod validator;

pub use analyzer::{PathAnalyzer, PathPattern, PatternSegment, SegmentType};
pub use yaml_gen::{YamlRuleGenerator, GeneratedRule};
pub use validator::{PythonValidator, ValidationResult};
pub use python_gen::PythonGenerator;

use crate::ai::types::{Draft, DraftContext, DraftType};
use crate::ai::draft::DraftManager;

/// Result of Pathfinder analysis
#[derive(Debug, Clone)]
pub struct PathfinderResult {
    /// The pattern detected in the paths
    pub pattern: PathPattern,
    /// Generated YAML rule (if expressible in YAML)
    pub yaml_rule: Option<GeneratedRule>,
    /// Generated Python code (if YAML insufficient)
    pub python_code: Option<String>,
    /// Reason for the output choice
    pub decision_reason: String,
    /// Complexity assessment
    pub complexity: Complexity,
    /// Preview of extracted values from sample paths
    pub preview: Vec<ExtractionPreview>,
}

/// Complexity level of the extraction pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Complexity {
    /// Simple glob + fixed extraction (YAML)
    Simple,
    /// Moderate regex patterns (YAML with regex)
    Moderate,
    /// Complex logic requiring Python
    Complex,
}

impl Complexity {
    /// Returns true if YAML can express this complexity
    pub fn is_yaml_expressible(&self) -> bool {
        matches!(self, Complexity::Simple | Complexity::Moderate)
    }
}

/// Preview of extracted values from a sample path
#[derive(Debug, Clone)]
pub struct ExtractionPreview {
    /// The source path
    pub path: String,
    /// Extracted field values
    pub fields: Vec<(String, String)>,
}

/// Error type for Pathfinder operations
#[derive(Debug, thiserror::Error)]
pub enum PathfinderError {
    #[error("No paths provided")]
    NoPaths,

    #[error("No pattern detected: {0}")]
    NoPattern(String),

    #[error("Generation failed: {0}")]
    GenerationFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("LLM error: {0}")]
    LlmError(String),

    #[error("Draft error: {0}")]
    DraftError(#[from] crate::ai::draft::DraftError),
}

/// Result type for Pathfinder operations
pub type Result<T> = std::result::Result<T, PathfinderError>;

/// The main Pathfinder wizard interface
pub struct Pathfinder {
    analyzer: PathAnalyzer,
    yaml_gen: YamlRuleGenerator,
    python_gen: Option<PythonGenerator>,
    #[allow(dead_code)]
    validator: PythonValidator,
}

impl Pathfinder {
    /// Create a new Pathfinder wizard
    pub fn new() -> Self {
        Self {
            analyzer: PathAnalyzer::new(),
            yaml_gen: YamlRuleGenerator::new(),
            python_gen: None,
            validator: PythonValidator::new(),
        }
    }

    /// Create with LLM support for Python generation
    ///
    /// The `generate_fn` is a callback that takes a prompt string and returns
    /// the LLM's response. This allows the caller to use any LLM provider.
    pub fn with_llm(mut self, generate_fn: python_gen::LlmGenerateFn) -> Self {
        self.python_gen = Some(PythonGenerator::new(generate_fn));
        self
    }

    /// Analyze paths and generate extraction rule
    pub async fn analyze(&self, paths: &[String], hints: Option<&str>) -> Result<PathfinderResult> {
        if paths.is_empty() {
            return Err(PathfinderError::NoPaths);
        }

        // Step 1: Analyze paths to detect pattern
        let pattern = self.analyzer.analyze(paths)?;

        // Step 2: Assess complexity
        let complexity = self.assess_complexity(&pattern);

        // Step 3: Generate appropriate output
        let (yaml_rule, python_code, decision_reason) = if complexity.is_yaml_expressible() {
            let rule = self.yaml_gen.generate(&pattern, hints)?;
            (Some(rule), None, "Pattern can be expressed in YAML".to_string())
        } else {
            if let Some(ref gen) = self.python_gen {
                let code = gen.generate(paths, &pattern, hints).await?;
                // Validate the generated Python
                let validation = self.validator.validate(&code)?;
                if !validation.is_valid {
                    return Err(PathfinderError::ValidationFailed(
                        validation.errors.join("; ")
                    ));
                }
                (None, Some(code), "Complex pattern requires Python".to_string())
            } else {
                return Err(PathfinderError::LlmError(
                    "LLM not configured for Python generation".to_string()
                ));
            }
        };

        // Step 4: Generate preview
        let preview = self.generate_preview(paths, &pattern);

        Ok(PathfinderResult {
            pattern,
            yaml_rule,
            python_code,
            decision_reason,
            complexity,
            preview,
        })
    }

    /// Assess the complexity of a pattern
    fn assess_complexity(&self, pattern: &PathPattern) -> Complexity {
        let mut has_regex = false;
        let mut regex_complexity = 0;

        for segment in &pattern.segments {
            match &segment.segment_type {
                SegmentType::Regex(r) => {
                    has_regex = true;
                    regex_complexity += r.len();
                }
                SegmentType::Variable { .. } => {
                    // Variables are fine in YAML
                }
                _ => {}
            }
        }

        // Thresholds from config
        if regex_complexity > 200 || pattern.segments.len() > 10 {
            Complexity::Complex
        } else if has_regex || regex_complexity > 50 {
            Complexity::Moderate
        } else {
            Complexity::Simple
        }
    }

    /// Generate extraction preview for sample paths
    fn generate_preview(&self, paths: &[String], pattern: &PathPattern) -> Vec<ExtractionPreview> {
        paths.iter().take(5).map(|path| {
            let fields = pattern.extract(path)
                .into_iter()
                .collect();
            ExtractionPreview {
                path: path.clone(),
                fields,
            }
        }).collect()
    }

    /// Create a draft from the result
    pub async fn create_draft(
        &self,
        result: &PathfinderResult,
        draft_manager: &DraftManager,
        context: DraftContext,
    ) -> Result<Draft> {
        let (draft_type, content) = if let Some(ref rule) = result.yaml_rule {
            (DraftType::Extractor, rule.to_yaml())
        } else if let Some(ref code) = result.python_code {
            (DraftType::Extractor, code.clone())
        } else {
            return Err(PathfinderError::GenerationFailed(
                "No output generated".to_string()
            ));
        };

        let draft = draft_manager
            .create_draft(draft_type, &content, context, Some("qwen2.5-coder-1.5b"))
            .await?;

        Ok(draft)
    }
}

impl Default for Pathfinder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complexity_yaml_expressible() {
        assert!(Complexity::Simple.is_yaml_expressible());
        assert!(Complexity::Moderate.is_yaml_expressible());
        assert!(!Complexity::Complex.is_yaml_expressible());
    }
}
