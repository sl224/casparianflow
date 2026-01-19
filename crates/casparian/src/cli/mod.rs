//! CLI module for Casparian Flow
//!
//! This module provides the command-line interface for Casparian Flow,
//! including standalone utilities like `scan` and `preview` that don't
//! require a running Sentinel.

pub mod error;
pub mod output;

// W1: Core commands (fully implemented)
pub mod scan;
#[cfg(feature = "data-plane")]
pub mod preview;
#[cfg(not(feature = "data-plane"))]
pub mod preview {
    use std::path::PathBuf;

    #[derive(Debug)]
    pub struct PreviewArgs {
        pub file: PathBuf,
        pub rows: usize,
        pub schema: bool,
        pub raw: bool,
        pub head: Option<usize>,
        pub delimiter: Option<char>,
        pub json: bool,
    }

    pub fn run(_args: PreviewArgs) -> anyhow::Result<()> {
        anyhow::bail!("preview requires the `data-plane` feature")
    }
}
pub mod run;
pub mod backfill;
pub mod pipeline;

// W2: Tagging commands (stubs)
#[cfg(feature = "data-plane")]
pub mod tag;
#[cfg(not(feature = "data-plane"))]
pub mod tag {
    use std::path::PathBuf;

    #[derive(Debug)]
    pub struct TagArgs {
        pub path: Option<PathBuf>,
        pub topic: Option<String>,
        pub dry_run: bool,
        pub no_queue: bool,
    }

    #[derive(Debug)]
    pub struct UntagArgs {
        pub path: PathBuf,
    }

    pub fn run(_args: TagArgs) -> anyhow::Result<()> {
        anyhow::bail!("tag requires the `data-plane` feature")
    }

    pub fn run_untag(_args: UntagArgs) -> anyhow::Result<()> {
        anyhow::bail!("untag requires the `data-plane` feature")
    }
}
pub mod files;

// W3: Parser commands (stubs)
pub mod parser;

// W4: Job commands (stubs)
pub mod jobs;
pub mod job;
pub mod worker;

// W5: Resource commands (stubs)
pub mod source;
pub mod rule;
pub mod topic;

// W8: TUI
pub mod tui;

// Configuration and context
pub mod config;
pub mod context;

// Re-exports are used by the scan, preview, and resource modules
#[allow(unused_imports)]
pub use error::HelpfulError;
#[allow(unused_imports)]
pub use output::{format_number, format_number_signed, format_size, format_time, print_table};
