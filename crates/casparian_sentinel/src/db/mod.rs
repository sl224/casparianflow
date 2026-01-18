//! Database layer for Sentinel
//!
//! Ported from Python SQLAlchemy to Rust with casparian_db.

pub mod models;
pub mod queue;

pub use queue::JobQueue;
