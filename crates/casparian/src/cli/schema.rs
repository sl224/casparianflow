//! Schema command - show parser schemas

use anyhow::{bail, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::output::print_table;
use crate::cli::run::ensure_dev_venv;

/// Arguments for the `schema` command
#[derive(Debug, Args)]
pub struct SchemaArgs {
    /// Parser name or path (e.g., fix or parsers/fix/fix_parser.py)
    pub parser: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct SchemaField {
    name: String,
    dtype: String,
    nullable: bool,
}

type SchemaMap = BTreeMap<String, Vec<SchemaField>>;

fn interpreter_for_venv(venv_path: &Path) -> PathBuf {
    if cfg!(windows) {
        venv_path.join("Scripts").join("python.exe")
    } else {
        venv_path.join("bin").join("python")
    }
}

fn resolve_parser_path(parser: &str) -> PathBuf {
    if parser == "fix" {
        PathBuf::from("parsers/fix/fix_parser.py")
    } else {
        PathBuf::from(parser)
    }
}

pub fn run(args: SchemaArgs) -> Result<()> {
    let parser_path = resolve_parser_path(&args.parser);
    if !parser_path.exists() {
        bail!("Parser not found: {}", parser_path.display());
    }

    let parser_source = std::fs::read_to_string(&parser_path)
        .with_context(|| format!("Failed to read parser: {}", parser_path.display()))?;

    let interpreter = if let Some(venv_path) = ensure_dev_venv(&parser_source)? {
        std::env::set_var("VIRTUAL_ENV", &venv_path);
        interpreter_for_venv(&venv_path)
    } else if let Ok(venv_path) = std::env::var("VIRTUAL_ENV") {
        interpreter_for_venv(Path::new(&venv_path))
    } else {
        PathBuf::from("python3")
    };

    let script = r#"
import importlib.util
import json
import os

parser_path = os.environ.get("SCHEMA_PARSER_PATH")
if not parser_path:
    raise ValueError("SCHEMA_PARSER_PATH not set")

spec = importlib.util.spec_from_file_location("parser", parser_path)
parser_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(parser_module)

if not hasattr(parser_module, "get_schemas"):
    raise ValueError("Parser does not define get_schemas()")

schemas = parser_module.get_schemas()

def schema_to_list(schema):
    return [
        {"name": field.name, "dtype": str(field.type), "nullable": field.nullable}
        for field in schema
    ]

output = {name: schema_to_list(schema) for name, schema in schemas.items()}
print(json.dumps(output))
"#;

    let output = Command::new(&interpreter)
        .arg("-c")
        .arg(script)
        .env("SCHEMA_PARSER_PATH", &parser_path)
        .output()
        .with_context(|| format!("Failed to run python: {}", interpreter.display()))?;

    if !output.status.success() {
        bail!(
            "schema command failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let schemas: SchemaMap = serde_json::from_str(&stdout)
        .context("Failed to parse schema JSON from parser")?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&schemas)?);
        return Ok(());
    }

    for (output_name, fields) in schemas {
        println!("Output: {}", output_name);
        let headers: &[&str] = &["column", "type", "nullable"];
        let rows: Vec<Vec<String>> = fields
            .into_iter()
            .map(|field| vec![field.name, field.dtype, field.nullable.to_string()])
            .collect();
        print_table(headers, rows);
        println!();
    }

    Ok(())
}
