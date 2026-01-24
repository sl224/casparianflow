//! MCP tools for schema intent (§7.6)
//!
//! - `casp.schema.infer_intent` → Infer schema from parser output + derived fields
//! - `casp.schema.resolve_ambiguity` → Resolve type ambiguities

// Sync tool implementations (no async)
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::CoreHandle;
use crate::intent::confidence::compute_schema_column_confidence;
use crate::intent::session::SessionStore;
use crate::intent::state::IntentState;
use crate::intent::types::{
    CollisionResolution, ColumnCollision, ColumnConstraints, ColumnInference, ColumnSource,
    Confidence, ConfidenceLabel, FileSetId, HumanQuestion, InferenceEvidence, InferenceMethod,
    ProposalId, QuestionId, QuestionKind, QuestionOption, SchemaIntentColumn, SchemaIntentProposal,
    SchemaIntentSources, SchemaSafeDefaults, SessionId,
};
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::tools::McpTool;

// ============================================================================
// Schema Infer Intent Tool
// ============================================================================

/// Tool: casp.schema.infer_intent
pub struct SchemaInferIntentTool;

#[derive(Debug, Deserialize)]
struct SchemaInferIntentArgs {
    /// Session ID
    session_id: SessionId,
    /// Sample data from parser output (column name -> sample values)
    parser_output_sample: Vec<ColumnSample>,
    /// Derived fields from path analysis
    #[serde(default)]
    derived_fields: Vec<DerivedFieldSpec>,
    /// Safe default preferences
    #[serde(default)]
    safe_defaults: Option<SchemaSafeDefaults>,
}

#[derive(Debug, Deserialize)]
struct ColumnSample {
    name: String,
    #[serde(default)]
    values: Vec<serde_json::Value>,
    #[serde(default)]
    null_count: u64,
    #[serde(default)]
    total_count: u64,
}

#[derive(Debug, Deserialize)]
struct DerivedFieldSpec {
    name: String,
    dtype: String,
    #[serde(default)]
    examples: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SchemaInferIntentResponse {
    proposal_id: ProposalId,
    proposal_hash: String,
    columns: Vec<ColumnIntentSummary>,
    ambiguous_count: usize,
    collision_count: usize,
    required_human_questions: Vec<HumanQuestion>,
}

#[derive(Debug, Serialize)]
struct ColumnIntentSummary {
    name: String,
    source: ColumnSource,
    declared_type: String,
    nullable: bool,
    inference_method: InferenceMethod,
    candidates: Vec<String>,
    confidence: Confidence,
}

impl McpTool for SchemaInferIntentTool {
    fn name(&self) -> &'static str {
        "casp_schema_infer_intent"
    }

    fn description(&self) -> &'static str {
        "Infer schema intent from parser output samples and derived fields. Returns type candidates with confidence scores."
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
                "parser_output_sample": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "values": { "type": "array" },
                            "null_count": { "type": "integer" },
                            "total_count": { "type": "integer" }
                        },
                        "required": ["name"]
                    },
                    "description": "Sample data from parser output"
                },
                "derived_fields": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "dtype": { "type": "string" },
                            "examples": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["name", "dtype"]
                    },
                    "description": "Derived fields from path analysis"
                },
                "safe_defaults": {
                    "type": "object",
                    "properties": {
                        "timestamp_timezone": { "type": "string" },
                        "string_truncation": { "type": "string" },
                        "numeric_overflow": { "type": "string" }
                    }
                }
            },
            "required": ["session_id", "parser_output_sample"]
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
        let args: SchemaInferIntentArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Infer types for parsed columns
        let mut columns = Vec::new();
        let mut questions = Vec::new();

        for sample in &args.parser_output_sample {
            let (column, question) = infer_column_type(sample, ColumnSource::Parsed)?;
            columns.push(column);
            if let Some(q) = question {
                questions.push(q);
            }
        }

        // Add derived fields
        for derived in &args.derived_fields {
            columns.push(SchemaIntentColumn {
                name: derived.name.clone(),
                source: ColumnSource::Derived,
                declared_type: derived.dtype.clone(),
                nullable: false,
                constraints: ColumnConstraints {
                    enum_values: None,
                    min: None,
                    max: None,
                },
                inference: ColumnInference {
                    method: InferenceMethod::ConstraintElimination,
                    candidates: vec![derived.dtype.clone()],
                    evidence: InferenceEvidence {
                        null_rate: 0.0,
                        distinct: derived.examples.len() as u64,
                        format_hits: derived.examples.len() as u64,
                    },
                    confidence: Confidence::high(vec!["Derived from path pattern".to_string()]),
                },
            });
        }

        // Detect collisions
        let mut collisions = Vec::new();
        let parsed_names: std::collections::HashSet<_> = args
            .parser_output_sample
            .iter()
            .map(|c| c.name.clone())
            .collect();

        for derived in &args.derived_fields {
            let base_name = derived.name.trim_start_matches("_cf_path_");
            if parsed_names.contains(base_name) {
                collisions.push(ColumnCollision {
                    left: derived.name.clone(),
                    right: base_name.to_string(),
                    resolution: CollisionResolution::Namespace,
                });
            }
        }

        // Count ambiguous columns
        let ambiguous_count = columns
            .iter()
            .filter(|c| c.inference.method == InferenceMethod::AmbiguousRequiresHuman)
            .count();

        // Create proposal
        let proposal = SchemaIntentProposal {
            proposal_id: ProposalId::new(),
            proposal_hash: String::new(),
            input_sources: SchemaIntentSources {
                parser_output_sample_ref: None,
                derived_fields_ref: None,
            },
            columns: columns.clone(),
            column_collisions: collisions.clone(),
            safe_defaults: args.safe_defaults.unwrap_or_default(),
            required_human_questions: questions.clone(),
        };

        let proposal_hash = format!("{:x}", md5::compute(serde_json::to_string(&proposal)?));

        let mut proposal = proposal;
        proposal.proposal_hash = proposal_hash.clone();

        bundle.write_proposal("schema_intent", proposal.proposal_id, &proposal)?;
        bundle.update_state(IntentState::InferSchemaIntent)?;

        let response = SchemaInferIntentResponse {
            proposal_id: proposal.proposal_id,
            proposal_hash,
            columns: columns
                .iter()
                .map(|c| ColumnIntentSummary {
                    name: c.name.clone(),
                    source: c.source.clone(),
                    declared_type: c.declared_type.clone(),
                    nullable: c.nullable,
                    inference_method: c.inference.method.clone(),
                    candidates: c.inference.candidates.clone(),
                    confidence: c.inference.confidence.clone(),
                })
                .collect(),
            ambiguous_count,
            collision_count: collisions.len(),
            required_human_questions: questions,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Schema Resolve Ambiguity Tool
// ============================================================================

/// Tool: casp.schema.resolve_ambiguity
pub struct SchemaResolveAmbiguityTool;

#[derive(Debug, Deserialize)]
struct SchemaResolveAmbiguityArgs {
    /// Session ID
    session_id: SessionId,
    /// Proposal ID
    proposal_id: ProposalId,
    /// Resolutions: column name -> chosen type
    resolutions: std::collections::HashMap<String, String>,
    /// Approval token
    approval_token_hash: String,
}

#[derive(Debug, Serialize)]
struct SchemaResolveAmbiguityResponse {
    resolved: bool,
    resolved_columns: Vec<String>,
    new_state: String,
}

impl McpTool for SchemaResolveAmbiguityTool {
    fn name(&self) -> &'static str {
        "casp_schema_resolve_ambiguity"
    }

    fn description(&self) -> &'static str {
        "Resolve schema type ambiguities. Requires approval token (Gate G4)."
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
                "proposal_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "Proposal ID"
                },
                "resolutions": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Map of column name to chosen type"
                },
                "approval_token_hash": {
                    "type": "string",
                    "description": "Approval token hash"
                }
            },
            "required": ["session_id", "proposal_id", "resolutions", "approval_token_hash"]
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
        let args: SchemaResolveAmbiguityArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        let proposal: SchemaIntentProposal =
            bundle.read_proposal("schema_intent", args.proposal_id)?;

        if args.approval_token_hash != proposal.proposal_hash {
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
            choice_payload: serde_json::json!({ "resolutions": args.resolutions }),
            notes: Some("Schema ambiguities resolved via MCP".to_string()),
        };
        bundle.append_decision(&decision)?;

        bundle.update_state(IntentState::AwaitingSchemaApproval)?;

        let response = SchemaResolveAmbiguityResponse {
            resolved: true,
            resolved_columns: args.resolutions.keys().cloned().collect(),
            new_state: IntentState::GenerateParserDraft.as_str().to_string(),
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn infer_column_type(
    sample: &ColumnSample,
    source: ColumnSource,
) -> anyhow::Result<(SchemaIntentColumn, Option<HumanQuestion>)> {
    let null_rate = if sample.total_count > 0 {
        sample.null_count as f64 / sample.total_count as f64
    } else {
        0.0
    };

    let nullable = null_rate > 0.0;

    // Analyze values to determine type
    let non_null_values: Vec<_> = sample.values.iter().filter(|v| !v.is_null()).collect();

    let (candidates, method) = if non_null_values.is_empty() {
        (
            vec!["string".to_string()],
            InferenceMethod::AmbiguousRequiresHuman,
        )
    } else {
        analyze_value_types(&non_null_values)
    };

    let declared_type = candidates
        .first()
        .cloned()
        .unwrap_or_else(|| "string".to_string());

    let inference = ColumnInference {
        method: method.clone(),
        candidates: candidates.clone(),
        evidence: InferenceEvidence {
            null_rate,
            distinct: non_null_values.len() as u64,
            format_hits: non_null_values.len() as u64,
        },
        confidence: if method == InferenceMethod::ConstraintElimination {
            Confidence::high(vec!["Single type candidate".to_string()])
        } else {
            Confidence::low(vec!["Multiple type candidates".to_string()])
        },
    };

    let column = SchemaIntentColumn {
        name: sample.name.clone(),
        source,
        declared_type,
        nullable,
        constraints: ColumnConstraints {
            enum_values: None,
            min: None,
            max: None,
        },
        inference,
    };

    // Generate question if ambiguous
    let question = if method == InferenceMethod::AmbiguousRequiresHuman && candidates.len() > 1 {
        Some(HumanQuestion {
            question_id: QuestionId::new(),
            kind: QuestionKind::ResolveAmbiguity,
            prompt: format!(
                "Column '{}' has ambiguous type. Which type should be used?",
                sample.name
            ),
            options: candidates
                .iter()
                .map(|t| QuestionOption {
                    option_id: t.clone(),
                    label: t.clone(),
                    consequence: format!("Column will be typed as {}", t),
                    default: false,
                })
                .collect(),
            evidence_refs: vec![],
            deadline: None,
        })
    } else {
        None
    };

    Ok((column, question))
}

fn analyze_value_types(values: &[&serde_json::Value]) -> (Vec<String>, InferenceMethod) {
    let mut could_be_int = true;
    let mut could_be_float = true;
    let mut could_be_bool = true;
    let mut could_be_date = true;
    let mut could_be_timestamp = true;

    for value in values {
        match value {
            serde_json::Value::Number(n) => {
                could_be_bool = false;
                could_be_date = false;
                could_be_timestamp = false;
                if n.is_f64() && n.as_f64().map(|f| f.fract() != 0.0).unwrap_or(false) {
                    could_be_int = false;
                }
            }
            serde_json::Value::Bool(_) => {
                could_be_int = false;
                could_be_float = false;
                could_be_date = false;
                could_be_timestamp = false;
            }
            serde_json::Value::String(s) => {
                // Check formats
                if s.parse::<i64>().is_err() {
                    could_be_int = false;
                }
                if s.parse::<f64>().is_err() {
                    could_be_float = false;
                }
                if !matches!(s.to_lowercase().as_str(), "true" | "false" | "1" | "0") {
                    could_be_bool = false;
                }
                if !looks_like_date(s) {
                    could_be_date = false;
                }
                if !looks_like_timestamp(s) {
                    could_be_timestamp = false;
                }
            }
            _ => {
                could_be_int = false;
                could_be_float = false;
                could_be_bool = false;
                could_be_date = false;
                could_be_timestamp = false;
            }
        }
    }

    let mut candidates = Vec::new();

    if could_be_bool {
        candidates.push("boolean".to_string());
    }
    if could_be_int {
        candidates.push("int64".to_string());
    }
    if could_be_float && !could_be_int {
        candidates.push("float64".to_string());
    }
    if could_be_date {
        candidates.push("date".to_string());
    }
    if could_be_timestamp && !could_be_date {
        candidates.push("timestamp".to_string());
    }

    if candidates.is_empty() {
        candidates.push("string".to_string());
    }

    let method = if candidates.len() == 1 {
        InferenceMethod::ConstraintElimination
    } else {
        InferenceMethod::AmbiguousRequiresHuman
    };

    (candidates, method)
}

fn looks_like_date(s: &str) -> bool {
    let patterns = [
        r"^\d{4}-\d{2}-\d{2}$",
        r"^\d{2}/\d{2}/\d{4}$",
        r"^\d{2}-\d{2}-\d{4}$",
    ];

    patterns
        .iter()
        .any(|p| regex::Regex::new(p).map(|r| r.is_match(s)).unwrap_or(false))
}

fn looks_like_timestamp(s: &str) -> bool {
    let patterns = [
        r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}",
        r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}",
    ];

    patterns
        .iter()
        .any(|p| regex::Regex::new(p).map(|r| r.is_match(s)).unwrap_or(false))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_int_values() {
        let v1 = serde_json::json!(1);
        let v2 = serde_json::json!(2);
        let v3 = serde_json::json!(3);
        let values = vec![&v1, &v2, &v3];
        let (candidates, method) = analyze_value_types(&values);
        assert!(candidates.contains(&"int64".to_string()));
        assert_eq!(method, InferenceMethod::ConstraintElimination);
    }

    #[test]
    fn test_analyze_float_values() {
        let v1 = serde_json::json!(1.5);
        let v2 = serde_json::json!(2.7);
        let values = vec![&v1, &v2];
        let (candidates, _) = analyze_value_types(&values);
        assert!(candidates.contains(&"float64".to_string()));
    }

    #[test]
    fn test_looks_like_date() {
        assert!(looks_like_date("2024-01-15"));
        assert!(looks_like_date("01/15/2024"));
        assert!(!looks_like_date("hello"));
        assert!(!looks_like_date("2024-01-15T10:30:00"));
    }

    #[test]
    fn test_looks_like_timestamp() {
        assert!(looks_like_timestamp("2024-01-15T10:30:00"));
        assert!(looks_like_timestamp("2024-01-15 10:30:00"));
        assert!(!looks_like_timestamp("2024-01-15"));
    }
}
