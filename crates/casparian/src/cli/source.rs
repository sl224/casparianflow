//! Source command - Manage data sources
//!
//! W5 implements this module.

use clap::Subcommand;
use std::path::PathBuf;

/// Subcommands for source management
#[derive(Subcommand, Debug, Clone)]
pub enum SourceAction {
    /// List all sources
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add a new source
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        recursive: bool,
    },
    /// Show source details
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Remove a source
    Remove {
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// Sync a source (re-discover files)
    Sync {
        name: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

/// Execute the source command
pub fn run(_action: SourceAction) -> anyhow::Result<()> {
    todo!("W5 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W5 implements this")]
    fn test_source_not_implemented() {
        let action = SourceAction::List { json: false };
        run(action).unwrap();
    }
}
