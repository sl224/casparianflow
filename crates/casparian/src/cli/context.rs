//! CLI context management
//!
//! Provides kubectl-style context switching for sources.
//! Context is stored in `~/.casparian_flow/context.toml`.

use std::path::PathBuf;

use casparian::scout::WorkspaceId;

/// Get the path to the context file
fn context_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".casparian_flow")
        .join("context.toml")
}

/// Context configuration
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Context {
    #[serde(default)]
    pub source: Option<SourceContext>,
    #[serde(default)]
    pub workspace: Option<WorkspaceContext>,
}

/// Source context
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceContext {
    pub name: String,
}

/// Workspace context
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceContext {
    pub id: String,
}

/// Get the default source from context file
pub fn get_default_source() -> Option<String> {
    let path = context_file_path();
    if !path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&path).ok()?;
    let context: Context = toml::from_str(&content).ok()?;
    context.source.map(|s| s.name)
}

/// Get the active workspace ID from context file
pub fn get_active_workspace_id() -> anyhow::Result<Option<WorkspaceId>> {
    let path = context_file_path();
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let context: Context = toml::from_str(&content).unwrap_or_default();
    let Some(workspace) = context.workspace else {
        return Ok(None);
    };

    let id = WorkspaceId::parse(&workspace.id)?;
    Ok(Some(id))
}

/// Set the active workspace ID in context file
pub fn set_active_workspace(id: &WorkspaceId) -> anyhow::Result<()> {
    let path = context_file_path();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut context = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        Context::default()
    };

    context.workspace = Some(WorkspaceContext {
        id: id.to_string(),
    });

    let content = toml::to_string_pretty(&context)?;
    std::fs::write(&path, content)?;

    Ok(())
}

/// Clear the active workspace from context file
pub fn clear_active_workspace() -> anyhow::Result<()> {
    let path = context_file_path();
    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let mut context: Context = toml::from_str(&content).unwrap_or_default();

    context.workspace = None;

    let content = toml::to_string_pretty(&context)?;
    std::fs::write(&path, content)?;

    Ok(())
}

/// Set the default source in context file
pub fn set_default_source(name: &str) -> anyhow::Result<()> {
    let path = context_file_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Load existing context or create new
    let mut context = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        Context::default()
    };

    // Update source context
    context.source = Some(SourceContext {
        name: name.to_string(),
    });

    // Write back
    let content = toml::to_string_pretty(&context)?;
    std::fs::write(&path, content)?;

    Ok(())
}

/// Clear the default source from context file
pub fn clear_default_source() -> anyhow::Result<()> {
    let path = context_file_path();

    if !path.exists() {
        return Ok(());
    }

    // Load existing context
    let content = std::fs::read_to_string(&path)?;
    let mut context: Context = toml::from_str(&content).unwrap_or_default();

    // Clear source context
    context.source = None;

    // Write back
    let content = toml::to_string_pretty(&context)?;
    std::fs::write(&path, content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_serialization() {
        let context = Context {
            source: Some(SourceContext {
                name: "invoices".to_string(),
            }),
            workspace: Some(WorkspaceContext {
                id: WorkspaceId::new().to_string(),
            }),
        };

        let toml_str = toml::to_string_pretty(&context).unwrap();
        assert!(toml_str.contains("invoices"));

        let parsed: Context = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.source.unwrap().name, "invoices");
    }

    #[test]
    fn test_empty_context() {
        let context = Context::default();
        assert!(context.source.is_none());
        assert!(context.workspace.is_none());

        let toml_str = toml::to_string_pretty(&context).unwrap();
        let parsed: Context = toml::from_str(&toml_str).unwrap();
        assert!(parsed.source.is_none());
    }
}
