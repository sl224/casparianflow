//! Python Validator
//!
//! Validates generated Python code for syntax correctness and security.
//! Used when Pathfinder generates Python extractors for complex patterns.

use std::process::Command;

/// Result of Python code validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the code is valid
    pub is_valid: bool,
    /// Syntax errors found
    pub errors: Vec<String>,
    /// Security warnings
    pub warnings: Vec<String>,
    /// Parsed structure info (imports, classes, functions)
    pub structure: Option<CodeStructure>,
}

/// Parsed code structure
#[derive(Debug, Clone)]
pub struct CodeStructure {
    /// Import statements
    pub imports: Vec<String>,
    /// Class definitions
    pub classes: Vec<String>,
    /// Function definitions
    pub functions: Vec<String>,
    /// Has main guard
    pub has_main_guard: bool,
}

/// Python code validator
pub struct PythonValidator {
    /// Forbidden imports for security
    forbidden_imports: Vec<&'static str>,
    /// Forbidden function calls
    forbidden_calls: Vec<&'static str>,
}

impl PythonValidator {
    /// Create a new validator with default security rules
    pub fn new() -> Self {
        Self {
            forbidden_imports: vec![
                "os.system",
                "subprocess",
                "eval",
                "exec",
                "__import__",
                "importlib",
                "pickle",
                "shelve",
                "marshal",
                "socket",
                "http.server",
                "ftplib",
                "telnetlib",
                "smtplib",
            ],
            forbidden_calls: vec![
                "eval(",
                "exec(",
                "compile(",
                "open(", // Allow only with context manager
                "__import__(",
                "globals(",
                "locals(",
                "getattr(",
                "setattr(",
                "delattr(",
            ],
        }
    }

    /// Validate Python code
    pub fn validate(&self, code: &str) -> super::Result<ValidationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check syntax using Python
        if let Err(syntax_errors) = self.check_syntax(code) {
            errors.extend(syntax_errors);
        }

        // Security checks
        self.check_security(code, &mut errors, &mut warnings);

        // Parse structure
        let structure = self.parse_structure(code);

        // Validate extractor structure
        if let Some(ref s) = structure {
            self.validate_extractor_structure(s, &mut errors, &mut warnings);
        }

        let is_valid = errors.is_empty();

        Ok(ValidationResult {
            is_valid,
            errors,
            warnings,
            structure,
        })
    }

    /// Check Python syntax using the interpreter
    fn check_syntax(&self, code: &str) -> Result<(), Vec<String>> {
        // Use Python's compile() to check syntax
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
            Err(e) => {
                // Python not available - do basic checks
                Err(vec![format!("Could not run Python syntax check: {}", e)])
            }
        }
    }

    /// Check for security issues
    fn check_security(&self, code: &str, errors: &mut Vec<String>, warnings: &mut Vec<String>) {
        // Check forbidden imports
        for forbidden in &self.forbidden_imports {
            if code.contains(&format!("import {}", forbidden))
                || code.contains(&format!("from {} import", forbidden))
            {
                errors.push(format!("Forbidden import: {}", forbidden));
            }
        }

        // Check forbidden function calls
        for forbidden in &self.forbidden_calls {
            if code.contains(forbidden) {
                // Special case: allow open() with 'with' statement
                if *forbidden == "open(" {
                    let has_safe_open = code
                        .lines()
                        .any(|line| line.trim().starts_with("with") && line.contains("open("));
                    if !has_safe_open {
                        warnings.push(format!(
                            "Potentially unsafe call: {} - use 'with' statement for file operations",
                            forbidden
                        ));
                    }
                } else if *forbidden == "compile(" {
                    // Allow re.compile() but not standalone compile()
                    let has_unsafe_compile = code.lines().any(|line| {
                        let trimmed = line.trim();
                        trimmed.contains("compile(") && !trimmed.contains("re.compile(")
                    });
                    if has_unsafe_compile {
                        errors.push(format!("Forbidden call: {}", forbidden));
                    }
                } else {
                    errors.push(format!("Forbidden call: {}", forbidden));
                }
            }
        }

        // Check for shell execution patterns
        if code.contains("os.system") || code.contains("os.popen") {
            errors.push("Shell execution not allowed".to_string());
        }

        // Check for network operations
        if code.contains("urllib") || code.contains("requests") || code.contains("http.client") {
            warnings.push("Network operations detected - ensure this is intended".to_string());
        }
    }

    /// Parse code structure
    fn parse_structure(&self, code: &str) -> Option<CodeStructure> {
        let mut imports = Vec::new();
        let mut classes = Vec::new();
        let mut functions = Vec::new();
        let mut has_main_guard = false;

        for line in code.lines() {
            let trimmed = line.trim();

            // Imports
            if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                imports.push(trimmed.to_string());
            }

            // Classes
            if trimmed.starts_with("class ") {
                if let Some(name) = trimmed
                    .strip_prefix("class ")
                    .and_then(|s| s.split(&['(', ':'][..]).next())
                {
                    classes.push(name.trim().to_string());
                }
            }

            // Functions
            if trimmed.starts_with("def ") {
                if let Some(name) = trimmed
                    .strip_prefix("def ")
                    .and_then(|s| s.split('(').next())
                {
                    functions.push(name.trim().to_string());
                }
            }

            // Main guard
            if trimmed.contains("__name__") && trimmed.contains("__main__") {
                has_main_guard = true;
            }
        }

        Some(CodeStructure {
            imports,
            classes,
            functions,
            has_main_guard,
        })
    }

    /// Validate that code follows extractor conventions
    fn validate_extractor_structure(
        &self,
        structure: &CodeStructure,
        _errors: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) {
        // Check for expected imports
        let has_re = structure.imports.iter().any(|i| i.contains("re"));
        let has_pathlib = structure
            .imports
            .iter()
            .any(|i| i.contains("pathlib") || i.contains("Path"));

        if !has_re && !has_pathlib {
            warnings.push("Consider importing 're' or 'pathlib' for path extraction".to_string());
        }

        // Check for extract function
        let has_extract = structure.functions.iter().any(|f| f.contains("extract"));
        if !has_extract {
            warnings.push("Expected an 'extract' function for path extraction".to_string());
        }
    }
}

impl Default for PythonValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_code() {
        let validator = PythonValidator::new();

        let code = r#"
import re
from pathlib import Path

def extract(path: str) -> dict:
    """Extract fields from path."""
    pattern = re.compile(r'/data/(\d{4})/(.+)')
    match = pattern.match(path)
    if match:
        return {
            'year': match.group(1),
            'filename': match.group(2),
        }
    return {}
"#;

        let result = validator.validate(code).unwrap();
        assert!(result.is_valid, "Expected valid code: {:?}", result.errors);
    }

    #[test]
    fn test_syntax_error() {
        let validator = PythonValidator::new();

        let code = r#"
def broken(
    print("missing closing paren"
"#;

        let result = validator.validate(code).unwrap();
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_forbidden_import() {
        let validator = PythonValidator::new();

        let code = r#"
import subprocess

def run_command():
    subprocess.run(['ls'])
"#;

        let result = validator.validate(code).unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("subprocess")));
    }

    #[test]
    fn test_forbidden_eval() {
        let validator = PythonValidator::new();

        let code = r#"
def dangerous():
    code = "print('hello')"
    eval(code)
"#;

        let result = validator.validate(code).unwrap();
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("eval")));
    }

    #[test]
    fn test_safe_open_with_context_manager() {
        let validator = PythonValidator::new();

        let code = r#"
def read_file(path):
    with open(path, 'r') as f:
        return f.read()
"#;

        let result = validator.validate(code).unwrap();
        // Should have a warning but not an error
        assert!(result.is_valid || result.errors.is_empty());
    }

    #[test]
    fn test_structure_parsing() {
        let validator = PythonValidator::new();

        let code = r#"
import re
from typing import Dict

class PathExtractor:
    def extract(self, path: str) -> Dict:
        pass

def helper():
    pass

if __name__ == "__main__":
    print("test")
"#;

        let result = validator.validate(code).unwrap();
        let structure = result.structure.unwrap();

        assert!(structure.imports.len() >= 2);
        assert!(structure.classes.contains(&"PathExtractor".to_string()));
        assert!(structure.functions.contains(&"extract".to_string()));
        assert!(structure.functions.contains(&"helper".to_string()));
        assert!(structure.has_main_guard);
    }
}
