//! Topic command - Manage topics
//!
//! W5 implements this module.

use clap::Subcommand;

/// Subcommands for topic management
#[derive(Subcommand, Debug, Clone)]
pub enum TopicAction {
    /// List all topics
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create a new topic
    Create {
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Show topic details
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Delete a topic
    Delete {
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// List files for a topic
    Files {
        name: String,
        #[arg(long, default_value = "50")]
        limit: usize,
    },
}

/// Execute the topic command
pub fn run(_action: TopicAction) -> anyhow::Result<()> {
    todo!("W5 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W5 implements this")]
    fn test_topic_not_implemented() {
        let action = TopicAction::List { json: false };
        run(action).unwrap();
    }
}
