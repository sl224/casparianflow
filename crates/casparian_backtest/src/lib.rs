//! Backtest Engine - Multi-file validation with fail-fast optimization
//!
//! # Philosophy: High-Failure Files First
//!
//! The backtest engine validates a parser against multiple files to ensure
//! it handles the variety of real-world data. The key optimization is
//! **fail-fast**: test files that have historically failed FIRST.
//!
//! This enables rapid feedback:
//! - If high-failure files still fail, stop early (parser not ready)
//! - If they pass, continue with remaining files
//! - Track failure history to prioritize testing order
//!
//! # Backtest Loop
//!
//! 1. Get all files in scope
//! 2. Sort by: high-failure first, then resolved, then untested, then passing
//! 3. Run parser against each file
//! 4. Check pass rate against threshold
//! 5. If below threshold after high-failure files, stop early
//! 6. Record results for next iteration
//!
//! # Termination Conditions
//!
//! - Pass rate achieved (e.g., 95%)
//! - Max iterations reached
//! - Plateau detected (no improvement for N iterations)
//! - Timeout
//! - User stopped

pub mod failfast;
pub mod high_failure;
pub mod iteration;
pub mod metrics;

pub use failfast::*;
pub use high_failure::*;
pub use iteration::*;
pub use metrics::*;
