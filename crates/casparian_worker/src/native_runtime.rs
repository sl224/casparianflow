use anyhow::{Context, Result};
use arrow::ipc::reader::StreamReader;
use casparian_sinks::OutputBatch;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::bridge::OutputInfo;
use crate::runtime::{PluginRuntime, RunContext, RunOutputs};

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const PROTOCOL_VERSION: &str = "0.1";

#[derive(Debug)]
enum ControlFrame {
    Hello {
        protocol: String,
        parser_id: String,
        parser_version: String,
    },
    OutputBegin {
        output: String,
        schema_hash: String,
        stream_index: u32,
    },
    OutputEnd {
        output: String,
        rows_emitted: Option<u64>,
        stream_index: u32,
    },
    Warning(String),
    Error(String),
}

pub struct NativeSubprocessRuntime;

impl NativeSubprocessRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl PluginRuntime for NativeSubprocessRuntime {
    fn run_file(&self, ctx: &RunContext, input_path: &Path) -> Result<RunOutputs> {
        if ctx.entrypoint.trim().is_empty() {
            anyhow::bail!("Entrypoint is required for native runtime");
        }
        if ctx.schema_hashes.is_empty() {
            anyhow::bail!("Schema hashes are required for native runtime");
        }

        let mut child = Command::new(&ctx.entrypoint)
            .arg(input_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn native plugin process")?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;

        let (tx, rx) = mpsc::channel();
        let stderr_handle = spawn_stderr_reader(stderr, tx);

        let mut logs = String::new();
        let mut output_batches = Vec::new();
        let mut output_info = Vec::new();
        let mut stream_index_expected = 0;

        let hello = rx
            .recv_timeout(HELLO_TIMEOUT)
            .context("Timed out waiting for hello frame")?;
        match hello {
            ControlFrame::Hello {
                protocol,
                parser_id: _,
                parser_version: _,
            } => {
                if protocol != PROTOCOL_VERSION {
                    anyhow::bail!(
                        "Protocol mismatch: expected {}, got {}",
                        PROTOCOL_VERSION,
                        protocol
                    );
                }
            }
            other => {
                anyhow::bail!("Expected hello frame first, got {:?}", other);
            }
        }

        let mut stdout_reader = BufReader::new(stdout);
        loop {
            let frame = match rx.recv() {
                Ok(frame) => frame,
                Err(_) => break,
            };
            match frame {
                ControlFrame::OutputBegin {
                    output,
                    schema_hash,
                    stream_index,
                } => {
                    if stream_index != stream_index_expected {
                        anyhow::bail!(
                            "Unexpected stream_index {} (expected {})",
                            stream_index,
                            stream_index_expected
                        );
                    }
                    let expected_hash = ctx
                        .schema_hashes
                        .get(&output)
                        .ok_or_else(|| anyhow::anyhow!("Unknown output '{}'", output))?;
                    if expected_hash != &schema_hash {
                        anyhow::bail!(
                            "Schema hash mismatch for '{}': expected {}, got {}",
                            output,
                            expected_hash,
                            schema_hash
                        );
                    }

                    let batches = read_arrow_stream(&mut stdout_reader)
                        .with_context(|| format!("Failed to read Arrow stream for '{}'", output))?;

                    let end_frame = loop {
                        let next = rx.recv().context("Missing output_end frame")?;
                        match next {
                            ControlFrame::Warning(message) => {
                                logs.push_str(&message);
                                logs.push('\n');
                                continue;
                            }
                            other => break other,
                        }
                    };
                    match end_frame {
                        ControlFrame::OutputEnd {
                            output: end_output,
                            stream_index: end_index,
                            rows_emitted: _,
                        } => {
                            if end_output != output || end_index != stream_index {
                                anyhow::bail!(
                                    "output_end mismatch: expected {}#{}, got {}#{}",
                                    output,
                                    stream_index,
                                    end_output,
                                    end_index
                                );
                            }
                        }
                        ControlFrame::Error(message) => {
                            anyhow::bail!("Native plugin error: {}", message);
                        }
                        other => {
                            anyhow::bail!("Expected output_end after stream, got {:?}", other);
                        }
                    }

                    output_info.push(OutputInfo {
                        name: output,
                        table: None,
                    });
                    output_batches.push(batches);
                    stream_index_expected += 1;
                }
                ControlFrame::Warning(message) => {
                    logs.push_str(&message);
                    logs.push('\n');
                }
                ControlFrame::Error(message) => {
                    anyhow::bail!("Native plugin error: {}", message);
                }
                ControlFrame::Hello { .. } => {
                    anyhow::bail!("Unexpected hello frame after startup");
                }
                ControlFrame::OutputEnd { .. } => {
                    anyhow::bail!("output_end received without output_begin");
                }
            }
        }

        let status = child.wait().context("Failed to wait for native plugin")?;
        if !status.success() {
            anyhow::bail!("Native plugin exited with status {}", status);
        }

        let _ = stderr_handle.join();

        Ok(RunOutputs {
            output_batches,
            output_info,
            logs,
        })
    }
}

fn spawn_stderr_reader(stderr: std::process::ChildStderr, tx: Sender<ControlFrame>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match parse_control_frame(&line) {
                        Ok(frame) => {
                            let _ = tx.send(frame);
                        }
                        Err(err) => {
                            let _ = tx.send(ControlFrame::Error(format!(
                                "Invalid control frame: {}",
                                err
                            )));
                            break;
                        }
                    }
                }
                Err(err) => {
                    let _ = tx.send(ControlFrame::Error(format!(
                        "Failed to read stderr: {}",
                        err
                    )));
                    break;
                }
            }
        }
    })
}

fn parse_control_frame(line: &str) -> Result<ControlFrame> {
    let value: serde_json::Value = serde_json::from_str(line)?;
    let frame_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("control frame missing 'type'"))?;

    match frame_type {
        "hello" => Ok(ControlFrame::Hello {
            protocol: value
                .get("protocol")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            parser_id: value
                .get("parser_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            parser_version: value
                .get("parser_version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "output_begin" => Ok(ControlFrame::OutputBegin {
            output: value
                .get("output")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("output_begin missing output"))?
                .to_string(),
            schema_hash: value
                .get("schema_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("output_begin missing schema_hash"))?
                .to_string(),
            stream_index: value
                .get("stream_index")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow::anyhow!("output_begin missing stream_index"))? as u32,
        }),
        "output_end" => Ok(ControlFrame::OutputEnd {
            output: value
                .get("output")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("output_end missing output"))?
                .to_string(),
            rows_emitted: value.get("rows_emitted").and_then(|v| v.as_u64()),
            stream_index: value
                .get("stream_index")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow::anyhow!("output_end missing stream_index"))? as u32,
        }),
        "warning" => Ok(ControlFrame::Warning(
            value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("warning")
                .to_string(),
        )),
        "error" => Ok(ControlFrame::Error(
            value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("error")
                .to_string(),
        )),
        other => Ok(ControlFrame::Warning(format!(
            "Ignored control frame: {}",
            other
        ))),
    }
}

fn read_arrow_stream(
    reader: &mut BufReader<std::process::ChildStdout>,
) -> Result<Vec<OutputBatch>> {
    let mut stream_reader = StreamReader::try_new(reader, None)
        .context("stdout is not valid Arrow IPC stream")?;
    let mut batches = Vec::new();
    for batch in stream_reader.by_ref() {
        let batch = batch.context("Failed to read Arrow batch")?;
        batches.push(OutputBatch::from_record_batch(batch));
    }
    Ok(batches)
}
