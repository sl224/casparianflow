//! Rule command - Manage tagging rules
//!
//! W5 implements this module.

use clap::Subcommand;

/// Subcommands for rule management
#[derive(Subcommand, Debug, Clone)]
pub enum RuleAction {
    /// List all rules
    List {
        #[arg(long)]
        json: bool,
    },
    /// Add a new rule
    Add {
        /// Glob pattern to match files
        pattern: String,
        /// Topic to assign to matching files
        #[arg(long)]
        topic: String,
        /// Rule priority (higher = evaluated first)
        #[arg(long, default_value = "0")]
        priority: i32,
    },
    /// Show rule details
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Remove a rule
    Remove {
        id: String,
        #[arg(long)]
        force: bool,
    },
    /// Test a rule against a path
    Test {
        id: String,
        path: String,
    },
}

/// Execute the rule command
pub fn run(_action: RuleAction) -> anyhow::Result<()> {
    todo!("W5 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W5 implements this")]
    fn test_rule_not_implemented() {
        let action = RuleAction::List { json: false };
        run(action).unwrap();
    }
}
