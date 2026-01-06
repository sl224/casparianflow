//! Parser command - Manage parsers
//!
//! W3 implements this module.

use clap::Subcommand;
use std::path::PathBuf;

/// Subcommands for parser management
#[derive(Subcommand, Debug, Clone)]
pub enum ParserAction {
    /// List all parsers
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show parser details
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Create a new parser
    Create {
        name: String,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        topic: Option<String>,
    },
    /// Test a parser against a file
    Test {
        name: String,
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Publish a parser as a plugin
    Publish {
        name: String,
        #[arg(long)]
        version: String,
    },
    /// Delete a parser
    Delete {
        name: String,
        #[arg(long)]
        force: bool,
    },
}

/// Execute the parser command
pub fn run(_action: ParserAction) -> anyhow::Result<()> {
    todo!("W3 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W3 implements this")]
    fn test_parser_not_implemented() {
        let action = ParserAction::List { json: false };
        run(action).unwrap();
    }
}
