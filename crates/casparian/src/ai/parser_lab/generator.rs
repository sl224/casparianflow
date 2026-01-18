//! Parser Code Generator
//!
//! Generates Python parser code from sample analysis.
//! Produces polars-based code conforming to the Bridge Protocol.

use super::sample_reader::{SampleAnalysis, FileFormat, ColumnInfo};
use super::ParserLabError;

/// Options for parser generation
#[derive(Debug, Clone)]
pub struct ParserOptions {
    /// Include error handling
    pub include_error_handling: bool,
    /// Include validation
    pub include_validation: bool,
    /// Parser version
    pub version: String,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            include_error_handling: true,
            include_validation: true,
            version: "1.0.0".to_string(),
        }
    }
}

/// Generated parser result
#[derive(Debug, Clone)]
pub struct GeneratedParser {
    /// Parser Python code
    pub code: String,
    /// Suggested parser name
    pub name: String,
    /// Lines of code
    pub lines_of_code: usize,
    /// Columns requiring special handling
    pub special_columns: Vec<String>,
    /// Generation warnings
    pub warnings: Vec<String>,
}

/// Parser code generator
pub struct ParserGenerator {
    /// Python indent string
    indent: String,
}

impl ParserGenerator {
    /// Create a new generator
    pub fn new() -> Self {
        Self {
            indent: "    ".to_string(),
        }
    }

    /// Generate parser code from analysis
    pub fn generate(
        &self,
        analysis: &SampleAnalysis,
        options: ParserOptions,
        hints: Option<&str>,
    ) -> super::Result<(String, String, Vec<String>, Vec<String>)> {
        if analysis.columns.is_empty() && analysis.format != FileFormat::Parquet {
            return Err(ParserLabError::SchemaInferenceFailed(
                "No columns detected in sample".to_string()
            ));
        }

        let parser_name = self.generate_parser_name(hints);
        let topic_name = self.sanitize_topic(&parser_name);
        let mut code = String::new();
        let mut special_columns = Vec::new();
        let mut warnings = Vec::new();

        // Imports
        code.push_str("import polars as pl\n");
        code.push_str("from pathlib import Path\n");
        code.push_str("\n");

        // Bridge Protocol constants
        code.push_str(&format!("TOPIC = \"{}\"\n", topic_name));
        // Parser class
        code.push_str(&format!("class {}:\n", self.to_class_name(&parser_name)));
        code.push_str(&format!("{}\"\"\"Parser for {} files.\"\"\"\n\n", self.indent, analysis.format));

        // Class attributes
        code.push_str(&format!("{}name = \"{}\"\n", self.indent, parser_name));
        code.push_str(&format!("{}version = \"{}\"\n", self.indent, options.version));
        code.push_str(&format!("{}topics = [\"{}\"]\n\n", self.indent, topic_name));

        // Parse method
        code.push_str(&format!("{}def parse(self, ctx):\n", self.indent));
        code.push_str(&format!("{}{}\"\"\"Parse a file and yield data.\"\"\"\n", self.indent, self.indent));
        code.push_str(&format!("{}{}file_path = ctx.input_path\n\n", self.indent, self.indent));

        // File reading
        let read_code = self.generate_read_code(analysis, &options);
        for line in read_code.lines() {
            code.push_str(&format!("{}{}{}\n", self.indent, self.indent, line));
        }
        code.push('\n');

        // Type conversions
        let (conversion_code, converted_cols) = self.generate_type_conversions(analysis);
        if !conversion_code.is_empty() {
            code.push_str(&format!("{}{}# Type conversions\n", self.indent, self.indent));
            for line in conversion_code.lines() {
                code.push_str(&format!("{}{}{}\n", self.indent, self.indent, line));
            }
            code.push('\n');
            special_columns.extend(converted_cols);
        }

        // Validation
        if options.include_validation {
            let validation_code = self.generate_validation(analysis);
            if !validation_code.is_empty() {
                code.push_str(&format!("{}{}# Validation\n", self.indent, self.indent));
                for line in validation_code.lines() {
                    code.push_str(&format!("{}{}{}\n", self.indent, self.indent, line));
                }
                code.push('\n');
            }
        }

        // Yield result
        code.push_str(&format!("{}{}yield (TOPIC, df)\n", self.indent, self.indent));
        code.push('\n');

        // Standalone test block
        code.push_str("\n");
        code.push_str("if __name__ == \"__main__\":\n");
        code.push_str(&format!("{}import sys\n", self.indent));
        code.push_str(&format!("{}if len(sys.argv) < 2:\n", self.indent));
        code.push_str(&format!("{}{}print(\"Usage: python parser.py <file>\")\n", self.indent, self.indent));
        code.push_str(&format!("{}{}sys.exit(1)\n", self.indent, self.indent));
        code.push_str(&format!("{}class MockCtx:\n", self.indent));
        code.push_str(&format!("{}{}def __init__(self, path):\n", self.indent, self.indent));
        code.push_str(&format!("{}{}{}self.input_path = path\n", self.indent, self.indent, self.indent));
        code.push_str(&format!("{}parser = {}()\n", self.indent, self.to_class_name(&parser_name)));
        code.push_str(&format!("{}for topic, df in parser.parse(MockCtx(sys.argv[1])):\n", self.indent));
        code.push_str(&format!("{}{}print(f\"Topic: {{topic}}\")\n", self.indent, self.indent));
        code.push_str(&format!("{}{}print(df)\n", self.indent, self.indent));

        // Warnings
        if analysis.columns.iter().all(|c| !c.nullable) {
            warnings.push("All columns are non-nullable - consider if this is correct".to_string());
        }

        Ok((code, parser_name, special_columns, warnings))
    }

    /// Generate parser name from hints or default
    fn generate_parser_name(&self, hints: Option<&str>) -> String {
        hints.map(|h| {
            h.to_lowercase()
                .replace(' ', "_")
                .replace('-', "_")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "data_parser".to_string())
    }

    /// Sanitize topic name
    fn sanitize_topic(&self, name: &str) -> String {
        name.to_lowercase()
            .replace(' ', "_")
            .replace('-', "_")
    }

    /// Convert to PascalCase class name
    fn to_class_name(&self, name: &str) -> String {
        name.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<String>()
    }

    /// Generate file reading code
    fn generate_read_code(&self, analysis: &SampleAnalysis, options: &ParserOptions) -> String {
        let mut code = String::new();

        match analysis.format {
            FileFormat::Csv => {
                let delimiter = analysis.delimiter.unwrap_or(',');
                if options.include_error_handling {
                    code.push_str("try:\n");
                    if delimiter == ',' {
                        code.push_str("    df = pl.read_csv(file_path)\n");
                    } else {
                        code.push_str(&format!("    df = pl.read_csv(file_path, separator=\"{}\")\n", delimiter));
                    }
                    code.push_str("except Exception as e:\n");
                    code.push_str("    raise ValueError(f\"Failed to read CSV: {e}\")\n");
                } else if delimiter == ',' {
                    code.push_str("df = pl.read_csv(file_path)\n");
                } else {
                    code.push_str(&format!("df = pl.read_csv(file_path, separator=\"{}\")\n", delimiter));
                }
            }
            FileFormat::Tsv => {
                if options.include_error_handling {
                    code.push_str("try:\n");
                    code.push_str("    df = pl.read_csv(file_path, separator=\"\\t\")\n");
                    code.push_str("except Exception as e:\n");
                    code.push_str("    raise ValueError(f\"Failed to read TSV: {e}\")\n");
                } else {
                    code.push_str("df = pl.read_csv(file_path, separator=\"\\t\")\n");
                }
            }
            FileFormat::Json => {
                if options.include_error_handling {
                    code.push_str("try:\n");
                    code.push_str("    df = pl.read_json(file_path)\n");
                    code.push_str("except Exception as e:\n");
                    code.push_str("    raise ValueError(f\"Failed to read JSON: {e}\")\n");
                } else {
                    code.push_str("df = pl.read_json(file_path)\n");
                }
            }
            FileFormat::Ndjson => {
                if options.include_error_handling {
                    code.push_str("try:\n");
                    code.push_str("    df = pl.read_ndjson(file_path)\n");
                    code.push_str("except Exception as e:\n");
                    code.push_str("    raise ValueError(f\"Failed to read NDJSON: {e}\")\n");
                } else {
                    code.push_str("df = pl.read_ndjson(file_path)\n");
                }
            }
            FileFormat::Parquet => {
                if options.include_error_handling {
                    code.push_str("try:\n");
                    code.push_str("    df = pl.read_parquet(file_path)\n");
                    code.push_str("except Exception as e:\n");
                    code.push_str("    raise ValueError(f\"Failed to read Parquet: {e}\")\n");
                } else {
                    code.push_str("df = pl.read_parquet(file_path)\n");
                }
            }
            FileFormat::Unknown => {
                code.push_str("# Unknown format - defaulting to CSV\n");
                code.push_str("df = pl.read_csv(file_path)\n");
            }
        }

        code
    }

    /// Generate type conversion code
    fn generate_type_conversions(&self, analysis: &SampleAnalysis) -> (String, Vec<String>) {
        let mut code = String::new();
        let mut special_columns = Vec::new();

        for col in &analysis.columns {
            match col.data_type.as_str() {
                "date" => {
                    special_columns.push(format!("{} (date)", col.name));
                    if let Some(ref format) = col.format {
                        code.push_str(&format!(
                            "df = df.with_columns(pl.col(\"{}\").str.strptime(pl.Date, \"{}\"))\n",
                            col.name, format
                        ));
                    } else {
                        code.push_str(&format!(
                            "df = df.with_columns(pl.col(\"{}\").str.strptime(pl.Date, \"%Y-%m-%d\"))\n",
                            col.name
                        ));
                    }
                }
                "timestamp" => {
                    special_columns.push(format!("{} (timestamp)", col.name));
                    if let Some(ref format) = col.format {
                        code.push_str(&format!(
                            "df = df.with_columns(pl.col(\"{}\").str.strptime(pl.Datetime, \"{}\"))\n",
                            col.name, format
                        ));
                    } else {
                        code.push_str(&format!(
                            "df = df.with_columns(pl.col(\"{}\").str.strptime(pl.Datetime, \"%Y-%m-%dT%H:%M:%S\"))\n",
                            col.name
                        ));
                    }
                }
                "boolean" => {
                    special_columns.push(format!("{} (boolean)", col.name));
                    code.push_str(&format!(
                        "df = df.with_columns(pl.col(\"{}\").cast(pl.Boolean))\n",
                        col.name
                    ));
                }
                "int64" => {
                    if col.nullable {
                        special_columns.push(format!("{} (nullable int)", col.name));
                        code.push_str(&format!(
                            "df = df.with_columns(pl.col(\"{}\").cast(pl.Int64, strict=False))\n",
                            col.name
                        ));
                    }
                }
                "float64" => {
                    if col.nullable {
                        special_columns.push(format!("{} (nullable float)", col.name));
                        code.push_str(&format!(
                            "df = df.with_columns(pl.col(\"{}\").cast(pl.Float64, strict=False))\n",
                            col.name
                        ));
                    }
                }
                _ => {}
            }
        }

        (code, special_columns)
    }

    /// Generate validation code
    fn generate_validation(&self, analysis: &SampleAnalysis) -> String {
        let mut code = String::new();

        let non_nullable: Vec<&ColumnInfo> = analysis.columns.iter()
            .filter(|c| !c.nullable)
            .collect();

        if !non_nullable.is_empty() {
            code.push_str("# Check for nulls in required columns\n");
            for col in non_nullable {
                code.push_str(&format!(
                    "null_count = df.select(pl.col(\"{}\").null_count()).item()\n",
                    col.name
                ));
                code.push_str(&format!(
                    "if null_count > 0:\n    raise ValueError(f\"Column '{}' has {{null_count}} null values\")\n",
                    col.name
                ));
            }
        }

        code
    }
}

impl Default for ParserGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::parser_lab::sample_reader::SampleReader;
    use tempfile::TempDir;

    #[test]
    fn test_generate_parser_name() {
        let gen = ParserGenerator::new();
        assert_eq!(gen.generate_parser_name(Some("Sales Data")), "sales_data");
        assert_eq!(gen.generate_parser_name(Some("my-parser")), "my_parser");
        assert_eq!(gen.generate_parser_name(None), "data_parser");
    }

    #[test]
    fn test_to_class_name() {
        let gen = ParserGenerator::new();
        assert_eq!(gen.to_class_name("sales_data"), "SalesData");
        assert_eq!(gen.to_class_name("my_parser"), "MyParser");
        assert_eq!(gen.to_class_name("data"), "Data");
    }

    #[test]
    fn test_generate_simple_csv_parser() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.csv");
        std::fs::write(&path, "name,age\nAlice,30\nBob,25").unwrap();

        let reader = SampleReader::new();
        let analysis = reader.analyze(&[path.to_string_lossy().to_string()]).unwrap();

        let gen = ParserGenerator::new();
        let (code, name, _, _) = gen.generate(&analysis, ParserOptions::default(), Some("test parser")).unwrap();

        assert!(code.contains("TOPIC = \"test_parser\""));
        assert!(code.contains("class TestParser:"));
        assert!(code.contains("def parse(self, ctx):"));
        assert!(code.contains("pl.read_csv"));
        assert_eq!(name, "test_parser");
    }
}
