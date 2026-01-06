//! Schema Contract System
//!
//! # Philosophy: Schema = Intent, then Contract
//!
//! The schema lifecycle in Casparian Flow:
//!
//! 1. **Discovery**: Parser analyzes files, proposes schema
//! 2. **Review**: User sees schema, can adjust columns/types
//! 3. **Approval**: User clicks "Approve" - schema becomes CONTRACT
//! 4. **Enforcement**: Parser must conform. Violations are failures.
//! 5. **Amendment**: Controlled evolution when data changes
//!
//! Once approved, there are NO silent fallbacks. No guessing. No coercion.
//! If the parser outputs data that doesn't match the contract, it FAILS.
//!
//! This ensures:
//! - Data quality: Users know exactly what they're getting
//! - Debuggability: Failures are explicit, not hidden in type coercion
//! - Trust: The contract is the contract
//!
//! # Modules
//!
//! - [`contract`]: Core types for schema contracts (LockedSchema, LockedColumn, etc.)
//! - [`storage`]: SQLite-backed persistence for contracts
//! - [`approval`]: Workflow for approving schemas and creating contracts
//! - [`amendment`]: Workflow for controlled schema evolution

pub mod amendment;
pub mod approval;
pub mod contract;
pub mod storage;

pub use contract::*;
pub use storage::SchemaStorage;

// Re-export key types from approval module
pub use approval::{
    ApprovalError, ApprovalResult, ApprovalWarning, ApprovedColumn, ApprovedSchemaVariant,
    QuestionAnswer, SchemaApprovalRequest, WarningType,
};

// Re-export key types from amendment module
pub use amendment::{
    AmendmentAction, AmendmentError, AmendmentReason, AmendmentResult, AmendmentStatus,
    SchemaAmendmentProposal, SchemaChange, SampleValue,
};
