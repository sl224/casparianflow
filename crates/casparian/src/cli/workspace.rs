//! Workspace CLI commands and active workspace resolution.

use crate::cli::config;
use crate::cli::context;
use crate::cli::error::HelpfulError;
use anyhow::Context as AnyhowContext;
use casparian::scout::{Database, Workspace, WorkspaceId};
use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum WorkspaceAction {
    /// List available workspaces
    List,
    /// Create a new workspace (and set it active)
    Create {
        /// Workspace name
        name: String,
    },
    /// Set the active workspace (by id or name)
    Set {
        /// Workspace id or name. If omitted, shows current.
        workspace: Option<String>,
    },
}

pub fn run(action: WorkspaceAction) -> anyhow::Result<()> {
    match action {
        WorkspaceAction::List => list_workspaces(),
        WorkspaceAction::Create { name } => create_workspace(&name),
        WorkspaceAction::Set { workspace } => set_workspace(workspace.as_deref()),
    }
}

pub fn resolve_active_workspace_id(db: &Database) -> Result<WorkspaceId, HelpfulError> {
    let active = context::get_active_workspace_id().map_err(|err| {
        HelpfulError::new(format!("Workspace context error: {}", err))
            .with_context("Delete the context file to reset")
    })?;

    if let Some(id) = active {
        match db.get_workspace(&id) {
            Ok(Some(_)) => return Ok(id),
            Ok(None) => {
                context::clear_active_workspace().map_err(|err| {
                    HelpfulError::new(format!("Failed to clear workspace context: {}", err))
                })?;
            }
            Err(err) => {
                return Err(HelpfulError::new(format!(
                    "Failed to load active workspace: {}",
                    err
                ))
                .with_context("Active workspace is required for this command"));
            }
        }
    }

    let workspace = db
        .ensure_default_workspace()
        .map_err(|e| HelpfulError::new(format!("Failed to ensure workspace: {}", e)))?;
    context::set_active_workspace(&workspace.id).map_err(|err| {
        HelpfulError::new(format!("Failed to persist workspace context: {}", err))
    })?;
    Ok(workspace.id)
}

fn list_workspaces() -> anyhow::Result<()> {
    let db = open_db()?;
    let workspaces = db.list_workspaces()?;
    let active_id = context::get_active_workspace_id().ok().flatten();

    if workspaces.is_empty() {
        println!("No workspaces found.");
        println!("Create one with: casparian workspace create <name>");
        return Ok(());
    }

    println!("WORKSPACES");
    for workspace in workspaces {
        let marker = if Some(workspace.id) == active_id {
            "*"
        } else {
            " "
        };
        println!("{} {} ({})", marker, workspace.name, workspace.id);
    }

    Ok(())
}

fn create_workspace(name: &str) -> anyhow::Result<()> {
    let db = open_db()?;
    let workspace = db.create_workspace(name)?;
    context::set_active_workspace(&workspace.id)?;
    println!("Created workspace '{}' ({})", workspace.name, workspace.id);
    println!("Active workspace set to '{}'", workspace.name);
    Ok(())
}

fn set_workspace(workspace_ref: Option<&str>) -> anyhow::Result<()> {
    let db = open_db()?;

    if let Some(workspace_ref) = workspace_ref {
        let workspace = resolve_workspace_ref(&db, workspace_ref)?;
        context::set_active_workspace(&workspace.id)?;
        println!("Active workspace set to '{}' ({})", workspace.name, workspace.id);
        return Ok(());
    }

    let active_id = context::get_active_workspace_id().ok().flatten();
    if let Some(active_id) = active_id {
        if let Some(workspace) = db.get_workspace(&active_id)? {
            println!("Active workspace: '{}' ({})", workspace.name, workspace.id);
        } else {
            println!("Active workspace not found. Use `casparian workspace set <id|name>`.");
        }
    } else {
        println!("No active workspace set. Use `casparian workspace set <id|name>`.");
    }

    Ok(())
}

fn resolve_workspace_ref(db: &Database, raw: &str) -> anyhow::Result<Workspace> {
    if let Ok(id) = WorkspaceId::parse(raw) {
        if let Some(workspace) = db.get_workspace(&id)? {
            return Ok(workspace);
        }
    }

    db.get_workspace_by_name(raw)?
        .ok_or_else(|| anyhow::anyhow!("Workspace '{}' not found", raw))
}

fn open_db() -> anyhow::Result<Database> {
    let db_path = config::active_db_path();
    Database::open(&db_path).with_context(|| {
        format!(
            "Failed to open database at {}",
            db_path.to_string_lossy()
        )
    })
}
