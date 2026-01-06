//! Casparian MCP Server
//!
//! Model Context Protocol server for Claude Code integration.
//! Provides tools for:
//!
//! ## Discovery Tools
//! - `quick_scan` - Fast metadata scan of directories
//! - `apply_scope` - Group files into scopes for processing
//!
//! ## Schema Tools
//! - `discover_schemas` - Analyze files to infer schema structure
//! - `approve_schemas` - Create locked schema contracts
//! - `propose_amendment` - Modify existing contracts
//!
//! ## Backtest Tools
//! - `run_backtest` - Validate parser against multiple files
//! - `fix_parser` - Generate fixes based on failures
//!
//! ## Execution Tools
//! - `execute_pipeline` - Run full processing pipeline
//! - `query_output` - Query processed data
//!
//! # Usage
//!
//! ```rust,ignore
//! use casparian_mcp::tools::{create_default_registry, ToolRegistry};
//!
//! let registry = create_default_registry();
//! assert_eq!(registry.len(), 9);
//!
//! // Get a specific tool
//! if let Some(tool) = registry.get("quick_scan") {
//!     println!("Tool: {}", tool.name());
//!     println!("Description: {}", tool.description());
//! }
//! ```

pub mod protocol;
pub mod server;
pub mod tools;
pub mod types;

pub use server::McpServer;
pub use tools::{create_default_registry, register_all_tools, ToolRegistry};
pub use types::*;
