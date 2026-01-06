//! Job command - Manage individual jobs
//!
//! W4 implements this module.

use clap::Subcommand;

/// Subcommands for job management
#[derive(Subcommand, Debug, Clone)]
pub enum JobAction {
    /// Show job details
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// View job logs
    Logs {
        id: String,
        #[arg(short = 'f', long)]
        follow: bool,
        #[arg(long)]
        tail: Option<usize>,
    },
    /// Retry a failed job
    Retry {
        id: String,
    },
    /// Cancel a pending or running job
    Cancel {
        id: String,
    },
}

/// Execute the job command
pub fn run(_action: JobAction) -> anyhow::Result<()> {
    todo!("W4 implements this")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "W4 implements this")]
    fn test_job_not_implemented() {
        let action = JobAction::Show {
            id: "test".to_string(),
            json: false,
        };
        run(action).unwrap();
    }
}
