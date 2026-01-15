//! Python Extractor Generator
//!
//! Uses LLM to generate Python extraction code for complex path patterns
//! that cannot be expressed in YAML.

use super::analyzer::{PathPattern, SegmentType, VariablePattern};
use super::{PathfinderError, Result};
use std::future::Future;
use std::pin::Pin;

/// Type alias for async LLM generation function
pub type LlmGenerateFn = Box<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = std::result::Result<String, String>> + Send>>
        + Send
        + Sync,
>;

/// Python code generator using LLM
pub struct PythonGenerator {
    /// Function to call the LLM
    generate_fn: LlmGenerateFn,
}

impl PythonGenerator {
    /// Create a new generator with an LLM generation function
    ///
    /// The function takes a prompt string and returns the generated Python code
    pub fn new(generate_fn: LlmGenerateFn) -> Self {
        Self { generate_fn }
    }

    /// Generate Python extraction code
    pub async fn generate(
        &self,
        paths: &[String],
        pattern: &PathPattern,
        hints: Option<&str>,
    ) -> Result<String> {
        let prompt = self.build_prompt(paths, pattern, hints);

        let result = (self.generate_fn)(prompt).await
            .map_err(|e| PathfinderError::LlmError(e))?;

        // Extract code block from response
        let code = self.extract_code_block(&result);

        Ok(code)
    }

    /// Build the prompt for the LLM
    fn build_prompt(&self, paths: &[String], pattern: &PathPattern, hints: Option<&str>) -> String {
        let mut prompt = String::new();

        prompt.push_str("Generate a Python function to extract metadata from file paths.\n\n");

        // Add sample paths
        prompt.push_str("Sample paths:\n");
        for path in paths.iter().take(10) {
            prompt.push_str(&format!("- {}\n", path));
        }
        prompt.push('\n');

        // Add detected pattern info
        prompt.push_str("Detected pattern segments:\n");
        for (i, segment) in pattern.segments.iter().enumerate() {
            let desc = match &segment.segment_type {
                SegmentType::Fixed(s) => format!("Fixed: \"{}\"", s),
                SegmentType::Variable { pattern_type, examples } => {
                    let type_desc = match pattern_type {
                        VariablePattern::Date { format } => format!("Date ({:?})", format),
                        VariablePattern::Year => "Year".to_string(),
                        VariablePattern::Month => "Month".to_string(),
                        VariablePattern::NumericId => "Numeric ID".to_string(),
                        VariablePattern::AlphanumericId => "Alphanumeric ID".to_string(),
                        VariablePattern::EntityPrefix { prefix, separator } => {
                            format!("Entity prefix ({}{}<id>)", prefix, separator)
                        }
                        VariablePattern::FreeText => "Free text".to_string(),
                    };
                    format!("Variable: {} (examples: {:?})", type_desc, examples.iter().take(3).collect::<Vec<_>>())
                }
                SegmentType::Regex(r) => format!("Regex: {}", r),
                SegmentType::Filename { extension } => {
                    format!("Filename (extension: {:?})", extension)
                }
            };
            if let Some(ref field_name) = segment.field_name {
                prompt.push_str(&format!("{}: {} -> field \"{}\"\n", i, desc, field_name));
            } else {
                prompt.push_str(&format!("{}: {}\n", i, desc));
            }
        }
        prompt.push('\n');

        // Add hints if provided
        if let Some(hint) = hints {
            prompt.push_str(&format!("User hints: {}\n\n", hint));
        }

        // Add requirements
        prompt.push_str("Requirements:\n");
        prompt.push_str("1. Create a function `extract(path: str) -> dict`\n");
        prompt.push_str("2. Return a dictionary with extracted fields\n");
        prompt.push_str("3. Use regex for pattern matching\n");
        prompt.push_str("4. Handle edge cases gracefully (return empty dict on failure)\n");
        prompt.push_str("5. Include type hints\n");
        prompt.push_str("6. Do not use external libraries besides `re` and `pathlib`\n");
        prompt.push('\n');

        prompt.push_str("Generate the Python code:\n");
        prompt.push_str("```python\n");

        prompt
    }

    /// Extract Python code block from LLM response
    fn extract_code_block(&self, response: &str) -> String {
        // Look for code block markers
        if let Some(start) = response.find("```python") {
            let code_start = start + "```python".len();
            if let Some(end) = response[code_start..].find("```") {
                return response[code_start..code_start + end].trim().to_string();
            }
        }

        // Look for generic code block
        if let Some(start) = response.find("```") {
            let code_start = start + 3;
            // Skip language identifier if present
            let code_start = if let Some(newline) = response[code_start..].find('\n') {
                code_start + newline + 1
            } else {
                code_start
            };
            if let Some(end) = response[code_start..].find("```") {
                return response[code_start..code_start + end].trim().to_string();
            }
        }

        // No code block found, try to extract just the function
        let lines: Vec<&str> = response.lines().collect();
        let mut in_function = false;
        let mut code_lines = Vec::new();
        let mut indent_level = 0;

        for line in lines {
            if line.trim().starts_with("def extract") || line.trim().starts_with("import ") || line.trim().starts_with("from ") {
                in_function = true;
            }

            if in_function {
                code_lines.push(line);

                // Track indentation to know when function ends
                if !line.trim().is_empty() {
                    let current_indent = line.len() - line.trim_start().len();
                    if line.trim().starts_with("def ") {
                        indent_level = current_indent;
                    } else if current_indent <= indent_level && !line.trim().is_empty() && code_lines.len() > 1 {
                        // Back to base indent, function probably ended
                        if !line.trim().starts_with("def ") && !line.trim().starts_with("import ") {
                            break;
                        }
                    }
                }
            }
        }

        code_lines.join("\n")
    }
}

/// Create a default template for Python extractors
pub fn default_template(fields: &[(String, String)]) -> String {
    let mut code = String::new();

    code.push_str("import re\n");
    code.push_str("from pathlib import Path\n");
    code.push_str("from typing import Dict, Optional\n\n");

    code.push_str("def extract(path: str) -> Dict[str, Optional[str]]:\n");
    code.push_str("    \"\"\"Extract metadata from file path.\"\"\"\n");
    code.push_str("    result = {}\n");
    code.push_str("    p = Path(path)\n");
    code.push_str("    parts = p.parts\n\n");

    for (name, _pattern) in fields {
        code.push_str(&format!("    result['{}'] = None  # TODO: implement extraction\n", name));
    }

    code.push_str("\n    return result\n");

    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_block_with_markers() {
        let gen = PythonGenerator::new(Box::new(|_| {
            Box::pin(async { Ok("".to_string()) })
        }));

        let response = r#"Here's the code:

```python
def extract(path: str) -> dict:
    return {}
```

That's it!"#;

        let code = gen.extract_code_block(response);
        assert!(code.contains("def extract"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn test_extract_code_block_without_markers() {
        let gen = PythonGenerator::new(Box::new(|_| {
            Box::pin(async { Ok("".to_string()) })
        }));

        let response = r#"import re

def extract(path: str) -> dict:
    return {}

print("done")"#;

        let code = gen.extract_code_block(response);
        assert!(code.contains("import re"));
        assert!(code.contains("def extract"));
    }

    #[test]
    fn test_default_template() {
        let fields = vec![
            ("year".to_string(), r"\d{4}".to_string()),
            ("client".to_string(), r"CLIENT-\w+".to_string()),
        ];

        let code = default_template(&fields);
        assert!(code.contains("def extract"));
        assert!(code.contains("result['year']"));
        assert!(code.contains("result['client']"));
    }
}
