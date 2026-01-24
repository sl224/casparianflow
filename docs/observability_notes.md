# Observability Notes (2026-01-24)

## Tracing init
- casparian CLI: `crates/casparian/src/main.rs` sets up EnvFilter, console layer (stdout/stderr; TUI errors only), and rolling file logs in `CASPARIAN_HOME/logs` (default `~/.casparian_flow/logs`, file prefix `casparian.log`).
- sentinel: `crates/casparian_sentinel/src/main.rs` uses EnvFilter + console + rolling file logs in `CASPARIAN_HOME/logs` (file prefix `casparian-sentinel.log`).
- tauri UI: `tauri-ui/src-tauri/src/main.rs` uses EnvFilter + console + rolling file logs in `CASPARIAN_HOME/logs` (file prefix `casparian-ui.log`).

## Tape events
- CLI `--tape` creates a `TapeWriter` in `crates/casparian/src/main.rs` and records UICommand/SystemResponse/ErrorEvent.
- Scan telemetry (`scan.start`, `scan.progress`, `scan.complete`, `scan.fail`) emitted in `crates/casparian/src/cli/scan.rs` and `crates/casparian/src/cli/tui/app.rs` via `casparian::telemetry::TelemetryRecorder`.
- Run/pipeline telemetry (`run.start/complete/fail`) emitted in `crates/casparian/src/cli/run.rs` and `crates/casparian/src/cli/pipeline.rs`.

## Scan progress
- Progress/throughput/stall tracking lives in `crates/casparian/src/scout/scanner.rs` (`ProgressEmitter` with time + count emission).
- CLI/TUI forward `ScanProgress` via channels in `crates/casparian/src/cli/scan.rs` and `crates/casparian/src/cli/tui/app.rs`.

## Metric keys
- Canonical keys + helpers in `crates/casparian_protocol/src/metrics.rs`.
- Worker produces metrics using constants; sentinel + UI consume using helpers/constants.

## Support bundle
- `casparian support-bundle` implemented in `crates/casparian/src/cli/support_bundle.rs`.
- Includes tape files from `CASPARIAN_HOME/tapes` and log files from `CASPARIAN_HOME/logs` by default, plus redacted config + manifest.
