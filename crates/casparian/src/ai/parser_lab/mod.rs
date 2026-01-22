//! Parser Lab Wizard
//!
//! Generates Python parsers from sample files. Analyzes file format, infers schema,
//! and produces polars-based parser code that conforms to the Bridge Protocol.
//!
//! ## Flow
//!
//! ```text
//! User selects file(s) → W → l (Parser Lab)
//!     │
//!     ▼
//! SampleReader.analyze(paths)
//!     │
//!     ├─ Detect format (CSV, JSON, etc.)
//!     ├─ Sample rows
//!     └─ Infer schema
//!     │
//!     ▼
//! ParserGenerator.generate(analysis, hints)
//!     │
//!     ├─ Generate Python parser code
//!     └─ Include Bridge Protocol constants
//!     │
//!     ▼
//! ParserValidator.validate(code, sample_file)
//!     │
//!     ├─ Syntax check
//!     ├─ Import validation
//!     └─ Test run against sample
//!     │
//!     ▼
//! Show Result
//!     ├─ [a] Approve → save to parsers/
//!     ├─ [h] Hint → regenerate with hint
//!     ├─ [t] Test → run against more files
//!     └─ [Esc] Cancel
//! ```

pub mod generator;
pub mod sample_reader;
pub mod validator;

pub use generator::{ParserGenerator, GeneratedParser, ParserOptions};
pub use sample_reader::{SampleReader, SampleAnalysis, FileFormat, ColumnInfo};
pub use validator::{ParserValidator, ParserValidationResult};

use crate::ai::types::{Draft, DraftContext, DraftType};
use crate::ai::draft::DraftManager;

/// Result of Parser Lab analysis and generation
#[derive(Debug, Clone)]
pub struct ParserLabResult {
    /// The sample analysis
    pub analysis: SampleAnalysis,
    /// Generated parser code
    pub parser_code: String,
    /// Suggested parser name
    pub parser_name: String,
    /// Parser complexity assessment
    pub complexity: Complexity,
    /// Columns with special handling (dates, etc.)
    pub special_columns: Vec<String>,
    /// Generation warnings
    pub warnings: Vec<String>,
    /// Validation result (if validated)
    pub validation: Option<ParserValidationResult>,
}

/// Parser complexity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Complexity {
    /// Simple flat structure (basic CSV/JSON)
    Simple,
    /// Moderate complexity (dates, nulls, type conversions)
    Moderate,
    /// Complex (nested JSON, custom parsing logic)
    Complex,
}

impl Complexity {
    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Complexity::Simple => "Simple flat structure",
            Complexity::Moderate => "Moderate (dates, nulls, conversions)",
            Complexity::Complex => "Complex (nested, custom logic)",
        }
    }
}

/// Error type for Parser Lab operations
#[derive(Debug, thiserror::Error)]
pub enum ParserLabError {
    #[error("No files provided")]
    NoFiles,

    #[error("Failed to read sample file: {0}")]
    ReadError(String),

    #[error("Unsupported file format: {0}")]
    UnsupportedFormat(String),

    #[error("Schema inference failed: {0}")]
    SchemaInferenceFailed(String),

    #[error("Generation failed: {0}")]
    GenerationFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Draft error: {0}")]
    DraftError(#[from] crate::ai::draft::DraftError),
}

/// Result type for Parser Lab operations
pub type Result<T> = std::result::Result<T, ParserLabError>;

/// The main Parser Lab wizard interface
pub struct ParserLab {
    sample_reader: SampleReader,
    generator: ParserGenerator,
    validator: ParserValidator,
}

impl ParserLab {
    /// Create a new Parser Lab wizard
    pub fn new() -> Self {
        Self {
            sample_reader: SampleReader::new(),
            generator: ParserGenerator::new(),
            validator: ParserValidator::new(),
        }
    }

    /// Analyze sample files and generate parser
    pub fn analyze(
        &self,
        paths: &[String],
        options: Option<ParserOptions>,
        hints: Option<&str>,
    ) -> Result<ParserLabResult> {
        if paths.is_empty() {
            return Err(ParserLabError::NoFiles);
        }

        // Step 1: Analyze the sample file(s)
        let analysis = self.sample_reader.analyze(paths)?;

        // Step 2: Generate parser code
        let (parser_code, parser_name, special_columns, warnings) =
            self.generator.generate(&analysis, options.unwrap_or_default(), hints)?;

        // Step 3: Assess complexity
        let complexity = self.assess_complexity(&analysis, &special_columns);

        // Step 4: Optional syntax validation
        let validation = match self.validator.validate_syntax(&parser_code) {
            Ok(result) => Some(result),
            Err(_) => None,
        };

        Ok(ParserLabResult {
            analysis,
            parser_code,
            parser_name,
            complexity,
            special_columns,
            warnings,
            validation,
        })
    }

    /// Validate parser against sample file
    pub fn validate(
        &self,
        result: &ParserLabResult,
        sample_path: &str,
    ) -> Result<ParserValidationResult> {
        self.validator
            .validate_against_file(&result.parser_code, sample_path)
            .map_err(|e| ParserLabError::ValidationFailed(e.to_string()))
    }

    /// Assess complexity based on analysis
    fn assess_complexity(&self, analysis: &SampleAnalysis, special_columns: &[String]) -> Complexity {
        let has_dates = special_columns.iter().any(|c| c.contains("date") || c.contains("time"));
        let has_nulls = analysis.columns.iter().any(|c| c.nullable);
        let column_count = analysis.columns.len();

        // Check for nested structures (JSON)
        let is_nested = matches!(analysis.format, FileFormat::Json | FileFormat::Ndjson)
            && analysis.columns.iter().any(|c| c.data_type.contains("struct") || c.data_type.contains("list"));

        if is_nested || column_count > 20 {
            Complexity::Complex
        } else if has_dates || has_nulls || column_count > 10 {
            Complexity::Moderate
        } else {
            Complexity::Simple
        }
    }

    /// Create a draft from the result
    pub fn create_draft(
        &self,
        result: &ParserLabResult,
        draft_manager: &DraftManager,
        context: DraftContext,
    ) -> Result<Draft> {
        let draft = draft_manager
            .create_draft(
                DraftType::Parser,
                &result.parser_code,
                context,
                None, // No model needed - rule-based generation
            )
            ?;

        Ok(draft)
    }
}

impl Default for ParserLab {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complexity_description() {
        assert_eq!(Complexity::Simple.description(), "Simple flat structure");
        assert_eq!(Complexity::Moderate.description(), "Moderate (dates, nulls, conversions)");
        assert_eq!(Complexity::Complex.description(), "Complex (nested, custom logic)");
    }
}
