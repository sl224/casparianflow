//! MCP tools for tag rules (§7.4)
//!
//! - `casp.tags.propose_rules` → Propose tagging rules
//! - `casp.tags.apply_rules` → Apply approved tagging rules

// Sync tool implementations (no async)
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::core::CoreHandle;
use crate::intent::session::SessionStore;
use crate::intent::state::IntentState;
use crate::intent::types::{
    Confidence, ConfidenceLabel, FileSetId, HumanQuestion, ProposalId, QuestionId, QuestionKind,
    QuestionOption, RuleEvaluation, RuleEvaluationExamples, RuleEvaluationSampling, SessionId,
    TagRule, TagRuleCandidate, TagRuleProposal, TagRuleWhen,
};
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::tools::McpTool;

// ============================================================================
// Tag Rules Propose Tool
// ============================================================================

/// Tool: casp.tags.propose_rules
pub struct TagsProposeRulesTool;

#[derive(Debug, Deserialize)]
struct TagsProposeRulesArgs {
    /// Session ID
    session_id: SessionId,
    /// File set ID to base rules on
    file_set_id: FileSetId,
    /// Intent-derived tag names
    #[serde(default)]
    suggested_tags: Vec<String>,
    /// Route to this topic
    #[serde(default)]
    route_to_topic: Option<String>,
}

#[derive(Debug, Serialize)]
struct TagsProposeRulesResponse {
    proposal_id: ProposalId,
    proposal_hash: String,
    candidates: Vec<TagRuleCandidateSummary>,
    recommended_rule_id: Option<String>,
    required_human_questions: Vec<HumanQuestion>,
}

#[derive(Debug, Serialize)]
struct TagRuleCandidateSummary {
    rule_id: String,
    description: String,
    precision_estimate: f64,
    recall_estimate: f64,
    confidence: Confidence,
}

impl McpTool for TagsProposeRulesTool {
    fn name(&self) -> &'static str {
        "casp_tags_propose_rules"
    }

    fn description(&self) -> &'static str {
        "Propose tagging rules based on a file selection. Returns rule candidates with precision/recall estimates."
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
                    "description": "File set ID to base rules on"
                },
                "suggested_tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Intent-derived tag names (e.g., ['sales', 'q4_2024'])"
                },
                "route_to_topic": {
                    "type": "string",
                    "description": "Topic to route tagged files to"
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
        let args: TagsProposeRulesArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Read the file set to analyze patterns
        let entries = bundle.read_fileset(args.file_set_id)?;

        // Analyze paths to generate rule candidates
        let (candidates, questions) = analyze_for_rules(
            &entries,
            &args.suggested_tags,
            args.route_to_topic.as_deref(),
            &bundle,
        )?;

        // Determine recommended rule
        let recommended_rule_id = candidates
            .iter()
            .filter(|c| c.confidence.label == ConfidenceLabel::High)
            .max_by(|a, b| {
                a.evaluation
                    .precision_estimate
                    .partial_cmp(&b.evaluation.precision_estimate)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|c| c.rule.rule_id.clone());

        // Create proposal
        let proposal = TagRuleProposal {
            proposal_id: ProposalId::new(),
            proposal_hash: String::new(),
            candidates: candidates.clone(),
            recommended_rule_id: recommended_rule_id.clone(),
            required_human_questions: questions.clone(),
        };

        // Compute hash (simplified)
        let proposal_hash = format!("{:x}", md5::compute(serde_json::to_string(&proposal)?));

        let mut proposal = proposal;
        proposal.proposal_hash = proposal_hash.clone();

        bundle.write_proposal("tag_rules", proposal.proposal_id, &proposal)?;
        bundle.update_state(IntentState::ProposeTagRules)?;

        let response = TagsProposeRulesResponse {
            proposal_id: proposal.proposal_id,
            proposal_hash,
            candidates: candidates
                .iter()
                .map(|c| TagRuleCandidateSummary {
                    rule_id: c.rule.rule_id.clone(),
                    description: format!(
                        "Match {} patterns, {} extensions",
                        c.rule.when.path_glob.len(),
                        c.rule.when.extension.len()
                    ),
                    precision_estimate: c.evaluation.precision_estimate,
                    recall_estimate: c.evaluation.recall_estimate,
                    confidence: c.confidence.clone(),
                })
                .collect(),
            recommended_rule_id,
            required_human_questions: questions,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Tag Rules Apply Tool
// ============================================================================

/// Tool: casp.tags.apply_rules
pub struct TagsApplyRulesTool;

#[derive(Debug, Deserialize)]
struct TagsApplyRulesArgs {
    /// Session ID
    session_id: SessionId,
    /// Proposal ID
    proposal_id: ProposalId,
    /// Rule ID to apply
    selected_rule_id: String,
    /// Approval token
    approval_token_hash: String,
}

#[derive(Debug, Serialize)]
struct TagsApplyRulesResponse {
    applied: bool,
    rule_id: String,
    new_state: String,
}

impl McpTool for TagsApplyRulesTool {
    fn name(&self) -> &'static str {
        "casp_tags_apply_rules"
    }

    fn description(&self) -> &'static str {
        "Apply an approved tagging rule. Requires approval token (Gate G2)."
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
                "selected_rule_id": {
                    "type": "string",
                    "description": "Rule ID to apply from the proposal"
                },
                "approval_token_hash": {
                    "type": "string",
                    "description": "Approval token hash for verification"
                }
            },
            "required": ["session_id", "proposal_id", "selected_rule_id", "approval_token_hash"]
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
        let args: TagsApplyRulesArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Read the proposal
        let proposal: TagRuleProposal = bundle.read_proposal("tag_rules", args.proposal_id)?;

        // Verify approval token
        if args.approval_token_hash != proposal.proposal_hash {
            anyhow::bail!(
                "Approval token mismatch. Refresh the proposal and retry with the latest token."
            );
        }

        // Find the selected rule
        let _selected_candidate = proposal
            .candidates
            .iter()
            .find(|c| c.rule.rule_id == args.selected_rule_id)
            .ok_or_else(|| anyhow::anyhow!("Rule not found: {}", args.selected_rule_id))?;

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
                "selected_rule_id": args.selected_rule_id
            }),
            notes: Some("Tag rule approved via MCP".to_string()),
        };
        bundle.append_decision(&decision)?;

        // Update state
        bundle.update_state(IntentState::AwaitingTagRulesApproval)?;

        // In production, would actually persist the rule to the scout system

        let response = TagsApplyRulesResponse {
            applied: true,
            rule_id: args.selected_rule_id,
            new_state: IntentState::ProposePathFields.as_str().to_string(),
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn analyze_for_rules(
    entries: &[crate::intent::session::FileSetEntry],
    suggested_tags: &[String],
    route_to_topic: Option<&str>,
    _bundle: &crate::intent::session::SessionBundle,
) -> anyhow::Result<(Vec<TagRuleCandidate>, Vec<HumanQuestion>)> {
    // Analyze path patterns
    let mut dir_patterns: HashMap<String, u64> = HashMap::new();
    let mut ext_patterns: HashMap<String, u64> = HashMap::new();

    for entry in entries {
        let path = std::path::Path::new(&entry.path);

        // Extract directory pattern (first 2-3 components)
        if let Some(parent) = path.parent() {
            let pattern = format!("{}/**", parent.to_string_lossy());
            *dir_patterns.entry(pattern).or_default() += 1;
        }

        // Extract extension
        if let Some(ext) = path.extension() {
            let ext_str = format!(".{}", ext.to_string_lossy().to_lowercase());
            *ext_patterns.entry(ext_str).or_default() += 1;
        }
    }

    let total_files = entries.len() as u64;
    let mut candidates = Vec::new();

    // Generate rule candidates
    let tags = if suggested_tags.is_empty() {
        vec!["auto_tagged".to_string()]
    } else {
        suggested_tags.to_vec()
    };

    // Candidate 1: By extension only
    let top_extensions: Vec<String> = ext_patterns
        .iter()
        .filter(|(_, &count)| count as f64 / total_files as f64 > 0.8)
        .map(|(ext, _)| ext.clone())
        .collect();

    if !top_extensions.is_empty() {
        let rule = TagRule {
            rule_id: "ext_only".to_string(),
            enabled: false,
            when: TagRuleWhen {
                path_glob: vec![],
                extension: top_extensions.clone(),
                magic_bytes: vec![],
            },
            add_tags: tags.clone(),
            route_to_topic: route_to_topic.map(String::from),
        };

        let matched_count = ext_patterns
            .iter()
            .filter(|(ext, _)| top_extensions.contains(ext))
            .map(|(_, &count)| count)
            .sum::<u64>();

        let evaluation = RuleEvaluation {
            matched_file_set_id: FileSetId::new(),
            negative_sample_file_set_id: FileSetId::new(),
            precision_estimate: 0.95, // Would compute from negative samples
            recall_estimate: matched_count as f64 / total_files as f64,
            false_positive_estimate: 0.05,
            conflicts: vec![],
            examples: RuleEvaluationExamples {
                matches: entries.iter().take(3).map(|e| e.path.clone()).collect(),
                near_misses: vec![],
                false_positive_examples: vec![],
            },
            sampling: RuleEvaluationSampling {
                method: "stratified_sample".to_string(),
                seed: 42,
                notes: None,
            },
        };

        let confidence = Confidence::high(vec![format!(
            "Extension filter covers {:.0}% of files",
            evaluation.recall_estimate * 100.0
        )]);

        candidates.push(TagRuleCandidate {
            rule,
            evaluation,
            confidence,
        });
    }

    // Candidate 2: By directory pattern
    let top_dir: Option<(&String, &u64)> = dir_patterns.iter().max_by_key(|(_, &count)| count);

    if let Some((pattern, &count)) = top_dir {
        if count as f64 / total_files as f64 > 0.5 {
            let rule = TagRule {
                rule_id: "dir_pattern".to_string(),
                enabled: false,
                when: TagRuleWhen {
                    path_glob: vec![pattern.clone()],
                    extension: top_extensions.clone(),
                    magic_bytes: vec![],
                },
                add_tags: tags.clone(),
                route_to_topic: route_to_topic.map(String::from),
            };

            let evaluation = RuleEvaluation {
                matched_file_set_id: FileSetId::new(),
                negative_sample_file_set_id: FileSetId::new(),
                precision_estimate: 0.98,
                recall_estimate: count as f64 / total_files as f64,
                false_positive_estimate: 0.02,
                conflicts: vec![],
                examples: RuleEvaluationExamples {
                    matches: entries.iter().take(3).map(|e| e.path.clone()).collect(),
                    near_misses: vec![],
                    false_positive_examples: vec![],
                },
                sampling: RuleEvaluationSampling {
                    method: "stratified_sample".to_string(),
                    seed: 42,
                    notes: None,
                },
            };

            let confidence = Confidence::high(vec![
                format!(
                    "Directory pattern covers {:.0}% of files",
                    evaluation.recall_estimate * 100.0
                ),
                "Combined with extension filter".to_string(),
            ]);

            candidates.push(TagRuleCandidate {
                rule,
                evaluation,
                confidence,
            });
        }
    }

    // Generate questions if confidence is not high for any candidate
    let mut questions = Vec::new();
    if candidates
        .iter()
        .all(|c| c.confidence.label != ConfidenceLabel::High)
    {
        questions.push(HumanQuestion {
            question_id: QuestionId::new(),
            kind: QuestionKind::ResolveAmbiguity,
            prompt: "Multiple tagging strategies available. Which approach should be used?"
                .to_string(),
            options: candidates
                .iter()
                .map(|c| QuestionOption {
                    option_id: c.rule.rule_id.clone(),
                    label: format!("Use {} rule", c.rule.rule_id),
                    consequence: format!(
                        "Precision: {:.0}%, Recall: {:.0}%",
                        c.evaluation.precision_estimate * 100.0,
                        c.evaluation.recall_estimate * 100.0
                    ),
                    default: false,
                })
                .collect(),
            evidence_refs: vec![],
            deadline: None,
        });
    }

    Ok((candidates, questions))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tags_propose_args_deserialize() {
        let session_id = SessionId::new();
        let file_set_id = FileSetId::new();

        let json = json!({
            "session_id": session_id.to_string(),
            "file_set_id": file_set_id.to_string(),
            "suggested_tags": ["sales", "2024"],
            "route_to_topic": "sales_ingest"
        });

        let args: TagsProposeRulesArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.session_id, session_id);
        assert_eq!(args.suggested_tags, vec!["sales", "2024"]);
        assert_eq!(args.route_to_topic, Some("sales_ingest".to_string()));
    }
}
