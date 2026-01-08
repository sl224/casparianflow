//! Casparian Flow Security Module
//!
//! Provides:
//! - **Gatekeeper**: AST-based Python code validation
//! - **Azure Provider**: Device Code Flow authentication
//! - **Signing**: Ed25519 signature generation and verification

pub mod gatekeeper;
pub mod azure;
pub mod signing;

pub use gatekeeper::Gatekeeper;
pub use azure::AzureProvider;
