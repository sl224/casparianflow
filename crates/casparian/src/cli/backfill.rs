//! Backfill command - Re-process files when parser version changes
//!
//! When a parser is updated to a new version, this command identifies files
//! that were processed with old versions and need re-processing.
//!
//! The backfill workflow:
//! 1. Find the latest version of a parser
//! 2. Find all files tagged with the parser's subscribed topics
//! 3. Identify files that haven't been processed with the latest version
//! 4. Re-process them (or preview what would be processed)
//!
//! Usage:
//!   casparian backfill my_parser              # Preview files to backfill
//!   casparian backfill my_parser --execute    # Actually run backfill
//!   casparian backfill my_parser --limit 10   # Limit to 10 files
//!
//! NOTE: Backfill is not available in v1 and currently returns a helpful error.

use anyhow::Result;

use crate::cli::error::HelpfulError;

/// Arguments for the backfill command
#[derive(Debug, Clone)]
pub struct BackfillArgs {
    /// Parser name to backfill
    pub parser_name: String,
    /// Actually execute the backfill (default: preview mode)
    pub execute: bool,
    /// Maximum files to process
    pub limit: Option<usize>,
    /// Output as JSON
    pub json: bool,
    /// Force re-processing even if already processed with this version
    pub force: bool,
}

/// Run the backfill command
pub fn run(args: BackfillArgs) -> Result<()> {
    let _ = (
        &args.parser_name,
        args.execute,
        args.limit,
        args.json,
        args.force,
    );
    Err(HelpfulError::new("Backfill is not available in v1")
        .with_context("Parser history and topic subscriptions are not tracked in the v1 registry")
        .with_suggestions([
            "TRY: casparian run <parser.py> <input> for manual reprocessing".to_string(),
            "TRY: use pipeline backfill once registry backfill is implemented".to_string(),
        ])
        .into())
}
