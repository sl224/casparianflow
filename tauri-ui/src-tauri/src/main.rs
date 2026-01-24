//! Casparian Flow Desktop UI - Tauri Application
//!
//! This is the entry point for the Tauri application. It initializes the
//! application state and registers all Tauri commands.

// TODO(Phase 3): Fix these clippy warnings properly during silent corruption sweep
#![allow(dead_code)]
#![allow(clippy::redundant_closure)]
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod session_storage;
mod session_types;
mod state;
mod tape;

#[cfg(test)]
mod tests;

use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tracing_subscriber::Layer;

fn main() {
    // Initialize tracing (console + rolling file)
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "casparian_flow_ui=info,casparian_sentinel=info".into());

    let mut _log_guard: Option<tracing_appender::non_blocking::WorkerGuard> = None;
    let file_layer = match ensure_logs_dir() {
        Ok(log_dir) => {
            let file_appender = tracing_appender::rolling::daily(log_dir, "casparian-ui.log");
            let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
            _log_guard = Some(guard);
            Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(file_writer)
                    .with_ansi(false)
                    .with_filter(env_filter.clone()),
            )
        }
        Err(err) => {
            eprintln!("Warning: failed to create logs directory: {}", err);
            None
        }
    };

    let registry = tracing_subscriber::registry().with(file_layer);
    let console_layer = tracing_subscriber::fmt::layer().with_filter(env_filter.clone());
    registry.with(console_layer).init();

    tracing::info!("Starting Casparian Flow UI");

    // Initialize application state
    let app_state = match AppState::new() {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("Failed to initialize application state: {}", e);
            eprintln!("Failed to initialize application state: {}", e);
            std::process::exit(1);
        }
    };

    tracing::info!("Database path: {}", app_state.db_path);

    // Build and run Tauri application
    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            // Session commands (real typed sessions)
            commands::sessions::session_list,
            commands::sessions::session_create,
            commands::sessions::session_status,
            commands::sessions::session_advance,
            commands::sessions::session_cancel,
            commands::sessions::session_list_pending,
            // Approval commands
            commands::approvals::approval_list,
            commands::approvals::approval_decide,
            commands::approvals::approval_stats,
            // Query commands
            commands::query::query_execute,
            // Job commands
            commands::jobs::job_list,
            commands::jobs::job_status,
            commands::jobs::job_cancel,
            // Stats commands
            commands::stats::dashboard_stats,
            // Intent pipeline commands - Selection
            commands::intent::casp_select_propose,
            commands::intent::casp_select_approve,
            // Intent pipeline commands - File Sets
            commands::intent::casp_fileset_sample,
            commands::intent::casp_fileset_info,
            // Intent pipeline commands - Tag Rules
            commands::intent::casp_tags_apply_rules,
            // Intent pipeline commands - Path Fields
            commands::intent::casp_path_fields_apply,
            // Intent pipeline commands - Schema
            commands::intent::casp_schema_promote,
            commands::intent::casp_schema_resolve_ambiguity,
            // Intent pipeline commands - Backtest
            commands::intent::casp_intent_backtest_start,
            commands::intent::casp_intent_backtest_status,
            commands::intent::casp_intent_backtest_report,
            // Intent pipeline commands - Patch
            commands::intent::casp_patch_apply,
            // Intent pipeline commands - Publish
            commands::intent::casp_publish_plan,
            commands::intent::casp_publish_execute,
            // Intent pipeline commands - Run
            commands::intent::casp_run_plan,
            commands::intent::casp_run_execute,
            // Intent pipeline commands - Scan
            commands::intent::casparian_scan,
            // Intent pipeline commands - Parser
            commands::intent::parser_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn casparian_home() -> Option<std::path::PathBuf> {
    if let Ok(override_path) = std::env::var("CASPARIAN_HOME") {
        return Some(std::path::PathBuf::from(override_path));
    }
    dirs::home_dir().map(|h| h.join(".casparian_flow"))
}

fn ensure_logs_dir() -> std::io::Result<std::path::PathBuf> {
    let home = casparian_home().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "home directory not found")
    })?;
    let dir = home.join("logs");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
