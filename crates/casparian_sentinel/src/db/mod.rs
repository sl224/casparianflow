//! Database layer for Sentinel
//!
//! Ported from Python SQLAlchemy to Rust sqlx.

pub mod models;
pub mod queue;

pub use queue::JobQueue;
