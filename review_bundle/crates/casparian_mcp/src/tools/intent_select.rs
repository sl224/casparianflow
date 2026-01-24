//! MCP tools for file selection (§7.3)
//!
//! - `casp.select.propose` → Propose file selection based on intent

use anyhow::Context;
// Sync tool implementations (no async)
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::core::CoreHandle;
use crate::intent::confidence::compute_selection_confidence;
use crate::intent::fileset::FileSetStore;
use crate::intent::session::SessionStore;
use crate::intent::state::IntentState;
use crate::intent::types::{
    Confidence, DirPrefixEvidence, ExtensionEvidence, FileSetId, NextAction, ProposalId,
    SamplingMethod, SelectionEvidence, SelectionPreview, SelectionProposal, SemanticTokenEvidence,
    SessionId, TagCollisionEvidence,
};
use crate::jobs::JobExecutorHandle;
use crate::security::SecurityConfig;
use crate::server::McpServerConfig;
use crate::tools::McpTool;

// ============================================================================
// Select Propose Tool
// ============================================================================

/// Tool: casp.select.propose
pub struct SelectProposeTool;

#[derive(Debug, Deserialize)]
struct SelectProposeArgs {
    /// Session ID
    session_id: SessionId,
    /// Base directory to search in
    base_dir: String,
    /// Intent-derived filter patterns (globs)
    #[serde(default)]
    patterns: Vec<String>,
    /// Intent-derived semantic tokens to match in paths
    #[serde(default)]
    semantic_tokens: Vec<String>,
    /// File extensions to include
    #[serde(default)]
    extensions: Vec<String>,
    /// Maximum files to include in selection
    #[serde(default = "default_max_files")]
    max_files: usize,
}

fn default_max_files() -> usize {
    10000
}

#[derive(Debug, Serialize)]
struct SelectProposeResponse {
    proposal_id: ProposalId,
    proposal_hash: String,
    selected_file_set_id: FileSetId,
    selected_count: u64,
    near_miss_file_set_id: FileSetId,
    near_miss_count: u64,
    evidence: SelectionEvidence,
    confidence: Confidence,
    preview: SelectionPreview,
    next_actions: Vec<NextAction>,
}

impl McpTool for SelectProposeTool {
    fn name(&self) -> &'static str {
        "casp_select_propose"
    }

    fn description(&self) -> &'static str {
        "Propose a file selection based on intent-derived criteria. Returns file set IDs (never inline file lists) with evidence and confidence scores."
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
                "base_dir": {
                    "type": "string",
                    "description": "Base directory to search in"
                },
                "patterns": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Glob patterns to match files"
                },
                "semantic_tokens": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Semantic tokens to look for in paths (e.g., 'sales', 'orders')"
                },
                "extensions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File extensions to include (e.g., '.csv', '.parquet')"
                },
                "max_files": {
                    "type": "integer",
                    "description": "Maximum files to include in selection (default: 10000)"
                }
            },
            "required": ["session_id", "base_dir"]
        })
    }

    fn execute(
        &self,
        args: Value,
        security: &SecurityConfig,
        _core: &CoreHandle,
        _config: &McpServerConfig,
        _executor: &JobExecutorHandle,
    ) -> anyhow::Result<Value> {
        let args: SelectProposeArgs = serde_json::from_value(args)?;

        // Validate path is allowed
        let base_path = security
            .path_allowlist
            .validate(Path::new(&args.base_dir))?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Scan the directory for files
        let (selected_paths, near_miss_paths, evidence) = scan_and_classify(
            &base_path,
            &args.patterns,
            &args.semantic_tokens,
            &args.extensions,
            args.max_files,
        )?;

        // Create file sets
        let mut fs_store = FileSetStore::new();

        let selected_meta = fs_store.create_from_paths(
            &bundle,
            selected_paths.clone(),
            SamplingMethod::All,
            None,
        )?;

        let near_miss_meta = fs_store.create_from_paths(
            &bundle,
            near_miss_paths.clone(),
            SamplingMethod::All,
            None,
        )?;

        // Compute confidence
        let confidence_score = compute_selection_confidence(
            &evidence.top_dir_prefixes,
            &evidence.extensions,
            &evidence.semantic_tokens,
            &evidence.collision_with_existing_tags,
            selected_meta.count,
        );

        // Create preview (bounded)
        let preview = SelectionPreview {
            selected_examples: selected_paths.iter().take(5).cloned().collect(),
            near_miss_examples: near_miss_paths.iter().take(5).cloned().collect(),
        };

        // Determine next actions
        let next_actions = if confidence_score.label == crate::intent::types::ConfidenceLabel::High
        {
            vec![
                NextAction::AskHumanConfirmSelection,
                NextAction::ProposeTagRules,
            ]
        } else {
            vec![NextAction::AskHumanConfirmSelection]
        };

        // Create proposal
        let proposal = SelectionProposal {
            proposal_id: ProposalId::new(),
            proposal_hash: String::new(),
            selected_file_set_id: selected_meta.file_set_id,
            near_miss_file_set_id: near_miss_meta.file_set_id,
            evidence: evidence.clone(),
            confidence: confidence_score.to_confidence(),
            preview: preview.clone(),
            next_actions: next_actions.clone(),
        };

        // Compute hash and save
        let proposal_hash = proposal.compute_hash();
        let mut proposal = proposal;
        proposal.proposal_hash = proposal_hash.clone();

        bundle.write_proposal("selection", proposal.proposal_id, &proposal)?;

        // Update session state
        bundle.update_state(IntentState::ProposeSelection)?;

        let response = SelectProposeResponse {
            proposal_id: proposal.proposal_id,
            proposal_hash,
            selected_file_set_id: selected_meta.file_set_id,
            selected_count: selected_meta.count,
            near_miss_file_set_id: near_miss_meta.file_set_id,
            near_miss_count: near_miss_meta.count,
            evidence,
            confidence: proposal.confidence,
            preview,
            next_actions,
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Selection Approval Tool
// ============================================================================

/// Tool: casp.select.approve
pub struct SelectApproveTool;

#[derive(Debug, Deserialize)]
struct SelectApproveArgs {
    /// Session ID
    session_id: SessionId,
    /// Proposal ID to approve
    proposal_id: ProposalId,
    /// Approval token hash
    approval_token_hash: String,
}

#[derive(Debug, Serialize)]
struct SelectApproveResponse {
    approved: bool,
    new_state: String,
    corpus_snapshot_ref: String,
}

impl McpTool for SelectApproveTool {
    fn name(&self) -> &'static str {
        "casp_select_approve"
    }

    fn description(&self) -> &'static str {
        "Approve a file selection proposal. This triggers corpus snapshotting (Gate G1)."
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
                    "description": "Proposal ID to approve"
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
        let args: SelectApproveArgs = serde_json::from_value(args)?;

        let session_store = SessionStore::new();
        let bundle = session_store.get_session(args.session_id)?;

        // Read the proposal
        let proposal: SelectionProposal = bundle.read_proposal("selection", args.proposal_id)?;

        // Verify approval token (simplified - in production would verify signature)
        if args.approval_token_hash != proposal.proposal_hash {
            anyhow::bail!("Invalid approval token");
        }

        // Snapshot corpus - read all files in the selected file set
        let entries = bundle.read_fileset(proposal.selected_file_set_id)?;

        let corpus_entries: Vec<crate::intent::session::CorpusEntry> = entries
            .iter()
            .map(|e| {
                let metadata = std::fs::metadata(&e.path)
                    .with_context(|| format!("Failed to stat corpus file: {}", e.path))?;
                let modified = metadata
                    .modified()
                    .with_context(|| format!("Failed to read mtime for: {}", e.path))?;
                let mtime = modified
                    .duration_since(UNIX_EPOCH)
                    .with_context(|| format!("mtime before UNIX_EPOCH for: {}", e.path))?
                    .as_secs();
                let mtime = i64::try_from(mtime)
                    .with_context(|| format!("mtime exceeds i64::MAX for: {}", e.path))?;

                Ok(crate::intent::session::CorpusEntry {
                    path: e.path.clone(),
                    size: metadata.len(),
                    mtime,
                    content_hash: e.content_hash.clone(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        bundle.append_corpus_entries(&corpus_entries)?;

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
            notes: Some("Selection approved via MCP".to_string()),
        };
        bundle.append_decision(&decision)?;

        // Update state to next stage
        bundle.update_state(IntentState::ProposeTagRules)?;

        // Update manifest with corpus ref
        let mut manifest = bundle.read_manifest()?;
        manifest.corpus_manifest_ref = Some("corpora/corpus_manifest.jsonl".to_string());
        bundle.write_manifest(&manifest)?;

        let response = SelectApproveResponse {
            approved: true,
            new_state: IntentState::ProposeTagRules.as_str().to_string(),
            corpus_snapshot_ref: "corpora/corpus_manifest.jsonl".to_string(),
        };

        Ok(serde_json::to_value(response)?)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Scan directory and classify files into selected and near-miss
fn scan_and_classify(
    base_path: &Path,
    patterns: &[String],
    semantic_tokens: &[String],
    extensions: &[String],
    max_files: usize,
) -> anyhow::Result<(Vec<String>, Vec<String>, SelectionEvidence)> {
    use walkdir::WalkDir;

    let mut selected = Vec::new();
    let mut near_miss = Vec::new();

    let mut dir_counts: HashMap<String, u64> = HashMap::new();
    let mut ext_counts: HashMap<String, u64> = HashMap::new();
    let mut token_counts: HashMap<String, u64> = HashMap::new();

    let patterns: Vec<String> = patterns
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    let compiled_patterns: Vec<glob::Pattern> = patterns
        .iter()
        .map(|p| glob::Pattern::new(p).with_context(|| format!("Invalid glob pattern: {}", p)))
        .collect::<anyhow::Result<_>>()?;

    let semantic_tokens_lower: Vec<String> =
        semantic_tokens.iter().map(|t| t.to_lowercase()).collect();

    let extensions_lower: Vec<String> = extensions
        .iter()
        .map(|e| {
            let e = e.to_lowercase();
            if e.starts_with('.') {
                e
            } else {
                format!(".{}", e)
            }
        })
        .collect();

    for entry in WalkDir::new(base_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_symlink() {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();
        let path_lower = path_str.to_lowercase();

        // Check extension
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .unwrap_or_default();

        let ext_match = extensions_lower.is_empty() || extensions_lower.contains(&ext);

        // Check semantic tokens
        let mut token_match = semantic_tokens_lower.is_empty();
        for token in &semantic_tokens_lower {
            if path_lower.contains(token) {
                token_match = true;
                *token_counts.entry(token.clone()).or_default() += 1;
            }
        }

        // Check glob patterns
        let pattern_match =
            compiled_patterns.is_empty() || compiled_patterns.iter().any(|p| p.matches(&path_str));

        // Classify
        if ext_match && token_match && pattern_match {
            if selected.len() < max_files {
                selected.push(path_str.clone());

                // Track evidence
                if let Some(parent) = path.parent() {
                    let prefix = parent
                        .components()
                        .take(3)
                        .collect::<std::path::PathBuf>()
                        .to_string_lossy()
                        .to_string();
                    *dir_counts.entry(prefix).or_default() += 1;
                }

                if !ext.is_empty() {
                    *ext_counts.entry(ext).or_default() += 1;
                }
            }
        } else if ext_match || token_match {
            // Near miss - matched some criteria but not all
            if near_miss.len() < 1000 {
                near_miss.push(path_str);
            }
        }
    }

    // Build evidence
    let mut top_dir_prefixes: Vec<DirPrefixEvidence> = dir_counts
        .into_iter()
        .map(|(prefix, count)| DirPrefixEvidence { prefix, count })
        .collect();
    top_dir_prefixes.sort_by(|a, b| b.count.cmp(&a.count));
    top_dir_prefixes.truncate(10);

    let mut extensions_evidence: Vec<ExtensionEvidence> = ext_counts
        .into_iter()
        .map(|(ext, count)| ExtensionEvidence { ext, count })
        .collect();
    extensions_evidence.sort_by(|a, b| b.count.cmp(&a.count));

    let semantic_tokens_evidence: Vec<SemanticTokenEvidence> = token_counts
        .into_iter()
        .map(|(token, count)| SemanticTokenEvidence { token, count })
        .collect();

    let evidence = SelectionEvidence {
        top_dir_prefixes,
        extensions: extensions_evidence,
        semantic_tokens: semantic_tokens_evidence,
        collision_with_existing_tags: vec![], // Would check against existing tags
    };

    Ok((selected, near_miss, evidence))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_propose_args_deserialize() {
        let session_id = SessionId::new();

        let json = json!({
            "session_id": session_id.to_string(),
            "base_dir": "/data",
            "patterns": ["**/*.csv"],
            "semantic_tokens": ["sales", "orders"],
            "extensions": [".csv", ".parquet"]
        });

        let args: SelectProposeArgs = serde_json::from_value(json).unwrap();
        assert_eq!(args.session_id, session_id);
        assert_eq!(args.base_dir, "/data");
        assert_eq!(args.patterns, vec!["**/*.csv"]);
        assert_eq!(args.semantic_tokens, vec!["sales", "orders"]);
    }
}
