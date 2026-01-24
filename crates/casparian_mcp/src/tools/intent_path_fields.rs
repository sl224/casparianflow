//! MCP tools for path-derived fields (§7.5)
//!
//! - `casp.path_fields.propose` → Propose path-derived fields
//! - `casp.path_fields.apply` → Apply approved path field configuration

// Sync tool implementations (no async)
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use crate::core::CoreHandle;
use crate::intent::confidence::compute_path_field_confidence;
use crate::intent::session::SessionStore;
use crate::intent::state::IntentState;
use crate::intent::types::{
    Confidence, FileSetId, HumanQuestion, ParsedColumnCollision, PathField, PathFieldCollisions,
    PathFieldCoverage, PathFieldDtype, PathFieldNamespacing, PathFieldPattern, PathFieldProposal,
    PathFieldSource, ProposalId, QuestionId, QuestionKind, QuestionOption, SessionId,
};
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::tools::McpTool;

// ============================================================================
// Path Fields Propose Tool
// ============================================================================

/// Tool: casp.path_fields.propose
pub struct PathFieldsProposeTool;

#[derive(Debug, Deserialize)]
struct PathFieldsProposeArgs {
    /// Session ID
    session_id: SessionId,
    /// File set ID to analyze
    file_set_id: FileSetId,
    /// Known parsed column names (to detect collisions)
    #[serde(default)]
    parsed_columns: Vec<String>,
    /// Namespacing configuration
    #[serde(default)]
    namespacing: Option<PathFieldNamespacing>,
}

#[derive(Debug, Serialize)]
struct PathFieldsProposeResponse {
    proposal_id: ProposalId,
    proposal_hash: String,
    fields: Vec<PathFieldSummary>,
    collisions: PathFieldCollisionsSummary,
    required_human_questions: Vec<HumanQuestion>,
}

#[derive(Debug, Serialize)]
struct PathFieldSummary {
    field_name: String,
    dtype: PathFieldDtype,
    pattern_kind: String,
    coverage_pct: f64,
    confidence: Confidence,
    examples: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PathFieldCollisionsSummary {
    same_name_count: usize,
    segment_overlap_count: usize,
    parsed_column_collision_count: usize,
}

impl McpTool for PathFieldsProposeTool {
    fn name(&self) -> &'static str {
        "casp_path_fields_propose"
    }

    fn description(&self) -> &'static str {
        "Propose path-derived fields from file paths. Detects key=value patterns, partitions, and segment positions."
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
                "file_set_id": {
                    "type": "string",
                    "format": "uuid",
                    "description": "File set ID to analyze"
                },
                "parsed_columns": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Known parsed column names (to detect collisions)"
                },
                "namespacing": {
                    "type": "object",
                    "properties": {
                        "default_prefix": {
                            "type": "string",
                            "description": "Prefix for derived field names (default: '_cf_path_')"
                        },
                        "allow_promote": {
                            "type": "boolean",
                            "description": "Allow promoting fields to schema without prefix"
                        }
                    }
                }
            },
            "required": ["session_id", "file_set_id"]
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
        let args: PathFieldsProposeArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Read the file set
        let entries = bundle.read_fileset(args.file_set_id)?;
        let total_files = entries.len() as u64;

        // Extract path-derived fields
        let (fields, collisions, questions) = extract_path_fields(
            &entries,
            &args.parsed_columns,
            args.namespacing.as_ref(),
            total_files,
        )?;

        // Create proposal
        let proposal = PathFieldProposal {
            proposal_id: ProposalId::new(),
            proposal_hash: String::new(),
            input_file_set_id: args.file_set_id,
            namespacing: args.namespacing.unwrap_or_default(),
            fields: fields.clone(),
            collisions: collisions.clone(),
            required_human_questions: questions.clone(),
        };

        // Compute hash
        let proposal_hash = format!("{:x}", md5::compute(serde_json::to_string(&proposal)?));

        let mut proposal = proposal;
        proposal.proposal_hash = proposal_hash.clone();

        bundle.write_proposal("path_fields", proposal.proposal_id, &proposal)?;
        bundle.update_state(IntentState::ProposePathFields)?;

        let response = PathFieldsProposeResponse {
            proposal_id: proposal.proposal_id,
            proposal_hash,
            fields: fields
                .iter()
                .map(|f| PathFieldSummary {
                    field_name: f.field_name.clone(),
                    dtype: f.dtype.clone(),
                    pattern_kind: match &f.pattern {
                        PathFieldPattern::KeyValue { .. } => "key_value".to_string(),
                        PathFieldPattern::Regex { .. } => "regex".to_string(),
                        PathFieldPattern::SegmentPosition { .. } => "segment_position".to_string(),
                        PathFieldPattern::PartitionDir { .. } => "partition_dir".to_string(),
                    },
                    coverage_pct: f.coverage.matched_files as f64 / total_files as f64 * 100.0,
                    confidence: f.confidence.clone(),
                    examples: f.examples.clone(),
                })
                .collect(),
            collisions: PathFieldCollisionsSummary {
                same_name_count: collisions.same_name_different_values.len(),
                segment_overlap_count: collisions.segment_overlap.len(),
                parsed_column_collision_count: collisions.with_parsed_columns.len(),
            },
            required_human_questions: questions,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Path Fields Apply Tool
// ============================================================================

/// Tool: casp.path_fields.apply
pub struct PathFieldsApplyTool;

#[derive(Debug, Deserialize)]
struct PathFieldsApplyArgs {
    /// Session ID
    session_id: SessionId,
    /// Proposal ID
    proposal_id: ProposalId,
    /// Field names to include (if empty, include all)
    #[serde(default)]
    included_fields: Vec<String>,
    /// Collision resolutions
    #[serde(default)]
    collision_resolutions: HashMap<String, String>,
    /// Approval token
    approval_token_hash: String,
}

#[derive(Debug, Serialize)]
struct PathFieldsApplyResponse {
    applied: bool,
    field_count: usize,
    new_state: String,
}

impl McpTool for PathFieldsApplyTool {
    fn name(&self) -> &'static str {
        "casp_path_fields_apply"
    }

    fn description(&self) -> &'static str {
        "Apply approved path-derived field configuration. Requires approval token (Gate G3)."
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
                "included_fields": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Field names to include (if empty, include all)"
                },
                "collision_resolutions": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Map of collision field names to resolution (namespace|rename|drop)"
                },
                "approval_token_hash": {
                    "type": "string",
                    "description": "Approval token hash for verification"
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
        let args: PathFieldsApplyArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Read the proposal
        let proposal: PathFieldProposal = bundle.read_proposal("path_fields", args.proposal_id)?;

        // Verify approval token
        if args.approval_token_hash != proposal.proposal_hash {
            anyhow::bail!("Invalid approval token");
        }

        // Filter fields if specified
        let fields: Vec<&PathField> = if args.included_fields.is_empty() {
            proposal.fields.iter().collect()
        } else {
            proposal
                .fields
                .iter()
                .filter(|f| args.included_fields.contains(&f.field_name))
                .collect()
        };

        // Record decision
        let decision = crate::intent::types::DecisionRecord {
            timestamp: chrono::Utc::now(),
            actor: "agent".to_string(),
            decision: crate::intent::types::Decision::Approve,
            target: crate::intent::types::DecisionTarget {
                proposal_id: args.proposal_id,
                approval_target_hash: args.approval_token_hash,
            },
            choice_payload: serde_json::json!({
                "included_fields": args.included_fields,
                "collision_resolutions": args.collision_resolutions
            }),
            notes: Some("Path fields approved via MCP".to_string()),
        };
        bundle.append_decision(&decision)?;

        // Update state
        bundle.update_state(IntentState::AwaitingPathFieldsApproval)?;

        let response = PathFieldsApplyResponse {
            applied: true,
            field_count: fields.len(),
            new_state: IntentState::InferSchemaIntent.as_str().to_string(),
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn extract_path_fields(
    entries: &[crate::intent::session::FileSetEntry],
    parsed_columns: &[String],
    namespacing: Option<&PathFieldNamespacing>,
    total_files: u64,
) -> anyhow::Result<(Vec<PathField>, PathFieldCollisions, Vec<HumanQuestion>)> {
    let prefix = namespacing
        .map(|n| n.default_prefix.as_str())
        .unwrap_or("_cf_path_");

    // Patterns for key=value extraction
    let key_value_re = Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)=([^/\\]+)")?;
    let partition_re = Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)=([^/\\]+)/")?;
    let date_re = Regex::new(r"(\d{4}[-/]\d{2}[-/]\d{2})")?;

    let mut field_values: HashMap<String, Vec<String>> = HashMap::new();
    let mut field_sources: HashMap<String, PathFieldSource> = HashMap::new();
    let mut field_patterns: HashMap<String, PathFieldPattern> = HashMap::new();
    let mut field_matches: HashMap<String, u64> = HashMap::new();

    for entry in entries {
        let path = &entry.path;

        // Extract key=value patterns
        for cap in key_value_re.captures_iter(path) {
            let key = cap[1].to_string();
            let value = cap[2].to_string();

            field_values
                .entry(key.clone())
                .or_default()
                .push(value.clone());
            *field_matches.entry(key.clone()).or_default() += 1;

            if !field_patterns.contains_key(&key) {
                field_patterns.insert(
                    key.clone(),
                    PathFieldPattern::KeyValue {
                        value: format!("{}=", key),
                    },
                );
                field_sources.insert(
                    key,
                    PathFieldSource {
                        segment_index: None,
                        filename_group: None,
                    },
                );
            }
        }

        // Extract partition directories
        for cap in partition_re.captures_iter(path) {
            let key = cap[1].to_string();
            if !field_patterns.contains_key(&key) {
                field_patterns.insert(
                    key.clone(),
                    PathFieldPattern::PartitionDir {
                        value: format!("{}=", key),
                    },
                );
            }
        }

        // Look for date patterns in path
        for cap in date_re.captures_iter(path) {
            let date_str = cap[1].to_string();
            let key = "date".to_string();
            field_values.entry(key.clone()).or_default().push(date_str);
            *field_matches.entry(key.clone()).or_default() += 1;

            if !field_patterns.contains_key(&key) {
                field_patterns.insert(
                    key.clone(),
                    PathFieldPattern::Regex {
                        value: r"\d{4}[-/]\d{2}[-/]\d{2}".to_string(),
                    },
                );
                field_sources.insert(
                    key,
                    PathFieldSource {
                        segment_index: None,
                        filename_group: None,
                    },
                );
            }
        }
    }

    // Build fields
    let mut fields = Vec::new();
    for (name, values) in &field_values {
        let pattern = field_patterns
            .get(name)
            .cloned()
            .unwrap_or(PathFieldPattern::KeyValue {
                value: format!("{}=", name),
            });

        let source = field_sources.get(name).cloned().unwrap_or(PathFieldSource {
            segment_index: None,
            filename_group: None,
        });

        let matched_files = *field_matches.get(name).unwrap_or(&0);
        let coverage = PathFieldCoverage {
            matched_files,
            total_files,
        };

        // Infer dtype
        let dtype = infer_dtype(values);

        // Get unique examples
        let unique_values: HashSet<_> = values.iter().collect();
        let examples: Vec<String> = unique_values.into_iter().take(5).cloned().collect();

        let field = PathField {
            field_name: format!("{}{}", prefix, name),
            dtype: dtype.clone(),
            pattern,
            source,
            coverage: coverage.clone(),
            examples: examples.clone(),
            confidence: Confidence::medium(vec![format!(
                "Matched {}/{} files",
                matched_files, total_files
            )]),
        };

        // Compute confidence
        let confidence_score = compute_path_field_confidence(&field, total_files);
        let mut field = field;
        field.confidence = confidence_score.to_confidence();

        fields.push(field);
    }

    // Detect collisions
    let mut collisions = PathFieldCollisions {
        same_name_different_values: vec![],
        segment_overlap: vec![],
        with_parsed_columns: vec![],
    };

    // Check for collisions with parsed columns
    let parsed_columns_set: HashSet<_> = parsed_columns.iter().collect();
    for field in &fields {
        let base_name = field.field_name.trim_start_matches(prefix);
        if parsed_columns_set.contains(&base_name.to_string()) {
            collisions.with_parsed_columns.push(ParsedColumnCollision {
                derived_field: field.field_name.clone(),
                parsed_column: base_name.to_string(),
            });
        }
    }

    // Generate questions for collisions
    let mut questions = Vec::new();
    if !collisions.with_parsed_columns.is_empty() {
        questions.push(HumanQuestion {
            question_id: QuestionId::new(),
            kind: QuestionKind::ResolveCollision,
            prompt: format!(
                "{} derived field(s) collide with parsed columns. How should they be resolved?",
                collisions.with_parsed_columns.len()
            ),
            options: vec![
                QuestionOption {
                    option_id: "namespace".to_string(),
                    label: "Keep prefix".to_string(),
                    consequence: "Derived fields will have _cf_path_ prefix".to_string(),
                    default: true,
                },
                QuestionOption {
                    option_id: "drop".to_string(),
                    label: "Drop derived".to_string(),
                    consequence: "Only parsed columns will be kept".to_string(),
                    default: false,
                },
                QuestionOption {
                    option_id: "rename".to_string(),
                    label: "Rename derived".to_string(),
                    consequence: "Derived fields will be renamed with _derived suffix".to_string(),
                    default: false,
                },
            ],
            evidence_refs: vec![],
            deadline: None,
        });
    }

    Ok((fields, collisions, questions))
}

fn infer_dtype(values: &[String]) -> PathFieldDtype {
    if values.is_empty() {
        return PathFieldDtype::String;
    }

    // Check if all values are integers
    let all_int = values.iter().all(|v| v.parse::<i64>().is_ok());
    if all_int {
        return PathFieldDtype::Int;
    }

    // Check if all values look like dates
    let date_re = Regex::new(r"^\d{4}[-/]\d{2}[-/]\d{2}$").unwrap();
    let all_date = values.iter().all(|v| date_re.is_match(v));
    if all_date {
        return PathFieldDtype::Date;
    }

    // Check if all values look like timestamps
    let ts_re = Regex::new(r"^\d{4}[-/]\d{2}[-/]\d{2}[T ]\d{2}:\d{2}").unwrap();
    let all_ts = values.iter().all(|v| ts_re.is_match(v));
    if all_ts {
        return PathFieldDtype::Timestamp;
    }

    PathFieldDtype::String
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_dtype() {
        assert_eq!(
            infer_dtype(&["123".to_string(), "456".to_string()]),
            PathFieldDtype::Int
        );
        assert_eq!(
            infer_dtype(&["2024-01-15".to_string(), "2024-02-20".to_string()]),
            PathFieldDtype::Date
        );
        assert_eq!(
            infer_dtype(&["hello".to_string(), "world".to_string()]),
            PathFieldDtype::String
        );
    }

    #[test]
    fn test_path_fields_args_deserialize() {
        let session_id = SessionId::new();
        let file_set_id = FileSetId::new();

        let json = json!({
            "session_id": session_id.to_string(),
            "file_set_id": file_set_id.to_string(),
            "parsed_columns": ["id", "name", "region"]
        });

        let args: PathFieldsProposeArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.session_id, session_id);
        assert_eq!(args.parsed_columns, vec!["id", "name", "region"]);
    }
}
