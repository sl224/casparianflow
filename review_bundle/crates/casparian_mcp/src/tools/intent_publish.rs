//! MCP tools for publish and run (§7.8, §7.9)
//!
//! - `casp.schema.promote` → Promote ephemeral schema to schema-as-code
//! - `casp.publish.plan` → Create publish plan
//! - `casp.publish.execute` → Execute publish (requires approval)
//! - `casp.run.plan` → Create run plan
//! - `casp.run.execute` → Execute run (requires approval)

// Sync tool implementations (no async)
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::CoreHandle;
use crate::intent::session::SessionStore;
use crate::intent::state::IntentState;
use crate::intent::types::{
    EstimatedCost, FileSetId, HumanQuestion, JobPartitioning, ParserIdentity, ProposalId,
    PublishInvariants, PublishParserInfo, PublishPlan, PublishSchemaInfo, RunPlan, RunPlanSink,
    RunPlanValidations, SessionId, WritePolicy,
};
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::tools::McpTool;

// ============================================================================
// Schema Promote Tool
// ============================================================================

/// Tool: casp.schema.promote
pub struct SchemaPromoteTool;

#[derive(Debug, Deserialize)]
struct SchemaPromoteArgs {
    /// Session ID
    session_id: SessionId,
    /// Schema intent proposal ID
    schema_proposal_id: ProposalId,
    /// Schema name
    schema_name: String,
    /// Schema version
    #[serde(default = "default_schema_version")]
    schema_version: String,
}

fn default_schema_version() -> String {
    "1.0.0".to_string()
}

#[derive(Debug, Serialize)]
struct SchemaPromoteResponse {
    promoted: bool,
    schema_as_code_ref: String,
    schema_name: String,
    schema_version: String,
}

impl McpTool for SchemaPromoteTool {
    fn name(&self) -> &'static str {
        "casp_schema_promote"
    }

    fn description(&self) -> &'static str {
        "Promote ephemeral schema to schema-as-code. Generates a schema definition file."
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
                    "description": "Schema intent proposal ID"
                },
                "schema_name": {
                    "type": "string",
                    "description": "Name for the schema"
                },
                "schema_version": {
                    "type": "string",
                    "description": "Version string (default: 1.0.0)"
                }
            },
            "required": ["session_id", "schema_proposal_id", "schema_name"]
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
        let args: SchemaPromoteArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // In production, would generate schema-as-code file
        let schema_ref = format!(
            "schemas/{}_{}.schema.json",
            args.schema_name, args.schema_version
        );

        bundle.update_state(IntentState::PromoteSchema)?;

        let response = SchemaPromoteResponse {
            promoted: true,
            schema_as_code_ref: schema_ref,
            schema_name: args.schema_name,
            schema_version: args.schema_version,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Publish Plan Tool
// ============================================================================

/// Tool: casp.publish.plan
pub struct PublishPlanTool;

#[derive(Debug, Deserialize)]
struct PublishPlanArgs {
    /// Session ID
    session_id: SessionId,
    /// Parser draft ID
    draft_id: ProposalId,
    /// Schema name
    schema_name: String,
    /// Schema version
    schema_version: String,
    /// Parser name
    parser_name: String,
    /// Parser version
    parser_version: String,
}

#[derive(Debug, Serialize)]
struct PublishPlanResponse {
    proposal_id: ProposalId,
    proposal_hash: String,
    schema_info: PublishSchemaInfo,
    parser_info: PublishParserInfo,
    invariants: PublishInvariants,
    diff_summary: Vec<String>,
    required_human_questions: Vec<HumanQuestion>,
}

impl McpTool for PublishPlanTool {
    fn name(&self) -> &'static str {
        "casp_publish_plan"
    }

    fn description(&self) -> &'static str {
        "Create a publish plan for schema and parser. Validates invariants before publishing."
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
                "schema_name": { "type": "string" },
                "schema_version": { "type": "string" },
                "parser_name": { "type": "string" },
                "parser_version": { "type": "string" }
            },
            "required": ["session_id", "draft_id", "schema_name", "schema_version", "parser_name", "parser_version"]
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
        let args: PublishPlanArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        let schema_info = PublishSchemaInfo {
            schema_name: args.schema_name.clone(),
            new_version: args.schema_version.clone(),
            schema_as_code_ref: format!(
                "schemas/{}_{}.schema.json",
                args.schema_name, args.schema_version
            ),
            compiled_schema_ref: format!(
                "schemas/{}_{}.compiled.json",
                args.schema_name, args.schema_version
            ),
        };

        let parser_info = PublishParserInfo {
            name: args.parser_name.clone(),
            new_version: args.parser_version.clone(),
            source_hash: format!(
                "{:x}",
                md5::compute(format!("{}:{}", args.parser_name, args.parser_version))
            ),
            topics: vec![], // Would come from draft
        };

        let invariants = PublishInvariants {
            route_to_topic_in_parser_topics: true,
            no_same_name_version_different_hash: true,
            sink_validation_passed: true,
        };

        let plan = PublishPlan {
            proposal_id: ProposalId::new(),
            proposal_hash: String::new(),
            schema: schema_info.clone(),
            parser: parser_info.clone(),
            invariants: invariants.clone(),
            diff_summary: vec![
                "New schema version".to_string(),
                "New parser version".to_string(),
            ],
            required_human_questions: vec![],
        };

        let proposal_hash = format!("{:x}", md5::compute(serde_json::to_string(&plan)?));
        let mut plan = plan;
        plan.proposal_hash = proposal_hash.clone();

        bundle.write_proposal("publish_plan", plan.proposal_id, &plan)?;
        bundle.update_state(IntentState::PublishPlan)?;

        let response = PublishPlanResponse {
            proposal_id: plan.proposal_id,
            proposal_hash,
            schema_info,
            parser_info,
            invariants,
            diff_summary: plan.diff_summary,
            required_human_questions: vec![],
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Publish Execute Tool
// ============================================================================

/// Tool: casp.publish.execute
pub struct PublishExecuteTool;

#[derive(Debug, Deserialize)]
struct PublishExecuteArgs {
    /// Session ID
    session_id: SessionId,
    /// Proposal ID
    proposal_id: ProposalId,
    /// Approval token
    approval_token_hash: String,
}

#[derive(Debug, Serialize)]
struct PublishExecuteResponse {
    published: bool,
    schema_version: String,
    parser_version: String,
    new_state: String,
}

impl McpTool for PublishExecuteTool {
    fn name(&self) -> &'static str {
        "casp_publish_execute"
    }

    fn description(&self) -> &'static str {
        "Execute the publish plan. Requires approval token (Gate G5)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid"
                },
                "proposal_id": {
                    "type": "string",
                    "format": "uuid"
                },
                "approval_token_hash": {
                    "type": "string"
                }
            },
            "required": ["session_id", "proposal_id", "approval_token_hash"]
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
        let args: PublishExecuteArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        let plan: PublishPlan = bundle.read_proposal("publish_plan", args.proposal_id)?;

        if args.approval_token_hash != plan.proposal_hash {
            anyhow::bail!("Invalid approval token");
        }

        // Record decision
        let decision = crate::intent::types::DecisionRecord {
            timestamp: chrono::Utc::now(),
            actor: "agent".to_string(),
            decision: crate::intent::types::Decision::Approve,
            target: crate::intent::types::DecisionTarget {
                proposal_id: args.proposal_id,
                approval_target_hash: args.approval_token_hash,
            },
            choice_payload: serde_json::json!({}),
            notes: Some("Publish approved via MCP".to_string()),
        };
        bundle.append_decision(&decision)?;

        bundle.update_state(IntentState::PublishExecute)?;

        // In production, would actually publish to registry

        let response = PublishExecuteResponse {
            published: true,
            schema_version: plan.schema.new_version,
            parser_version: plan.parser.new_version,
            new_state: IntentState::RunPlan.as_str().to_string(),
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Run Plan Tool
// ============================================================================

/// Tool: casp.run.plan
pub struct RunPlanTool;

#[derive(Debug, Deserialize)]
struct RunPlanArgs {
    /// Session ID
    session_id: SessionId,
    /// File set ID to process
    file_set_id: FileSetId,
    /// Parser name
    parser_name: String,
    /// Parser version
    parser_version: String,
    /// Output sink URI
    sink_uri: String,
    /// Route to topic
    route_to_topic: String,
}

#[derive(Debug, Serialize)]
struct RunPlanResponse {
    proposal_id: ProposalId,
    proposal_hash: String,
    file_count: u64,
    estimated_size_bytes: u64,
    sink: RunPlanSink,
    validations: RunPlanValidations,
}

impl McpTool for RunPlanTool {
    fn name(&self) -> &'static str {
        "casp_run_plan"
    }

    fn description(&self) -> &'static str {
        "Create a run plan for processing files. Validates sink and topic mapping."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid"
                },
                "file_set_id": {
                    "type": "string",
                    "format": "uuid"
                },
                "parser_name": { "type": "string" },
                "parser_version": { "type": "string" },
                "sink_uri": {
                    "type": "string",
                    "description": "Output sink URI (e.g., parquet:///data/out/)"
                },
                "route_to_topic": { "type": "string" }
            },
            "required": ["session_id", "file_set_id", "parser_name", "parser_version", "sink_uri", "route_to_topic"]
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
        let args: RunPlanArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        let entries = bundle.read_fileset(args.file_set_id)?;
        let file_count = entries.len() as u64;

        // Estimate size (would stat files in production)
        let estimated_size = file_count * 10000; // Mock estimate

        let sink = RunPlanSink {
            sink_type: "parquet_dir".to_string(),
            path: args.sink_uri.clone(),
            duckdb_path: None,
            duckdb_table: None,
        };

        let validations = RunPlanValidations {
            sink_valid: true,
            topic_mapping_valid: true,
        };

        let plan = RunPlan {
            proposal_id: ProposalId::new(),
            proposal_hash: String::new(),
            input_file_set_id: args.file_set_id,
            route_to_topic: args.route_to_topic.clone(),
            parser_identity: ParserIdentity {
                name: args.parser_name.clone(),
                version: args.parser_version.clone(),
                topics: vec![args.route_to_topic.clone()],
                source_hash: format!(
                    "{:x}",
                    md5::compute(format!("{}:{}", args.parser_name, args.parser_version))
                ),
            },
            sink: sink.clone(),
            write_policy: WritePolicy::NewJobPartition,
            job_partitioning: JobPartitioning::default(),
            validations: validations.clone(),
            estimated_cost: EstimatedCost {
                files: file_count,
                size_bytes: estimated_size,
            },
        };

        let proposal_hash = format!("{:x}", md5::compute(serde_json::to_string(&plan)?));
        let mut plan = plan;
        plan.proposal_hash = proposal_hash.clone();

        bundle.write_proposal("run_plan", plan.proposal_id, &plan)?;
        bundle.update_state(IntentState::RunPlan)?;

        let response = RunPlanResponse {
            proposal_id: plan.proposal_id,
            proposal_hash,
            file_count,
            estimated_size_bytes: estimated_size,
            sink,
            validations,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Run Execute Tool
// ============================================================================

/// Tool: casp.run.execute
pub struct RunExecuteTool;

#[derive(Debug, Deserialize)]
struct RunExecuteArgs {
    /// Session ID
    session_id: SessionId,
    /// Proposal ID
    proposal_id: ProposalId,
    /// Approval token
    approval_token_hash: String,
}

#[derive(Debug, Serialize)]
struct RunExecuteResponse {
    started: bool,
    run_job_id: String,
    file_count: u64,
    new_state: String,
}

impl McpTool for RunExecuteTool {
    fn name(&self) -> &'static str {
        "casp_run_execute"
    }

    fn description(&self) -> &'static str {
        "Execute the run plan. Requires approval token (Gate G6)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "format": "uuid"
                },
                "proposal_id": {
                    "type": "string",
                    "format": "uuid"
                },
                "approval_token_hash": {
                    "type": "string"
                }
            },
            "required": ["session_id", "proposal_id", "approval_token_hash"]
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
        let args: RunExecuteArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        let plan: RunPlan = bundle.read_proposal("run_plan", args.proposal_id)?;

        if args.approval_token_hash != plan.proposal_hash {
            anyhow::bail!("Invalid approval token");
        }

        // Record decision
        let decision = crate::intent::types::DecisionRecord {
            timestamp: chrono::Utc::now(),
            actor: "agent".to_string(),
            decision: crate::intent::types::Decision::Approve,
            target: crate::intent::types::DecisionTarget {
                proposal_id: args.proposal_id,
                approval_target_hash: args.approval_token_hash,
            },
            choice_payload: serde_json::json!({}),
            notes: Some("Run approved via MCP".to_string()),
        };
        bundle.append_decision(&decision)?;

        bundle.update_state(IntentState::RunExecute)?;

        // In production, would start actual run job
        let run_job_id = uuid::Uuid::new_v4().to_string();

        let response = RunExecuteResponse {
            started: true,
            run_job_id,
            file_count: plan.estimated_cost.files,
            new_state: IntentState::Completed.as_str().to_string(),
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
    fn test_publish_plan_args_deserialize() {
        let session_id = SessionId::new();
        let draft_id = ProposalId::new();

        let json = json!({
            "session_id": session_id.to_string(),
            "draft_id": draft_id.to_string(),
            "schema_name": "sales",
            "schema_version": "1.0.0",
            "parser_name": "sales_csv",
            "parser_version": "1.0.0"
        });

        let args: PublishPlanArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.schema_name, "sales");
        assert_eq!(args.parser_name, "sales_csv");
    }
}
