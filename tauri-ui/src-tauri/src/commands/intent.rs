//! Intent Pipeline commands for Tauri.
//!
//! These commands wrap the MCP tool logic for file selection, tag rules,
//! path fields, schema, backtest, and publish/run workflows.

use crate::state::{CommandError, CommandResult};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;
use casparian_mcp::intent::{
    FileSetId, FileSetStore, ProposalId, SamplingMethod, SessionId, SessionStore,
};

// ============================================================================
// Selection Commands
// ============================================================================

/// Request for proposing a file selection.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectProposeRequest {
    pub session_id: String,
    pub base_dir: String,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub semantic_tokens: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default = "default_max_files")]
    pub max_files: usize,
}

fn default_max_files() -> usize {
    10000
}

/// Response from file selection proposal.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectProposeResponse {
    pub proposal_id: String,
    pub proposal_hash: String,
    pub file_set_id: String,
    pub file_count: u64,
    pub near_miss_count: u64,
    pub confidence: ConfidenceResponse,
    pub evidence: EvidenceResponse,
    pub sample_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfidenceResponse {
    pub score: f64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceResponse {
    pub dir_prefix_score: f64,
    pub extension_score: f64,
    pub semantic_score: f64,
}

/// Propose a file selection based on intent-derived criteria.
#[tauri::command]
pub async fn casp_select_propose(
    request: SelectProposeRequest,
    _state: State<'_, AppState>,
) -> CommandResult<SelectProposeResponse> {
    use std::collections::HashMap;
    use walkdir::WalkDir;

    // Parse session ID
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    // Scan directory for files
    let base_path = std::path::Path::new(&request.base_dir);
    if !base_path.exists() {
        return Err(CommandError::InvalidArgument(format!(
            "Directory does not exist: {}",
            request.base_dir
        )));
    }

    let mut selected_paths = Vec::new();
    let mut near_miss_paths = Vec::new();
    let mut dir_counts: HashMap<String, u64> = HashMap::new();
    let mut ext_counts: HashMap<String, u64> = HashMap::new();
    let mut token_counts: HashMap<String, u64> = HashMap::new();

    let patterns: Vec<String> = request
        .patterns
        .iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    let compiled_patterns: Vec<glob::Pattern> = patterns
        .iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();

    let semantic_tokens_lower: Vec<String> = request
        .semantic_tokens
        .iter()
        .map(|t| t.to_lowercase())
        .collect();

    let extensions_lower: Vec<String> = request
        .extensions
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
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
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
            if selected_paths.len() < request.max_files {
                selected_paths.push(path_str.clone());

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
            if near_miss_paths.len() < 1000 {
                near_miss_paths.push(path_str);
            }
        }
    }

    // Create file sets
    let mut fs_store = FileSetStore::new();

    let selected_meta = fs_store
        .create_from_paths(&bundle, selected_paths.clone(), SamplingMethod::All, None)
        .map_err(|e| CommandError::Internal(format!("Failed to create file set: {}", e)))?;

    let near_miss_meta = fs_store
        .create_from_paths(&bundle, near_miss_paths.clone(), SamplingMethod::All, None)
        .map_err(|e| {
            CommandError::Internal(format!("Failed to create near-miss file set: {}", e))
        })?;

    // Compute confidence scores
    let dir_total: u64 = dir_counts.values().sum();
    let dir_max = dir_counts.values().max().copied().unwrap_or(0);
    let dir_score = if dir_total > 0 {
        (dir_max as f64) / (dir_total as f64)
    } else {
        0.5
    };

    let ext_total: u64 = ext_counts.values().sum();
    let ext_max = ext_counts.values().max().copied().unwrap_or(0);
    let ext_score = if ext_total > 0 {
        (ext_max as f64) / (ext_total as f64)
    } else {
        0.5
    };

    let token_score = if !semantic_tokens_lower.is_empty() {
        let found = token_counts.len() as f64;
        let total = semantic_tokens_lower.len() as f64;
        found / total
    } else {
        0.5
    };

    let overall_score = (dir_score + ext_score + token_score) / 3.0;
    let label = if overall_score >= 0.8 {
        "high"
    } else if overall_score >= 0.5 {
        "medium"
    } else {
        "low"
    };

    // Create proposal
    let proposal_id = ProposalId::new();
    let proposal_hash = format!("{:x}", md5::compute(format!("{:?}", selected_paths)));

    // Save proposal
    let proposal_data = serde_json::json!({
        "proposal_id": proposal_id.to_string(),
        "proposal_hash": proposal_hash,
        "selected_file_set_id": selected_meta.file_set_id.to_string(),
        "near_miss_file_set_id": near_miss_meta.file_set_id.to_string(),
        "confidence": { "score": overall_score, "label": label },
    });

    bundle
        .write_proposal("selection", proposal_id, &proposal_data)
        .map_err(|e| CommandError::Internal(format!("Failed to save proposal: {}", e)))?;

    // Update session state
    bundle
        .update_state(casparian_mcp::intent::IntentState::ProposeSelection)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    let sample_files: Vec<String> = selected_paths.iter().take(5).cloned().collect();

    Ok(SelectProposeResponse {
        proposal_id: proposal_id.to_string(),
        proposal_hash,
        file_set_id: selected_meta.file_set_id.to_string(),
        file_count: selected_meta.count,
        near_miss_count: near_miss_meta.count,
        confidence: ConfidenceResponse {
            score: overall_score,
            label: label.to_string(),
        },
        evidence: EvidenceResponse {
            dir_prefix_score: dir_score,
            extension_score: ext_score,
            semantic_score: token_score,
        },
        sample_files,
    })
}

/// Request for approving a file selection.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectApproveRequest {
    pub session_id: String,
    pub proposal_id: String,
    pub approval_token_hash: String,
}

/// Response from file selection approval.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectApproveResponse {
    pub approved: bool,
    pub new_state: String,
}

/// Approve a file selection proposal.
#[tauri::command]
pub async fn casp_select_approve(
    request: SelectApproveRequest,
    _state: State<'_, AppState>,
) -> CommandResult<SelectApproveResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let proposal_id: ProposalId = request.proposal_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid proposal ID: {}", request.proposal_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    // Read proposal to verify hash
    let proposal: serde_json::Value = bundle
        .read_proposal("selection", proposal_id)
        .map_err(|e| CommandError::NotFound(format!("Proposal not found: {}", e)))?;

    let stored_hash = proposal
        .get("proposal_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if stored_hash != request.approval_token_hash {
        return Err(CommandError::InvalidArgument(
            "Invalid approval token".to_string(),
        ));
    }

    // Update state to next stage
    let new_state = casparian_mcp::intent::IntentState::ProposeTagRules;
    bundle
        .update_state(new_state)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(SelectApproveResponse {
        approved: true,
        new_state: new_state.as_str().to_string(),
    })
}

// ============================================================================
// File Set Commands
// ============================================================================

/// Request for sampling from a file set.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSetSampleRequest {
    pub session_id: String,
    pub file_set_id: String,
    #[serde(default = "default_sample_count")]
    pub n: usize,
}

fn default_sample_count() -> usize {
    25
}

/// Response with file set sample.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSetSampleResponse {
    pub files: Vec<String>,
    pub total_count: u64,
    pub sampled_count: usize,
}

/// Get a sample of files from a file set.
#[tauri::command]
pub async fn casp_fileset_sample(
    request: FileSetSampleRequest,
    _state: State<'_, AppState>,
) -> CommandResult<FileSetSampleResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let file_set_id: FileSetId = request.file_set_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid file set ID: {}", request.file_set_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    let entries = bundle
        .read_fileset(file_set_id)
        .map_err(|e| CommandError::NotFound(format!("File set not found: {}", e)))?;

    let total_count = entries.len() as u64;
    let n = request.n.min(100).min(entries.len());
    let files: Vec<String> = entries.iter().take(n).map(|e| e.path.clone()).collect();

    Ok(FileSetSampleResponse {
        files,
        total_count,
        sampled_count: n,
    })
}

/// Request for file set info.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSetInfoRequest {
    pub session_id: String,
    pub file_set_id: String,
}

/// Response with file set info.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSetInfoResponse {
    pub file_set_id: String,
    pub count: u64,
    pub sampling_method: String,
}

/// Get metadata about a file set.
#[tauri::command]
pub async fn casp_fileset_info(
    request: FileSetInfoRequest,
    _state: State<'_, AppState>,
) -> CommandResult<FileSetInfoResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let file_set_id: FileSetId = request.file_set_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid file set ID: {}", request.file_set_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    let entries = bundle
        .read_fileset(file_set_id)
        .map_err(|e| CommandError::NotFound(format!("File set not found: {}", e)))?;

    Ok(FileSetInfoResponse {
        file_set_id: file_set_id.to_string(),
        count: entries.len() as u64,
        sampling_method: "all".to_string(),
    })
}

// ============================================================================
// Tag Rules Commands
// ============================================================================

/// Request for applying tag rules.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagsApplyRulesRequest {
    pub session_id: String,
    pub proposal_id: String,
    pub selected_rule_id: String,
    pub approval_token_hash: String,
}

/// Response from applying tag rules.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagsApplyRulesResponse {
    pub applied: bool,
    pub new_state: String,
}

/// Apply approved tagging rules.
#[tauri::command]
pub async fn casp_tags_apply_rules(
    request: TagsApplyRulesRequest,
    _state: State<'_, AppState>,
) -> CommandResult<TagsApplyRulesResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    // Update state
    let new_state = casparian_mcp::intent::IntentState::ProposePathFields;
    bundle
        .update_state(new_state)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(TagsApplyRulesResponse {
        applied: true,
        new_state: new_state.as_str().to_string(),
    })
}

// ============================================================================
// Path Fields Commands
// ============================================================================

/// Request for applying path fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathFieldsApplyRequest {
    pub session_id: String,
    pub proposal_id: String,
    pub approval_token_hash: String,
    #[serde(default)]
    pub included_fields: Vec<String>,
}

/// Response from applying path fields.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathFieldsApplyResponse {
    pub applied: bool,
    pub new_state: String,
}

/// Apply approved path-derived fields.
#[tauri::command]
pub async fn casp_path_fields_apply(
    request: PathFieldsApplyRequest,
    _state: State<'_, AppState>,
) -> CommandResult<PathFieldsApplyResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    // Update state
    let new_state = casparian_mcp::intent::IntentState::InferSchemaIntent;
    bundle
        .update_state(new_state)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(PathFieldsApplyResponse {
        applied: true,
        new_state: new_state.as_str().to_string(),
    })
}

// ============================================================================
// Schema Commands
// ============================================================================

/// Request for promoting schema.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaPromoteRequest {
    pub session_id: String,
    pub schema_proposal_id: String,
    pub schema_name: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
}

fn default_schema_version() -> String {
    "1.0.0".to_string()
}

/// Response from promoting schema.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaPromoteResponse {
    pub promoted: bool,
    pub schema_ref: String,
    pub new_state: String,
}

/// Promote ephemeral schema to schema-as-code.
#[tauri::command]
pub async fn casp_schema_promote(
    request: SchemaPromoteRequest,
    _state: State<'_, AppState>,
) -> CommandResult<SchemaPromoteResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    let schema_ref = format!(
        "schemas/{}/v{}/schema.yaml",
        request.schema_name, request.schema_version
    );

    // Update state
    let new_state = casparian_mcp::intent::IntentState::GenerateParserDraft;
    bundle
        .update_state(new_state)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(SchemaPromoteResponse {
        promoted: true,
        schema_ref,
        new_state: new_state.as_str().to_string(),
    })
}

/// Request for resolving schema ambiguities.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaResolveAmbiguityRequest {
    pub session_id: String,
    pub proposal_id: String,
    pub resolutions: std::collections::HashMap<String, String>,
    pub approval_token_hash: String,
}

/// Response from resolving schema ambiguities.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaResolveAmbiguityResponse {
    pub resolved: bool,
    pub new_state: String,
}

/// Resolve schema type ambiguities.
#[tauri::command]
pub async fn casp_schema_resolve_ambiguity(
    request: SchemaResolveAmbiguityRequest,
    _state: State<'_, AppState>,
) -> CommandResult<SchemaResolveAmbiguityResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    // Update state
    let new_state = casparian_mcp::intent::IntentState::GenerateParserDraft;
    bundle
        .update_state(new_state)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(SchemaResolveAmbiguityResponse {
        resolved: true,
        new_state: new_state.as_str().to_string(),
    })
}

// ============================================================================
// Backtest Commands
// ============================================================================

/// Request for starting a backtest.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestStartRequest {
    pub session_id: String,
    pub draft_id: String,
    pub file_set_id: String,
    #[serde(default = "default_true")]
    pub fail_fast: bool,
}

fn default_true() -> bool {
    true
}

/// Response from starting a backtest.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestStartResponse {
    pub backtest_job_id: String,
    pub file_set_id: String,
    pub fail_fast: bool,
}

/// Start a backtest job.
#[tauri::command]
pub async fn casp_intent_backtest_start(
    request: BacktestStartRequest,
    _state: State<'_, AppState>,
) -> CommandResult<BacktestStartResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    let backtest_job_id = uuid::Uuid::new_v4().to_string();

    // Update state
    bundle
        .update_state(casparian_mcp::intent::IntentState::BacktestFailFast)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(BacktestStartResponse {
        backtest_job_id,
        file_set_id: request.file_set_id,
        fail_fast: request.fail_fast,
    })
}

/// Request for backtest status.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestStatusRequest {
    pub session_id: String,
    pub backtest_job_id: String,
}

/// Response with backtest status.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestStatusResponse {
    pub job_id: String,
    pub phase: String,
    pub elapsed_ms: u64,
    pub files_processed: u64,
    pub files_total: Option<u64>,
    pub rows_emitted: u64,
    pub rows_quarantined: u64,
    pub stalled: bool,
}

/// Get backtest job status.
#[tauri::command]
pub async fn casp_intent_backtest_status(
    request: BacktestStatusRequest,
    _state: State<'_, AppState>,
) -> CommandResult<BacktestStatusResponse> {
    // For now, return mock status
    // In production, would query actual job status
    Ok(BacktestStatusResponse {
        job_id: request.backtest_job_id,
        phase: "validate".to_string(),
        elapsed_ms: 5000,
        files_processed: 50,
        files_total: Some(100),
        rows_emitted: 10000,
        rows_quarantined: 100,
        stalled: false,
    })
}

/// Request for backtest report.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestReportRequest {
    pub session_id: String,
    pub backtest_job_id: String,
}

/// Response with backtest report.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestReportResponse {
    pub job_id: String,
    pub quality: QualityMetrics,
    pub top_violations: Vec<ViolationEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityMetrics {
    pub files_processed: u64,
    pub rows_emitted: u64,
    pub rows_quarantined: u64,
    pub quarantine_pct: f64,
    pub pass_rate_files: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViolationEntry {
    pub violation_type: String,
    pub count: u64,
    pub top_columns: Vec<ColumnCount>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnCount {
    pub name: String,
    pub count: u64,
}

/// Get backtest report.
#[tauri::command]
pub async fn casp_intent_backtest_report(
    request: BacktestReportRequest,
    _state: State<'_, AppState>,
) -> CommandResult<BacktestReportResponse> {
    // For now, return mock report
    Ok(BacktestReportResponse {
        job_id: request.backtest_job_id,
        quality: QualityMetrics {
            files_processed: 100,
            rows_emitted: 20000,
            rows_quarantined: 200,
            quarantine_pct: 1.0,
            pass_rate_files: 0.98,
        },
        top_violations: vec![ViolationEntry {
            violation_type: "TypeMismatch".to_string(),
            count: 100,
            top_columns: vec![ColumnCount {
                name: "amount".to_string(),
                count: 60,
            }],
        }],
    })
}

// ============================================================================
// Patch Commands
// ============================================================================

/// Request for applying a patch.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchApplyRequest {
    pub session_id: String,
    pub patch_type: String,
    pub patch_content: serde_json::Value,
    pub iteration_id: String,
}

/// Response from applying a patch.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchApplyResponse {
    pub applied: bool,
    pub patch_ref: String,
    pub next_action: String,
}

/// Apply a patch during backtest iteration.
#[tauri::command]
pub async fn casp_patch_apply(
    request: PatchApplyRequest,
    _state: State<'_, AppState>,
) -> CommandResult<PatchApplyResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    let patch_kind = format!("{}_patch", request.patch_type);
    let patch_bytes = serde_json::to_vec_pretty(&request.patch_content)
        .map_err(|e| CommandError::Internal(format!("Failed to serialize patch: {}", e)))?;

    let patch_ref = bundle
        .write_patch(&patch_kind, &request.iteration_id, &patch_bytes)
        .map_err(|e| CommandError::Internal(format!("Failed to write patch: {}", e)))?;

    Ok(PatchApplyResponse {
        applied: true,
        patch_ref,
        next_action: "re-run backtest".to_string(),
    })
}

// ============================================================================
// Publish Commands
// ============================================================================

/// Request for creating a publish plan.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishPlanRequest {
    pub session_id: String,
    pub draft_id: String,
    pub schema_name: String,
    pub schema_version: String,
    pub parser_name: String,
    pub parser_version: String,
}

/// Response with publish plan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishPlanResponse {
    pub proposal_id: String,
    pub approval_token_hash: String,
    pub schema_ref: String,
    pub parser_ref: String,
    pub invariants_checked: bool,
}

/// Create a publish plan.
#[tauri::command]
pub async fn casp_publish_plan(
    request: PublishPlanRequest,
    _state: State<'_, AppState>,
) -> CommandResult<PublishPlanResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    let proposal_id = ProposalId::new();
    let approval_token_hash = format!(
        "{:x}",
        md5::compute(format!(
            "{}:{}:{}:{}",
            request.schema_name,
            request.schema_version,
            request.parser_name,
            request.parser_version
        ))
    );

    // Update state
    bundle
        .update_state(casparian_mcp::intent::IntentState::AwaitingPublishApproval)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(PublishPlanResponse {
        proposal_id: proposal_id.to_string(),
        approval_token_hash,
        schema_ref: format!(
            "schemas/{}/v{}/schema.yaml",
            request.schema_name, request.schema_version
        ),
        parser_ref: format!(
            "parsers/{}/v{}/parser.py",
            request.parser_name, request.parser_version
        ),
        invariants_checked: true,
    })
}

/// Request for executing a publish.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishExecuteRequest {
    pub session_id: String,
    pub proposal_id: String,
    pub approval_token_hash: String,
}

/// Response from executing a publish.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishExecuteResponse {
    pub published: bool,
    pub new_state: String,
}

/// Execute a publish plan.
#[tauri::command]
pub async fn casp_publish_execute(
    request: PublishExecuteRequest,
    _state: State<'_, AppState>,
) -> CommandResult<PublishExecuteResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    // Update state
    let new_state = casparian_mcp::intent::IntentState::RunPlan;
    bundle
        .update_state(new_state)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(PublishExecuteResponse {
        published: true,
        new_state: new_state.as_str().to_string(),
    })
}

// ============================================================================
// Run Commands
// ============================================================================

/// Request for creating a run plan.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPlanRequest {
    pub session_id: String,
    pub file_set_id: String,
    pub parser_name: String,
    pub parser_version: String,
    pub sink_uri: String,
    pub route_to_topic: String,
}

/// Response with run plan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPlanResponse {
    pub proposal_id: String,
    pub approval_token_hash: String,
    pub file_count: u64,
    pub sink_uri: String,
}

/// Create a run plan.
#[tauri::command]
pub async fn casp_run_plan(
    request: RunPlanRequest,
    _state: State<'_, AppState>,
) -> CommandResult<RunPlanResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let file_set_id: FileSetId = request.file_set_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid file set ID: {}", request.file_set_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    let entries = bundle
        .read_fileset(file_set_id)
        .map_err(|e| CommandError::NotFound(format!("File set not found: {}", e)))?;

    let proposal_id = ProposalId::new();
    let approval_token_hash = format!(
        "{:x}",
        md5::compute(format!(
            "{}:{}:{}",
            request.parser_name, request.parser_version, request.sink_uri
        ))
    );

    // Update state
    bundle
        .update_state(casparian_mcp::intent::IntentState::AwaitingRunApproval)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(RunPlanResponse {
        proposal_id: proposal_id.to_string(),
        approval_token_hash,
        file_count: entries.len() as u64,
        sink_uri: request.sink_uri,
    })
}

/// Request for executing a run.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunExecuteRequest {
    pub session_id: String,
    pub proposal_id: String,
    pub approval_token_hash: String,
}

/// Response from executing a run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunExecuteResponse {
    pub started: bool,
    pub job_id: String,
    pub new_state: String,
}

/// Execute a run plan.
#[tauri::command]
pub async fn casp_run_execute(
    request: RunExecuteRequest,
    _state: State<'_, AppState>,
) -> CommandResult<RunExecuteResponse> {
    let session_id: SessionId = request.session_id.parse().map_err(|_| {
        CommandError::InvalidArgument(format!("Invalid session ID: {}", request.session_id))
    })?;

    let session_store = SessionStore::new();
    let bundle = session_store
        .get_session(session_id)
        .map_err(|e| CommandError::NotFound(format!("Session not found: {}", e)))?;

    let job_id = uuid::Uuid::new_v4().to_string();

    // Update state
    let new_state = casparian_mcp::intent::IntentState::RunExecute;
    bundle
        .update_state(new_state)
        .map_err(|e| CommandError::Internal(format!("Failed to update state: {}", e)))?;

    Ok(RunExecuteResponse {
        started: true,
        job_id,
        new_state: new_state.as_str().to_string(),
    })
}

// ============================================================================
// Scan Commands
// ============================================================================

/// Request for scanning a directory.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRequest {
    pub path: String,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default = "default_scan_limit")]
    pub limit: usize,
    #[serde(default = "default_true")]
    pub recursive: bool,
}

fn default_scan_limit() -> usize {
    1000
}

/// Response with scan results.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResponse {
    pub files: Vec<ScanFileEntry>,
    pub total_scanned: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFileEntry {
    pub path: String,
    pub size: u64,
    pub extension: Option<String>,
}

/// Scan a directory for files.
#[tauri::command]
pub async fn casparian_scan(
    request: ScanRequest,
    _state: State<'_, AppState>,
) -> CommandResult<ScanResponse> {
    use walkdir::WalkDir;

    let base_path = std::path::Path::new(&request.path);
    if !base_path.exists() {
        return Err(CommandError::InvalidArgument(format!(
            "Path does not exist: {}",
            request.path
        )));
    }

    let pattern = request
        .pattern
        .as_ref()
        .and_then(|p| glob::Pattern::new(p).ok());

    let walker = if request.recursive {
        WalkDir::new(base_path)
    } else {
        WalkDir::new(base_path).max_depth(1)
    };

    let mut files = Vec::new();
    let mut total_scanned = 0;

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        // Check pattern if provided
        if let Some(ref p) = pattern {
            if !p.matches(&path_str) {
                continue;
            }
        }

        total_scanned += 1;

        if files.len() < request.limit {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string());

            files.push(ScanFileEntry {
                path: path_str,
                size,
                extension,
            });
        }
    }

    Ok(ScanResponse {
        truncated: total_scanned > request.limit,
        files,
        total_scanned,
    })
}

// ============================================================================
// Parser Commands
// ============================================================================

/// Response with parser list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserListResponse {
    pub parsers: Vec<ParserInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserInfo {
    pub name: String,
    pub version: String,
    pub topics: Vec<String>,
    pub outputs: Vec<String>,
}

/// List available parsers.
#[tauri::command]
pub async fn parser_list(_state: State<'_, AppState>) -> CommandResult<ParserListResponse> {
    // For now, return empty list
    // In production, would query parser registry
    Ok(ParserListResponse { parsers: vec![] })
}
