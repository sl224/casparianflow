//! Parser Validator
//!
//! Validates generated Python parser code for syntax correctness and
//! tests it against sample files.

use std::process::Command;

/// Result of parser validation
#[derive(Debug, Clone)]
pub struct ParserValidationResult {
    /// Whether the parser is valid
    pub is_valid: bool,
    /// Syntax errors
    pub syntax_errors: Vec<String>,
    /// Import errors
    pub import_errors: Vec<String>,
    /// Runtime errors from test execution
    pub runtime_errors: Vec<String>,
    /// Warnings
    pub warnings: Vec<String>,
    /// Test output (if successful)
    pub test_output: Option<String>,
    /// Row count from test
    pub row_count: Option<usize>,
}

/// Parser code validator
pub struct ParserValidator {
    /// Required imports for a valid parser
    required_imports: Vec<&'static str>,
    /// Required elements
    required_elements: Vec<&'static str>,
}

impl ParserValidator {
    /// Create a new validator
    pub fn new() -> Self {
        Self {
            required_imports: vec!["polars"],
            required_elements: vec!["TOPIC", "SINK", "def parse"],
        }
    }

    /// Validate parser syntax using Python
    pub fn validate_syntax(&self, code: &str) -> super::Result<ParserValidationResult> {
        let mut syntax_errors = Vec::new();
        let mut import_errors = Vec::new();
        let mut warnings = Vec::new();

        // Check Python syntax
        if let Err(errors) = self.check_python_syntax(code) {
            syntax_errors.extend(errors);
        }

        // Check required imports
        for import in &self.required_imports {
            if !code.contains(&format!("import {}", import))
                && !code.contains(&format!("import {} ", import))
            {
                import_errors.push(format!("Missing required import: {}", import));
            }
        }

        // Check required elements
        for element in &self.required_elements {
            if !code.contains(element) {
                warnings.push(format!("Missing recommended element: {}", element));
            }
        }

        // Check for forbidden patterns
        self.check_forbidden_patterns(code, &mut import_errors, &mut warnings);

        let is_valid = syntax_errors.is_empty() && import_errors.is_empty();

        Ok(ParserValidationResult {
            is_valid,
            syntax_errors,
            import_errors,
            runtime_errors: vec![],
            warnings,
            test_output: None,
            row_count: None,
        })
    }

    /// Validate parser against a sample file
    pub fn validate_against_file(
        &self,
        code: &str,
        sample_path: &str,
    ) -> super::Result<ParserValidationResult> {
        // First check syntax
        let mut result = self.validate_syntax(code)?;

        if !result.is_valid {
            return Ok(result);
        }

        // Try to run the parser against the sample file
        match self.run_parser_test(code, sample_path) {
            Ok((output, row_count)) => {
                result.test_output = Some(output);
                result.row_count = Some(row_count);
            }
            Err(errors) => {
                result.runtime_errors = errors;
                result.is_valid = false;
            }
        }

        Ok(result)
    }

    /// Check Python syntax using the interpreter
    fn check_python_syntax(&self, code: &str) -> Result<(), Vec<String>> {
        let python_check = format!(
            r#"
import sys
import ast
try:
    ast.parse('''{code}''')
    sys.exit(0)
except SyntaxError as e:
    print(f"{{e.lineno}}:{{e.offset}}: {{e.msg}}")
    sys.exit(1)
"#,
            code = code.replace("'''", r"\'\'\'")
        );

        let output = Command::new("python3")
            .arg("-c")
            .arg(&python_check)
            .output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&result.stdout);
                    let errors: Vec<String> = stderr
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(|l| format!("Syntax error: {}", l))
                        .collect();
                    Err(errors)
                }
            }
            Err(e) => Err(vec![format!("Could not run Python syntax check: {}", e)]),
        }
    }

    /// Check for forbidden patterns in the code
    fn check_forbidden_patterns(
        &self,
        code: &str,
        errors: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) {
        let forbidden_imports = [
            "subprocess",
            "os.system",
            "eval",
            "exec",
            "__import__",
            "pickle",
            "shelve",
        ];

        for pattern in forbidden_imports {
            if code.contains(&format!("import {}", pattern))
                || code.contains(&format!("from {} ", pattern))
            {
                errors.push(format!("Forbidden import: {}", pattern));
            }
        }

        // Check for eval/exec calls
        if code.contains("eval(") {
            errors.push("Forbidden: eval() call".to_string());
        }
        if code.contains("exec(") {
            errors.push("Forbidden: exec() call".to_string());
        }

        // Warnings for potentially unsafe patterns
        if code.contains("open(") && !code.contains("with open") {
            warnings.push("Consider using 'with open()' for file operations".to_string());
        }
    }

    /// Run the parser against a sample file
    fn run_parser_test(
        &self,
        code: &str,
        sample_path: &str,
    ) -> Result<(String, usize), Vec<String>> {
        // Create temporary test script with unique name
        let temp_dir = std::env::temp_dir();
        let script_name = format!("casparian_parser_test_{}.py", std::process::id());
        let script_path = temp_dir.join(&script_name);

        // Write the parser code and test harness
        let test_code = format!(
            r#"{}

# Test harness
import sys
class MockCtx:
    def __init__(self, path):
        self.input_path = path
        self.source_hash = "test"
        self.job_id = "test"

try:
    # Find the parser class
    parser_class = None
    for name, obj in list(globals().items()):
        if isinstance(obj, type) and hasattr(obj, 'parse') and hasattr(obj, 'name'):
            parser_class = obj
            break

    if parser_class is None:
        print("ERROR: No parser class found")
        sys.exit(1)

    parser = parser_class()
    ctx = MockCtx("{}")

    row_count = 0
    for topic, df in parser.parse(ctx):
        row_count = len(df)
        print(f"SUCCESS: Parsed {{row_count}} rows for topic '{{topic}}'")
        print(df.head(5))

    print(f"ROW_COUNT:{{row_count}}")
except Exception as e:
    print(f"ERROR: {{e}}")
    sys.exit(1)
"#,
            code,
            sample_path.replace('\\', "\\\\").replace('"', "\\\"")
        );

        std::fs::write(&script_path, test_code)
            .map_err(|e| vec![format!("Failed to write test script: {}", e)])?;

        // Run the test
        let output = Command::new("python3")
            .arg(&script_path)
            .output()
            .map_err(|e| vec![format!("Failed to run parser test: {}", e)])?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let mut errors = Vec::new();
            if stdout.contains("ERROR:") {
                errors.push(
                    stdout
                        .lines()
                        .find(|l| l.starts_with("ERROR:"))
                        .unwrap_or("Unknown error")
                        .to_string(),
                );
            }
            if !stderr.is_empty() {
                errors.push(stderr);
            }
            if errors.is_empty() {
                errors.push("Parser test failed with unknown error".to_string());
            }
            return Err(errors);
        }

        // Extract row count from output
        let row_count = stdout
            .lines()
            .find(|l| l.starts_with("ROW_COUNT:"))
            .and_then(|l| l.strip_prefix("ROW_COUNT:"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        // Cleanup temp file
        let _ = std::fs::remove_file(&script_path);

        Ok((stdout, row_count))
    }
}

impl Default for ParserValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_parser() {
        let validator = ParserValidator::new();

        let code = r#"
import polars as pl

TOPIC = "test"
SINK = "duckdb"

class TestParser:
    name = "test"
    version = "1.0.0"
    topics = ["test"]

    def parse(self, ctx):
        df = pl.read_csv(ctx.input_path)
        yield (TOPIC, df)
"#;

        let result = validator.validate_syntax(code).unwrap();
        assert!(
            result.is_valid,
            "Expected valid: {:?}",
            result.syntax_errors
        );
    }

    #[test]
    fn test_validate_syntax_error() {
        let validator = ParserValidator::new();

        let code = r#"
import polars as pl

def broken(
    print("missing paren"
"#;

        let result = validator.validate_syntax(code).unwrap();
        assert!(!result.is_valid);
        assert!(!result.syntax_errors.is_empty());
    }

    #[test]
    fn test_forbidden_import() {
        let validator = ParserValidator::new();

        let code = r#"
import polars as pl
import subprocess

TOPIC = "test"
SINK = "duckdb"

def parse(ctx):
    pass
"#;

        let result = validator.validate_syntax(code).unwrap();
        assert!(!result.is_valid);
        assert!(result
            .import_errors
            .iter()
            .any(|e| e.contains("subprocess")));
    }

    #[test]
    fn test_missing_polars_import() {
        let validator = ParserValidator::new();

        let code = r#"
TOPIC = "test"
SINK = "duckdb"

def parse(ctx):
    pass
"#;

        let result = validator.validate_syntax(code).unwrap();
        assert!(!result.is_valid);
        assert!(result.import_errors.iter().any(|e| e.contains("polars")));
    }
}
