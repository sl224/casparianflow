//! Worker command - Manage workers
//!
//! W4 implements this module.

use clap::Subcommand;

/// Subcommands for worker management
#[derive(Subcommand, Debug, Clone)]
pub enum WorkerAction {
    /// List all workers
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show worker details
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Drain a worker (stop accepting new jobs)
    Drain {
        id: String,
    },
    /// Remove a worker
    Remove {
        id: String,
        #[arg(long)]
        force: bool,
    },
}

/// Execute the worker command
pub fn run(_action: WorkerAction) -> anyhow::Result<()> {
    todo!("W4 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W4 implements this")]
    fn test_worker_not_implemented() {
        let action = WorkerAction::List { json: false };
        run(action).unwrap();
    }
}
