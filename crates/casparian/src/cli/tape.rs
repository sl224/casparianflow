//! Tape command for Casparian CLI
//!
//! Provides commands for working with session tape recordings:
//! - `explain` - Summarize what happened in a recorded session
//! - `validate` - Check tape format and schema version

use anyhow::{Context, Result};
use casparian_tape::{EnvelopeV1, EventName, SCHEMA_VERSION};
use clap::Subcommand;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum TapeCommands {
    /// Explain what happened in a recorded session
    Explain {
        /// Path to the tape file
        tape_file: PathBuf,

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Validate a tape file format
    Validate {
        /// Path to the tape file
        tape_file: PathBuf,
    },
}

/// Summary of a job from the tape
#[derive(Debug, Clone)]
struct JobSummary {
    job_id: String,
    plugin_name: Option<String>,
    status: String,
    rows: Option<u64>,
    outputs: Vec<String>,
    error: Option<String>,
}

/// Overall tape summary
#[derive(Debug)]
struct TapeSummary {
    event_count: usize,
    schema_version: u32,
    commands: Vec<String>,
    jobs: HashMap<String, JobSummary>,
    errors: Vec<String>,
    materializations: usize,
}

impl TapeSummary {
    fn new() -> Self {
        Self {
            event_count: 0,
            schema_version: 0,
            commands: Vec::new(),
            jobs: HashMap::new(),
            errors: Vec::new(),
            materializations: 0,
        }
    }
}

pub fn run_tape_command(cmd: TapeCommands) -> Result<()> {
    match cmd {
        TapeCommands::Explain { tape_file, format } => {
            explain_tape(&tape_file, &format)
        }
        TapeCommands::Validate { tape_file } => {
            validate_tape(&tape_file)
        }
    }
}

fn explain_tape(tape_file: &PathBuf, format: &str) -> Result<()> {
    let file = File::open(tape_file)
        .with_context(|| format!("Failed to open tape file: {}", tape_file.display()))?;
    let reader = BufReader::new(file);

    let mut summary = TapeSummary::new();

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.with_context(|| format!("Failed to read line {}", line_num + 1))?;

        if line.trim().is_empty() {
            continue;
        }

        let envelope: EnvelopeV1 = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse event at line {}", line_num + 1))?;

        summary.event_count += 1;
        summary.schema_version = envelope.schema_version;

        process_event(&envelope, &mut summary);
    }

    if format == "json" {
        print_summary_json(&summary)?;
    } else {
        print_summary_text(&summary);
    }

    Ok(())
}

fn process_event(envelope: &EnvelopeV1, summary: &mut TapeSummary) {
    match &envelope.event_name {
        EventName::TapeStarted => {
            // Nothing special to do
        }
        EventName::TapeStopped => {
            // Nothing special to do
        }
        EventName::UICommand(name) => {
            summary.commands.push(name.clone());
        }
        EventName::DomainEvent(name) => {
            process_domain_event(name, &envelope.payload, summary);
        }
        EventName::SystemResponse(name) => {
            // Track responses for correlation
            if name.contains("Failed") || name.contains("Error") {
                let msg = envelope
                    .payload
                    .get("message")
                    .and_then(|v: &Value| v.as_str())
                    .or_else(|| envelope.payload.get("error").and_then(|v: &Value| v.as_str()));
                if let Some(msg) = msg {
                    summary.errors.push(format!("{}: {}", name, msg));
                }
            }
        }
        EventName::ErrorEvent(name) => {
            if let Some(msg) = envelope.payload.get("message").and_then(|v: &Value| v.as_str()) {
                summary.errors.push(format!("{}: {}", name, msg));
            } else {
                summary.errors.push(name.clone());
            }
        }
    }
}

fn process_domain_event(name: &str, payload: &Value, summary: &mut TapeSummary) {
    match name {
        "JobDispatched" => {
            if let Some(job_id_str) = extract_job_id(payload) {
                let plugin_name =
                    payload.get("plugin_name").and_then(|v| v.as_str()).map(String::from);

                summary.jobs.entry(job_id_str.clone()).or_insert(JobSummary {
                    job_id: job_id_str,
                    plugin_name,
                    status: "dispatched".to_string(),
                    rows: None,
                    outputs: Vec::new(),
                    error: None,
                });
            }
        }
        "JobCompleted" => {
            if let Some(job_id_str) = extract_job_id(payload) {
                if let Some(job) = summary.jobs.get_mut(&job_id_str) {
                    job.status = "completed".to_string();
                    if let Some(rows) = payload.get("rows").and_then(|v| v.as_u64()) {
                        job.rows = Some(rows);
                    }
                }
            }
        }
        "JobFailed" => {
            if let Some(job_id_str) = extract_job_id(payload) {
                if let Some(job) = summary.jobs.get_mut(&job_id_str) {
                    job.status = "failed".to_string();
                    job.error = payload.get("error").and_then(|v| v.as_str()).map(String::from);
                }
            }
        }
        "MaterializationRecorded" => {
            summary.materializations += 1;
            if let Some(job_id_str) = extract_job_id(payload) {
                if let Some(job) = summary.jobs.get_mut(&job_id_str) {
                    if let Some(output) = payload.get("output_name").and_then(|v| v.as_str()) {
                        job.outputs.push(output.to_string());
                    }
                    if let Some(rows) = payload.get("rows").and_then(|v| v.as_u64()) {
                        job.rows = Some(rows);
                    }
                }
            }
        }
        _ => {}
    }
}

fn extract_job_id(payload: &Value) -> Option<String> {
    match payload.get("job_id") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => {
            if let Some(v) = n.as_u64() {
                Some(v.to_string())
            } else if let Some(v) = n.as_i64() {
                Some(v.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn print_summary_text(summary: &TapeSummary) {
    println!("=== Tape Summary ===\n");
    println!("Schema Version: {}", summary.schema_version);
    println!("Total Events: {}", summary.event_count);
    println!();

    if !summary.commands.is_empty() {
        println!("Commands ({}):", summary.commands.len());
        for cmd in &summary.commands {
            println!("  - {}", cmd);
        }
        println!();
    }

    if !summary.jobs.is_empty() {
        println!("Jobs ({}):", summary.jobs.len());
        let mut completed = 0;
        let mut failed = 0;
        let mut total_rows: u64 = 0;

        for job in summary.jobs.values() {
            let plugin = job.plugin_name.as_deref().unwrap_or("unknown");
            let rows_str = job.rows.map(|r| format!("{} rows", r)).unwrap_or_default();
            println!("  [{}] {} - {} {}", job.status, job.job_id, plugin, rows_str);

            if let Some(error) = &job.error {
                println!("       Error: {}", error);
            }

            match job.status.as_str() {
                "completed" => completed += 1,
                "failed" => failed += 1,
                _ => {}
            }

            if let Some(rows) = job.rows {
                total_rows += rows;
            }
        }

        println!();
        println!("  Summary: {} completed, {} failed, {} total rows", completed, failed, total_rows);
        println!();
    }

    if summary.materializations > 0 {
        println!("Materializations: {}", summary.materializations);
        println!();
    }

    if !summary.errors.is_empty() {
        println!("Errors ({}):", summary.errors.len());
        for error in &summary.errors {
            println!("  - {}", error);
        }
        println!();
    }
}

fn print_summary_json(summary: &TapeSummary) -> Result<()> {
    let jobs_list: Vec<_> = summary.jobs.values().map(|j| {
        serde_json::json!({
            "job_id": j.job_id,
            "plugin_name": j.plugin_name,
            "status": j.status,
            "rows": j.rows,
            "outputs": j.outputs,
            "error": j.error,
        })
    }).collect();

    let output = serde_json::json!({
        "schema_version": summary.schema_version,
        "event_count": summary.event_count,
        "commands": summary.commands,
        "jobs": jobs_list,
        "materializations": summary.materializations,
        "errors": summary.errors,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn validate_tape(tape_file: &PathBuf) -> Result<()> {
    let file = File::open(tape_file)
        .with_context(|| format!("Failed to open tape file: {}", tape_file.display()))?;
    let reader = BufReader::new(file);

    let mut line_count = 0;
    let mut errors: Vec<String> = Vec::new();
    let mut last_seq: Option<u64> = None;
    let mut schema_version_seen: Option<u32> = None;

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.with_context(|| format!("Failed to read line {}", line_num + 1))?;

        if line.trim().is_empty() {
            continue;
        }

        line_count += 1;

        match serde_json::from_str::<EnvelopeV1>(&line) {
            Ok(envelope) => {
                // Check schema version
                if let Some(seen) = schema_version_seen {
                    if envelope.schema_version != seen {
                        errors.push(format!(
                            "Line {}: Schema version mismatch ({} vs {})",
                            line_num + 1, envelope.schema_version, seen
                        ));
                    }
                } else {
                    schema_version_seen = Some(envelope.schema_version);
                    if envelope.schema_version != SCHEMA_VERSION {
                        errors.push(format!(
                            "Warning: Tape uses schema version {} (current: {})",
                            envelope.schema_version, SCHEMA_VERSION
                        ));
                    }
                }

                // Check sequence monotonicity
                if let Some(last) = last_seq {
                    if envelope.seq != last + 1 {
                        errors.push(format!(
                            "Line {}: Sequence gap ({} -> {})",
                            line_num + 1, last, envelope.seq
                        ));
                    }
                }
                last_seq = Some(envelope.seq);

                // Check required fields
                if envelope.event_id.is_empty() {
                    errors.push(format!("Line {}: Empty event_id", line_num + 1));
                }
            }
            Err(e) => {
                errors.push(format!("Line {}: Parse error: {}", line_num + 1, e));
            }
        }
    }

    println!("=== Tape Validation ===\n");
    println!("File: {}", tape_file.display());
    println!("Lines: {}", line_count);

    if let Some(v) = schema_version_seen {
        println!("Schema Version: {}", v);
    }

    if errors.is_empty() {
        println!("\nResult: VALID ✓");
    } else {
        println!("\nIssues ({}):", errors.len());
        for error in &errors {
            println!("  - {}", error);
        }
        println!("\nResult: INVALID ✗");
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_tape(events: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for event in events {
            writeln!(file, "{}", event).unwrap();
        }
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_validate_valid_tape() {
        let tape = create_test_tape(&[
            r#"{"schema_version":1,"event_id":"e1","seq":0,"timestamp":"2026-01-01T00:00:00Z","correlation_id":null,"parent_id":null,"event_name":{"type":"tape_started"},"payload":{}}"#,
            r#"{"schema_version":1,"event_id":"e2","seq":1,"timestamp":"2026-01-01T00:00:01Z","correlation_id":null,"parent_id":null,"event_name":{"type":"ui_command","name":"Scan"},"payload":{}}"#,
        ]);

        // Just check it doesn't panic
        let result = validate_tape(&tape.path().to_path_buf());
        assert!(result.is_ok());
    }

    #[test]
    fn test_explain_empty_tape() {
        let tape = create_test_tape(&[
            r#"{"schema_version":1,"event_id":"e1","seq":0,"timestamp":"2026-01-01T00:00:00Z","correlation_id":null,"parent_id":null,"event_name":{"type":"tape_started"},"payload":{}}"#,
        ]);

        let result = explain_tape(&tape.path().to_path_buf(), "text");
        assert!(result.is_ok());
    }

    #[test]
    fn test_explain_with_jobs() {
        let tape = create_test_tape(&[
            r#"{"schema_version":1,"event_id":"e1","seq":0,"timestamp":"2026-01-01T00:00:00Z","correlation_id":null,"parent_id":null,"event_name":{"type":"tape_started"},"payload":{}}"#,
            r#"{"schema_version":1,"event_id":"e2","seq":1,"timestamp":"2026-01-01T00:00:01Z","correlation_id":"run-1","parent_id":null,"event_name":{"type":"domain_event","name":"JobDispatched"},"payload":{"job_id":"123","plugin_name":"test_parser"}}"#,
            r#"{"schema_version":1,"event_id":"e3","seq":2,"timestamp":"2026-01-01T00:00:02Z","correlation_id":"run-1","parent_id":null,"event_name":{"type":"domain_event","name":"JobCompleted"},"payload":{"job_id":"123","rows":100}}"#,
        ]);

        let result = explain_tape(&tape.path().to_path_buf(), "json");
        assert!(result.is_ok());
    }
}
