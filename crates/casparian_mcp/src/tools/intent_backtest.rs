//! MCP tools for backtest loop (§7.7)
//!
//! - `casp.parser.generate_draft` → Generate parser draft
//! - `casp.backtest.start` → Start backtest job
//! - `casp.backtest.status` → Get backtest progress
//! - `casp.backtest.report` → Get backtest report
//! - `casp.patch.apply` → Apply a patch (schema/parser/rule)

// Sync tool implementations (no async)
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::CoreHandle;
use crate::intent::session::SessionStore;
use crate::intent::state::IntentState;
use crate::intent::types::{
    BacktestMetrics, BacktestPhase, BacktestProgressEnvelope, BacktestQuality, BacktestReport,
    BuildStatus, FileSetId, ParserDraft, ParserIdentity, ProposalId, SessionId, TopKViolation,
    ViolationSummaryEntry, ViolationTopColumn,
};
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::tools::McpTool;

// ============================================================================
// Parser Generate Draft Tool
// ============================================================================

/// Tool: casp.parser.generate_draft
pub struct ParserGenerateDraftTool;

#[derive(Debug, Deserialize)]
struct ParserGenerateDraftArgs {
    /// Session ID
    session_id: SessionId,
    /// Schema intent proposal ID
    schema_proposal_id: ProposalId,
    /// Parser name
    parser_name: String,
    /// Parser version
    #[serde(default = "default_version")]
    parser_version: String,
    /// Topics to subscribe to
    #[serde(default)]
    topics: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

#[derive(Debug, Serialize)]
struct ParserGenerateDraftResponse {
    draft_id: ProposalId,
    parser_identity: ParserIdentity,
    build_status: BuildStatus,
    lint_status: BuildStatus,
    repo_ref: String,
}

impl McpTool for ParserGenerateDraftTool {
    fn name(&self) -> &'static str {
        "casp_parser_generate_draft"
    }

    fn description(&self) -> &'static str {
        "Generate a parser draft based on the schema intent. Returns draft ID and build status."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID"
                },
                "schema_proposal_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Schema intent proposal ID to base parser on"
                },
                "parser_name": {
                    "type": "string",
                    "description": "Name for the parser"
                },
                "parser_version": {
                    "type": "string",
                    "description": "Version string (default: 0.1.0)"
                },
                "topics": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Topics this parser subscribes to"
                }
            },
            "required": ["session_id", "schema_proposal_id", "parser_name"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: ParserGenerateDraftArgs = serde_json::from_value(args)?;
        let _ = &args.schema_proposal_id;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // In production, would actually generate parser code based on schema
        // For now, create a draft record

        let source_hash = format!(
            "{:x}",
            md5::compute(format!("{}:{}", args.parser_name, args.parser_version))
        );

        let parser_identity = ParserIdentity {
            name: args.parser_name,
            version: args.parser_version,
            topics: args.topics,
            source_hash,
        };

        let draft = ParserDraft {
            draft_id: ProposalId::new(),
            parser_identity: parser_identity.clone(),
            repo_ref: format!("sessions/{}/parser_draft", args.session_id),
            entrypoints: vec!["parse".to_string()],
            tests_ref: None,
            build_status: BuildStatus::Pass,
            lint_status: BuildStatus::Pass,
        };

        bundle.write_proposal("parser_draft", draft.draft_id, &draft)?;
        bundle.update_state(IntentState::GenerateParserDraft)?;

        let response = ParserGenerateDraftResponse {
            draft_id: draft.draft_id,
            parser_identity,
            build_status: draft.build_status,
            lint_status: draft.lint_status,
            repo_ref: draft.repo_ref,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Backtest Start Tool
// ============================================================================

/// Tool: casp.backtest.start
pub struct IntentBacktestStartTool;

#[derive(Debug, Deserialize)]
struct IntentBacktestStartArgs {
    /// Session ID
    session_id: SessionId,
    /// Parser draft ID
    draft_id: ProposalId,
    /// File set to backtest against
    file_set_id: FileSetId,
    /// Use fail-fast mode
    #[serde(default = "default_true")]
    fail_fast: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
struct IntentBacktestStartResponse {
    backtest_job_id: String,
    file_set_id: FileSetId,
    fail_fast: bool,
}

impl McpTool for IntentBacktestStartTool {
    fn name(&self) -> &'static str {
        "casp_intent_backtest_start"
    }

    fn description(&self) -> &'static str {
        "Start a backtest job for the parser draft against a file set."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID"
                },
                "draft_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Parser draft ID"
                },
                "file_set_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "File set ID to backtest against"
                },
                "fail_fast": {
                    "type": "boolean",
                    "description": "Use fail-fast mode (default: true)"
                }
            },
            "required": ["session_id", "draft_id", "file_set_id"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: IntentBacktestStartArgs = serde_json::from_value(args)?;
        let _ = &args.draft_id;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Create a backtest job ID
        let backtest_job_id = uuid::Uuid::new_v4().to_string();

        bundle.update_state(IntentState::BacktestFailFast)?;

        // In production, would kick off actual backtest job
        // For now, just record the intent

        let response = IntentBacktestStartResponse {
            backtest_job_id,
            file_set_id: args.file_set_id,
            fail_fast: args.fail_fast,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Backtest Status Tool
// ============================================================================

/// Tool: casp.backtest.status
pub struct IntentBacktestStatusTool;

#[derive(Debug, Deserialize)]
struct IntentBacktestStatusArgs {
    /// Session ID
    session_id: SessionId,
    /// Backtest job ID
    backtest_job_id: String,
}

impl McpTool for IntentBacktestStatusTool {
    fn name(&self) -> &'static str {
        "casp_intent_backtest_status"
    }

    fn description(&self) -> &'static str {
        "Get the status of a backtest job. Returns bounded progress envelope."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID"
                },
                "backtest_job_id": {
                    "type": "string",
                    "description": "Backtest job ID"
                }
            },
            "required": ["session_id", "backtest_job_id"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: IntentBacktestStatusArgs = serde_json::from_value(args)?;
        let _ = &args.session_id;

        // In production, would query actual job status
        // For now, return mock progress

        let progress = BacktestProgressEnvelope {
            job_id: args.backtest_job_id,
            phase: BacktestPhase::Validate,
            elapsed_ms: 5000,
            metrics: BacktestMetrics {
                files_processed: 50,
                files_total_estimate: Some(100),
                rows_emitted: 10000,
                rows_quarantined: 100,
            },
            top_violation_summary: vec![ViolationSummaryEntry {
                violation_type: "TypeMismatch".to_string(),
                count: 50,
                top_columns: vec![ViolationTopColumn {
                    name: "amount".to_string(),
                    count: 30,
                }],
            }],
            stalled: false,
        };

        Ok(serde_json::to_value(progress)?)
    }
}

// ============================================================================
// Backtest Report Tool
// ============================================================================

/// Tool: casp.backtest.report
pub struct IntentBacktestReportTool;

#[derive(Debug, Deserialize)]
struct IntentBacktestReportArgs {
    /// Session ID
    session_id: SessionId,
    /// Backtest job ID
    backtest_job_id: String,
}

impl McpTool for IntentBacktestReportTool {
    fn name(&self) -> &'static str {
        "casp_intent_backtest_report"
    }

    fn description(&self) -> &'static str {
        "Get the backtest report with top-K violations and quality metrics."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID"
                },
                "backtest_job_id": {
                    "type": "string",
                    "description": "Backtest job ID"
                }
            },
            "required": ["session_id", "backtest_job_id"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: IntentBacktestReportArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // In production, would read actual report
        // For now, return mock report

        let report = BacktestReport {
            job_id: args.backtest_job_id.clone(),
            input_file_set_id: FileSetId::new(),
            iterations_ref: format!("reports/backtest_iters_{}.jsonl", args.backtest_job_id),
            quality: BacktestQuality {
                files_processed: 100,
                rows_emitted: 20000,
                rows_quarantined: 200,
                quarantine_pct: 1.0,
                pass_rate_files: 0.98,
            },
            top_k_violations: vec![TopKViolation {
                violation_type: "TypeMismatch".to_string(),
                count: 100,
                top_columns: vec![ViolationTopColumn {
                    name: "amount".to_string(),
                    count: 60,
                }],
                example_contexts: vec![],
            }],
            full_report_ref: format!("reports/backtest_{}.json", args.backtest_job_id),
        };

        // Save report
        bundle.write_report("backtest", &args.backtest_job_id, &report)?;

        Ok(serde_json::to_value(report)?)
    }
}

// ============================================================================
// Patch Apply Tool
// ============================================================================

/// Tool: casp.patch.apply
pub struct PatchApplyTool;

#[derive(Debug, Deserialize)]
struct PatchApplyArgs {
    /// Session ID
    session_id: SessionId,
    /// Patch type (schema, parser, rule)
    patch_type: String,
    /// Patch content
    patch_content: serde_json::Value,
    /// Iteration ID
    iteration_id: String,
}

#[derive(Debug, Serialize)]
struct PatchApplyResponse {
    applied: bool,
    patch_ref: String,
    next_action: String,
}

impl McpTool for PatchApplyTool {
    fn name(&self) -> &'static str {
        "casp_patch_apply"
    }

    fn description(&self) -> &'static str {
        "Apply a single patch (schema/parser/rule) for the backtest loop. Only one patch type per iteration."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Session ID"
                },
                "patch_type": {
                    "type": "string",
                    "enum": ["schema", "parser", "rule"],
                    "description": "Type of patch to apply"
                },
                "patch_content": {
                    "type": "object",
                    "description": "Patch content (varies by type)"
                },
                "iteration_id": {
                    "type": "string",
                    "description": "Iteration identifier"
                }
            },
            "required": ["session_id", "patch_type", "patch_content", "iteration_id"]
        })
    }

    fn execute(
        &self,
        args: Value,
        _security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: PatchApplyArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Write patch to session bundle
        let patch_kind = format!("{}_patch", args.patch_type);
        let patch_bytes = serde_json::to_vec_pretty(&args.patch_content)?;
        let patch_ref = bundle.write_patch(&patch_kind, &args.iteration_id, &patch_bytes)?;

        let response = PatchApplyResponse {
            applied: true,
            patch_ref,
            next_action: "re-run backtest".to_string(),
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_draft_args_deserialize() {
        let session_id = SessionId::new();
        let schema_proposal_id = ProposalId::new();

        let json = json!({
            "session_id": session_id.to_string(),
            "schema_proposal_id": schema_proposal_id.to_string(),
            "parser_name": "sales_csv",
            "topics": ["sales_ingest"]
        });

        let args: ParserGenerateDraftArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.parser_name, "sales_csv");
        assert_eq!(args.topics, vec!["sales_ingest"]);
    }
}
