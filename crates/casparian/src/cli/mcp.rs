//! MCP (Model Context Protocol) CLI commands
//!
//! Provides the `casparian mcp serve` command for running the MCP server.

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum McpAction {
    /// Start the MCP server (stdio transport)
    ///
    /// Runs a JSON-RPC 2.0 server over stdin/stdout for AI tool integration.
    /// The server exposes tools for file discovery, parser execution, and data queries.
    ///
    /// Example usage with Claude Desktop:
    ///   Add to claude_desktop_config.json:
    ///   {
    ///     "mcpServers": {
    ///       "casparian": {
    ///         "command": "casparian",
    ///         "args": ["mcp", "serve"]
    ///       }
    ///     }
    ///   }
    Serve {
        /// Allowed paths (default: current directory only)
        /// Specify multiple times for multiple paths.
        #[arg(long = "allow-path", short = 'p')]
        allow_paths: Vec<PathBuf>,

        /// Maximum output size in bytes (default: 1MB)
        #[arg(long, default_value = "1048576")]
        max_output_bytes: usize,

        /// Maximum rows returned (default: 10000)
        #[arg(long, default_value = "10000")]
        max_rows: usize,

        /// Audit log file path (default: ~/.casparian_flow/mcp_audit.ndjson)
        #[arg(long)]
        audit_log: Option<PathBuf>,

        /// State store path (default: ~/.casparian_flow/state.sqlite)
        #[arg(long = "state-store")]
        database: Option<PathBuf>,

        /// Default output directory (default: ~/.casparian_flow/output)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Control API address (default: tcp://127.0.0.1:5556)
        #[arg(long)]
        control_addr: Option<String>,

        /// Allow MCP to open the DB in write mode when Control API is unavailable
        #[arg(long)]
        standalone_db_writer: bool,
    },

    /// Approve a pending MCP operation
    ///
    /// MCP operations that write data (like parser execution) require
    /// human approval. Use this command to approve or reject requests.
    Approve {
        /// Approval ID (from MCP tool response)
        approval_id: String,

        /// Reject instead of approve
        #[arg(long)]
        reject: bool,

        /// Control API address (default: tcp://127.0.0.1:5556)
        #[arg(long)]
        control_addr: Option<String>,

        /// Allow MCP to open the DB in write mode when Control API is unavailable
        #[arg(long)]
        standalone_db_writer: bool,
    },

    /// List pending approvals
    List {
        /// Show all (including expired)
        #[arg(long)]
        all: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Control API address (default: tcp://127.0.0.1:5556)
        #[arg(long)]
        control_addr: Option<String>,

        /// Allow MCP to open the DB in write mode when Control API is unavailable
        #[arg(long)]
        standalone_db_writer: bool,
    },
}

pub fn run(action: McpAction) -> Result<()> {
    match action {
        McpAction::Serve {
            allow_paths,
            max_output_bytes,
            max_rows,
            audit_log,
            database,
            output,
            control_addr,
            standalone_db_writer,
        } => run_serve(
            allow_paths,
            max_output_bytes,
            max_rows,
            audit_log,
            database,
            output,
            control_addr,
            standalone_db_writer,
        ),
        McpAction::Approve {
            approval_id,
            reject,
            control_addr,
            standalone_db_writer,
        } => run_approve(approval_id, reject, control_addr, standalone_db_writer),
        McpAction::List {
            all,
            json,
            control_addr,
            standalone_db_writer,
        } => run_list(all, json, control_addr, standalone_db_writer),
    }
}

fn run_serve(
    allow_paths: Vec<PathBuf>,
    max_output_bytes: usize,
    max_rows: usize,
    audit_log: Option<PathBuf>,
    database: Option<PathBuf>,
    _output: Option<PathBuf>,
    control_addr: Option<String>,
    standalone_db_writer: bool,
) -> Result<()> {
    use super::config;
    use casparian_mcp::{McpServer, McpServerConfig};

    // Build config with sensible defaults
    let allowed_paths = if allow_paths.is_empty() {
        // Default: current directory
        vec![std::env::current_dir()?]
    } else {
        allow_paths
    };

    let audit_log_path =
        Some(audit_log.unwrap_or_else(|| config::casparian_home().join("mcp_audit.ndjson")));

    let db_path = database.unwrap_or_else(config::state_store_path);
    let mcp_config = McpServerConfig {
        server_name: "casparian-mcp".to_string(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
        allowed_paths,
        max_response_bytes: max_output_bytes,
        max_rows,
        audit_log_path,
        db_path,
        query_catalog_path: config::query_catalog_path(),
        control_addr,
        standalone_db_writer,
    };

    info!("Starting MCP server (stdio)");

    // Run the synchronous server (no async runtime required)
    let mut server = McpServer::new(mcp_config)?;
    server.run()
}

fn run_approve(
    approval_id: String,
    reject: bool,
    control_addr: Option<String>,
    standalone_db_writer: bool,
) -> Result<()> {
    use super::config;
    use casparian_mcp::approvals::{ApprovalId, ApprovalManager};

    let db_path = config::state_store_path();
    let manager = if standalone_db_writer {
        ApprovalManager::new(db_path)?
    } else {
        ApprovalManager::new_control(control_addr)?
    };

    let id = ApprovalId::from_string(&approval_id);

    if reject {
        manager.reject(&id, Some("Rejected via CLI".to_string()))?;
        println!("Rejected approval: {}", approval_id);
    } else {
        manager.approve(&id)?;
        println!("Approved: {}", approval_id);

        // Get the approval to show what was approved
        if let Some(approval) = manager.get_approval(&id)? {
            println!("Operation: {}", approval.operation.description());
            println!("Target: {}", approval.summary.target_path);
        }
    }

    Ok(())
}

fn run_list(
    all: bool,
    json: bool,
    control_addr: Option<String>,
    standalone_db_writer: bool,
) -> Result<()> {
    use super::config;
    use casparian_mcp::approvals::ApprovalManager;

    let db_path = config::state_store_path();
    let manager = if standalone_db_writer {
        ApprovalManager::new(db_path)?
    } else {
        ApprovalManager::new_control(control_addr)?
    };

    let status_filter = if all { None } else { Some("pending") };
    let approvals = manager.list_approvals(status_filter)?;

    if json {
        let output: Vec<serde_json::Value> = approvals
            .iter()
            .map(|a| {
                serde_json::json!({
                    "approval_id": a.approval_id.to_string(),
                    "status": a.status.status_str(),
                    "operation": a.operation.description(),
                    "description": a.summary.description,
                    "target_path": a.summary.target_path,
                    "file_count": a.summary.file_count,
                    "created_at": a.created_at.to_rfc3339(),
                    "expires_at": a.expires_at.to_rfc3339(),
                    "approve_command": a.approve_command(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if approvals.is_empty() {
            println!("No pending approvals");
            return Ok(());
        }

        println!(
            "{:<36} {:<10} {:<40} {}",
            "APPROVAL ID", "STATUS", "OPERATION", "EXPIRES"
        );
        println!("{}", "-".repeat(100));

        for a in &approvals {
            let expires = a.expires_at.format("%Y-%m-%d %H:%M:%S");
            println!(
                "{:<36} {:<10} {:<40} {}",
                a.approval_id.to_string(),
                a.status.status_str(),
                truncate(&a.summary.description, 40),
                expires
            );
        }

        println!();
        println!("To approve: casparian mcp approve <APPROVAL_ID>");
        println!("To reject:  casparian mcp approve <APPROVAL_ID> --reject");
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
