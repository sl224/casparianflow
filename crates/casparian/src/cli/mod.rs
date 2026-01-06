//! CLI module for Casparian Flow
//!
//! This module provides the command-line interface for Casparian Flow,
//! including standalone utilities like `scan` and `preview` that don't
//! require a running Sentinel.

pub mod error;
pub mod output;

// W1: Core commands (fully implemented)
pub mod scan;
pub mod preview;

// W2: Tagging commands (stubs)
pub mod tag;
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

// W7: MCP Server
pub mod mcp;

// W8: TUI
pub mod tui;

// Configuration
pub mod config;

// Re-exports are used by the scan and preview modules
#[allow(unused_imports)]
pub use error::HelpfulError;
#[allow(unused_imports)]
pub use output::{format_size, format_time, print_table};
