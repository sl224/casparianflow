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

#[cfg(test)]
mod tests;

use state::AppState;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "casparian_flow_ui=info,casparian_sentinel=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

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
