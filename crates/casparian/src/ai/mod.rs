//! AI Wizards - Layer 2
//!
//! Build-time AI assistants that generate deterministic configuration and code.
//! All output is human-reviewable; nothing runs at runtime without approval.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    AI WIZARDS (Build-Time)                  │
//! │  ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌──────────┐ │
//! │  │Pathfinder │  │Parser Lab │  │ Labeling  │  │ Semantic │ │
//! │  │(YAML/Py)  │  │(Python)   │  │(Tag name) │  │  Path    │ │
//! │  └─────┬─────┘  └─────┬─────┘  └─────┬─────┘  └────┬─────┘ │
//! │        └──────────────┴──────────────┴─────────────┘       │
//! │                              │                              │
//! │                    ┌─────────▼─────────┐                    │
//! │                    │   Draft Manager   │                    │
//! │                    │   + Audit Log     │                    │
//! │                    └───────────────────┘                    │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Key Principles
//!
//! 1. **Build-time only**: AI assists during configuration, never at runtime
//! 2. **Human approval required**: All drafts must be explicitly approved
//! 3. **Audit everything**: Every LLM call is logged for debugging and compliance
//! 4. **YAML-first**: Prefer declarative rules over code where possible
//!
//! # Modules
//!
//! - [`types`]: Core type definitions (DraftId, DraftType, DraftStatus, etc.)
//! - [`draft`]: Draft management (create, approve, reject, cleanup)
//! - [`audit`]: Audit logging for all LLM interactions
//! - [`config`]: AI configuration from config.toml

pub mod audit;
pub mod config;
pub mod draft;
pub mod parser_lab;
pub mod pathfinder;
pub mod semantic_path;
pub mod types;

// Re-export commonly used types
pub use audit::AuditLogger;
pub use config::{AiConfig, AiProvider};
pub use draft::DraftManager;
pub use types::{Draft, DraftId, DraftStatus, DraftType, WizardType};
