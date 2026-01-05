//! Scout integration for Tauri
//!
//! Scout is the File Discovery + Tagging layer.
//! It discovers files and assigns tags based on patterns.
//! Actual processing happens in Sentinel (Tag → Plugin → Sink).

use casparian_scout::{
    Database as ScoutDatabase, FileStatus, ScannedFile, Scanner, Source, SourceType, TaggingRule, Tagger,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, State};
use tracing::info;

// ============================================================================
// State Management
// ============================================================================

/// Scout state managed by Tauri
pub struct ScoutState {
    /// Scout database (lazy initialized)
    database: Mutex<Option<ScoutDatabase>>,
    /// Path to the database file
    db_path: Mutex<PathBuf>,
}

impl ScoutState {
    pub fn new() -> Self {
        // SINGLE DATABASE: use casparian_flow.sqlite3 for ALL tables
        // This is shared with lib.rs (Sentinel tables) - no more scout.db
        let default_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".casparian_flow")
            .join("casparian_flow.sqlite3");

        Self {
            database: Mutex::new(None),
            db_path: Mutex::new(default_path),
        }
    }

    /// Get or initialize the database
    pub fn get_db(&self) -> Result<ScoutDatabase, String> {
        let mut db_guard = self.database.lock().map_err(|e| e.to_string())?;

        if db_guard.is_none() {
            let path = self.db_path.lock().map_err(|e| e.to_string())?;
            let db = ScoutDatabase::open(&path)
                .map_err(|e| format!("Failed to open Scout database: {}", e))?;
            *db_guard = Some(db);
        }

        Ok(db_guard.as_ref().unwrap().clone())
    }
}

impl Default for ScoutState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// API Types (Serialized to Frontend)
// ============================================================================

/// Source information for the frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub poll_interval_secs: u64,
    pub enabled: bool,
}

impl From<Source> for SourceInfo {
    fn from(s: Source) -> Self {
        Self {
            id: s.id,
            name: s.name,
            path: s.path,
            poll_interval_secs: s.poll_interval_secs,
            enabled: s.enabled,
        }
    }
}

/// Scanned file information for the frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub id: i64,
    pub source_id: String,
    pub path: String,
    pub rel_path: String,
    pub size: u64,
    pub status: String,
    pub tag: Option<String>,
    /// How the tag was assigned: "rule" or "manual"
    pub tag_source: Option<String>,
    /// ID of the tagging rule that matched (if tag_source = "rule")
    pub rule_id: Option<String>,
    /// Manual plugin override (None = use tag subscription)
    pub manual_plugin: Option<String>,
    pub error: Option<String>,
}

impl From<ScannedFile> for FileInfo {
    fn from(f: ScannedFile) -> Self {
        Self {
            id: f.id.unwrap_or(0),
            source_id: f.source_id,
            path: f.path,
            rel_path: f.rel_path,
            size: f.size,
            status: f.status.as_str().to_string(),
            tag: f.tag,
            tag_source: f.tag_source,
            rule_id: f.rule_id,
            manual_plugin: f.manual_plugin,
            error: f.error,
        }
    }
}

/// Tagging rule information for the frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaggingRuleInfo {
    pub id: String,
    pub name: String,
    pub source_id: String,
    pub pattern: String,
    pub tag: String,
    pub priority: i32,
    pub enabled: bool,
}

impl From<TaggingRule> for TaggingRuleInfo {
    fn from(r: TaggingRule) -> Self {
        Self {
            id: r.id,
            name: r.name,
            source_id: r.source_id,
            pattern: r.pattern,
            tag: r.tag,
            priority: r.priority,
            enabled: r.enabled,
        }
    }
}

/// Scan statistics
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub files_discovered: u64,
    pub files_new: u64,
    pub files_changed: u64,
    pub files_deleted: u64,
    pub bytes_scanned: u64,
    pub duration_ms: u64,
    pub errors: Vec<String>,
}

/// Pattern preview result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternPreview {
    pub pattern: String,
    pub matched_count: u64,
    pub matched_bytes: u64,
    pub sample_files: Vec<String>,
    pub is_valid: bool,
    pub error: Option<String>,
}

/// Tag coverage analysis
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCoverage {
    /// Per-rule statistics
    pub rules: Vec<TagCoverageStats>,
    /// Files not matched by any rule (untagged)
    pub untagged_count: u64,
    pub untagged_bytes: u64,
    pub untagged_samples: Vec<String>,
    /// Rules that overlap (match same files)
    pub overlaps: Vec<RuleOverlap>,
    /// Totals
    pub total_files: u64,
    pub total_bytes: u64,
    pub tagged_files: u64,
    pub tagged_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCoverageStats {
    pub rule_id: String,
    pub rule_name: String,
    pub pattern: String,
    pub tag: String,
    pub matched_count: u64,
    pub matched_bytes: u64,
    pub sample_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleOverlap {
    pub rule1_id: String,
    pub rule1_name: String,
    pub rule2_id: String,
    pub rule2_name: String,
    pub overlap_count: u64,
    pub sample_files: Vec<String>,
}

/// Overall Scout status
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutStatus {
    pub sources: u64,
    pub tagging_rules: u64,
    pub total_files: u64,
    pub pending_files: u64,
    pub tagged_files: u64,
    pub queued_files: u64,
    pub processed_files: u64,
    pub failed_files: u64,
    pub pending_bytes: u64,
    pub processed_bytes: u64,
}

/// Tag statistics for a source
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagStats {
    pub tag: String,
    pub total: u64,
    pub processed: u64,
    pub failed: u64,
}

// ============================================================================
// Database Initialization
// ============================================================================

#[tauri::command]
pub fn scout_init_db(state: State<'_, ScoutState>, path: Option<String>) -> Result<(), String> {
    // If path provided, update the stored path
    if let Some(p) = path {
        let mut path_guard = state.db_path.lock().map_err(|e| e.to_string())?;
        *path_guard = PathBuf::from(p);
    }

    // Initialize the database (get_db will create if needed)
    let _ = state.get_db()?;
    info!("Scout database initialized");
    Ok(())
}

// ============================================================================
// Source Commands
// ============================================================================

#[tauri::command]
pub fn scout_list_sources(state: State<'_, ScoutState>) -> Result<Vec<SourceInfo>, String> {
    let db = state.get_db()?;
    let sources = db
        .list_sources()
        .map_err(|e| format!("Failed to list sources: {}", e))?;
    Ok(sources.into_iter().map(|s| s.into()).collect())
}

#[tauri::command]
pub fn scout_add_source(
    state: State<'_, ScoutState>,
    id: String,
    name: String,
    path: String,
) -> Result<(), String> {
    let db = state.get_db()?;

    let source = Source {
        id,
        name,
        source_type: SourceType::Local,
        path,
        poll_interval_secs: 30,
        enabled: true,
    };

    db.upsert_source(&source)
        .map_err(|e| format!("Failed to add source: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn scout_remove_source(state: State<'_, ScoutState>, source_id: String) -> Result<(), String> {
    let db = state.get_db()?;
    db.delete_source(&source_id)
        .map_err(|e| format!("Failed to remove source: {}", e))?;
    Ok(())
}

// ============================================================================
// Tagging Rule Commands
// ============================================================================

#[tauri::command]
pub fn scout_list_tagging_rules(
    state: State<'_, ScoutState>,
) -> Result<Vec<TaggingRuleInfo>, String> {
    let db = state.get_db()?;
    let rules = db
        .list_tagging_rules()
        .map_err(|e| format!("Failed to list tagging rules: {}", e))?;
    Ok(rules.into_iter().map(|r| r.into()).collect())
}

#[tauri::command]
pub fn scout_list_tagging_rules_for_source(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<Vec<TaggingRuleInfo>, String> {
    let db = state.get_db()?;
    let rules = db
        .list_tagging_rules_for_source(&source_id)
        .map_err(|e| format!("Failed to list tagging rules: {}", e))?;
    Ok(rules.into_iter().map(|r| r.into()).collect())
}

#[tauri::command]
pub fn scout_add_tagging_rule(
    state: State<'_, ScoutState>,
    id: String,
    name: String,
    source_id: String,
    pattern: String,
    tag: String,
    priority: Option<i32>,
) -> Result<TaggingRuleInfo, String> {
    let db = state.get_db()?;

    let rule = TaggingRule {
        id,
        name,
        source_id,
        pattern,
        tag,
        priority: priority.unwrap_or(0),
        enabled: true,
    };

    db.upsert_tagging_rule(&rule)
        .map_err(|e| format!("Failed to add tagging rule: {}", e))?;

    Ok(rule.into())
}

#[tauri::command]
pub fn scout_remove_tagging_rule(
    state: State<'_, ScoutState>,
    rule_id: String,
) -> Result<(), String> {
    let db = state.get_db()?;
    db.delete_tagging_rule(&rule_id)
        .map_err(|e| format!("Failed to remove tagging rule: {}", e))?;
    Ok(())
}

// ============================================================================
// File Commands
// ============================================================================

#[tauri::command]
pub fn scout_list_files(
    state: State<'_, ScoutState>,
    source_id: String,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    let db = state.get_db()?;
    let limit = limit.unwrap_or(1000);

    let files = if let Some(status_str) = status {
        if let Some(file_status) = FileStatus::parse(&status_str) {
            db.list_files_by_source_and_status(&source_id, file_status, limit)
        } else {
            db.list_files_by_source(&source_id, limit)
        }
    } else {
        db.list_files_by_source(&source_id, limit)
    }
    .map_err(|e| format!("Failed to list files: {}", e))?;

    Ok(files.into_iter().map(FileInfo::from).collect())
}

#[tauri::command]
pub fn scout_list_untagged_files(
    state: State<'_, ScoutState>,
    source_id: String,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    let db = state.get_db()?;
    let limit = limit.unwrap_or(1000);

    let files = db
        .list_untagged_files(&source_id, limit)
        .map_err(|e| format!("Failed to list untagged files: {}", e))?;

    Ok(files.into_iter().map(FileInfo::from).collect())
}

#[tauri::command]
pub fn scout_list_failed_files(
    state: State<'_, ScoutState>,
    source_id: String,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    let db = state.get_db()?;
    let limit = limit.unwrap_or(100);

    let files = db
        .list_failed_files_for_source(&source_id, limit)
        .map_err(|e| format!("Failed to list failed files: {}", e))?;

    Ok(files.into_iter().map(FileInfo::from)
        .collect())
}

// ============================================================================
// Tagging Commands
// ============================================================================

/// Tag a single file
#[tauri::command]
pub fn scout_tag_file(
    state: State<'_, ScoutState>,
    file_id: i64,
    tag: String,
) -> Result<(), String> {
    let db = state.get_db()?;
    db.tag_file(file_id, &tag)
        .map_err(|e| format!("Failed to tag file: {}", e))?;
    info!("Tagged file {} with '{}'", file_id, tag);
    Ok(())
}

/// Tag multiple files at once
#[tauri::command]
pub fn scout_tag_files(
    state: State<'_, ScoutState>,
    file_ids: Vec<i64>,
    tag: String,
) -> Result<u64, String> {
    let db = state.get_db()?;
    let count = db
        .tag_files(&file_ids, &tag)
        .map_err(|e| format!("Failed to tag files: {}", e))?;
    info!("Tagged {} files with '{}'", count, tag);
    Ok(count)
}

/// Apply tagging rules to all pending files in a source
/// Sets tag_source='rule' and rule_id for automatic tagging
#[tauri::command]
pub fn scout_auto_tag(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<u64, String> {
    let db = state.get_db()?;

    // Get tagging rules for this source
    let rules = db
        .list_tagging_rules_for_source(&source_id)
        .map_err(|e| format!("Failed to list tagging rules: {}", e))?;

    if rules.is_empty() {
        return Ok(0);
    }

    // Create tagger
    let tagger = Tagger::new(rules).map_err(|e| format!("Failed to create tagger: {}", e))?;

    // Get pending files
    let files = db
        .list_untagged_files(&source_id, 10000)
        .map_err(|e| format!("Failed to list files: {}", e))?;

    let mut tagged_count = 0u64;

    for file in &files {
        // Use get_tag_with_rule_id to track which rule matched
        if let Some((tag, rule_id)) = tagger.get_tag_with_rule_id(file) {
            if let Some(file_id) = file.id {
                db.tag_file_by_rule(file_id, tag, rule_id)
                    .map_err(|e| format!("Failed to tag file: {}", e))?;
                tagged_count += 1;
            }
        }
    }

    info!(
        "Auto-tagged {} files in source '{}'",
        tagged_count, source_id
    );
    Ok(tagged_count)
}

/// Get tag statistics for a source
#[tauri::command]
pub fn scout_get_tag_stats(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<Vec<TagStats>, String> {
    let db = state.get_db()?;

    let stats = db
        .get_tag_stats(&source_id)
        .map_err(|e| format!("Failed to get tag stats: {}", e))?;

    Ok(stats
        .into_iter()
        .map(|(tag, total, processed, failed)| TagStats {
            tag,
            total,
            processed,
            failed,
        })
        .collect())
}

// ============================================================================
// Scan Commands
// ============================================================================

#[tauri::command]
pub fn scout_scan(state: State<'_, ScoutState>, source_id: String) -> Result<ScanStats, String> {
    let db = state.get_db()?;

    let source = db
        .get_source(&source_id)
        .map_err(|e| format!("Failed to get source: {}", e))?
        .ok_or_else(|| format!("Source not found: {}", source_id))?;

    let scanner = Scanner::new(db);
    let result = scanner
        .scan_source(&source)
        .map_err(|e| format!("Scan failed: {}", e))?;

    Ok(ScanStats {
        files_discovered: result.stats.files_discovered,
        files_new: result.stats.files_new,
        files_changed: result.stats.files_changed,
        files_deleted: result.stats.files_deleted,
        bytes_scanned: result.stats.bytes_scanned,
        duration_ms: result.stats.duration_ms,
        errors: result.errors.iter().map(|(path, err)| format!("{}: {}", path, err)).collect(),
    })
}

// ============================================================================
// Status & Coverage Commands
// ============================================================================

#[tauri::command]
pub fn scout_status(state: State<'_, ScoutState>) -> Result<ScoutStatus, String> {
    let db = state.get_db()?;

    let stats = db
        .get_stats()
        .map_err(|e| format!("Failed to get stats: {}", e))?;

    Ok(ScoutStatus {
        sources: stats.total_sources,
        tagging_rules: stats.total_tagging_rules,
        total_files: stats.total_files,
        pending_files: stats.files_pending,
        tagged_files: stats.files_tagged,
        queued_files: stats.files_queued,
        processed_files: stats.files_processed,
        failed_files: stats.files_failed,
        pending_bytes: stats.bytes_pending,
        processed_bytes: stats.bytes_processed,
    })
}

#[tauri::command]
pub fn scout_preview_pattern(
    state: State<'_, ScoutState>,
    source_id: String,
    pattern: String,
) -> Result<PatternPreview, String> {
    // Validate pattern syntax
    let compiled = match glob::Pattern::new(&pattern) {
        Ok(p) => p,
        Err(e) => {
            return Ok(PatternPreview {
                pattern,
                matched_count: 0,
                matched_bytes: 0,
                sample_files: vec![],
                is_valid: false,
                error: Some(format!("Invalid pattern: {}", e)),
            });
        }
    };

    let db = state.get_db()?;

    // Get files for this source
    let files = db
        .list_files_by_source(&source_id, 10000)
        .map_err(|e| format!("Failed to list files: {}", e))?;

    let mut matched_count = 0u64;
    let mut matched_bytes = 0u64;
    let mut sample_files = Vec::new();

    for file in &files {
        if compiled.matches(&file.rel_path) {
            matched_count += 1;
            matched_bytes += file.size;
            if sample_files.len() < 10 {
                sample_files.push(file.rel_path.clone());
            }
        }
    }

    Ok(PatternPreview {
        pattern,
        matched_count,
        matched_bytes,
        sample_files,
        is_valid: true,
        error: None,
    })
}

#[tauri::command]
pub fn scout_analyze_coverage(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<TagCoverage, String> {
    let db = state.get_db()?;

    // Get tagging rules for this source
    let rules = db
        .list_tagging_rules_for_source(&source_id)
        .map_err(|e| format!("Failed to list tagging rules: {}", e))?;

    // Compile patterns
    let compiled_patterns: Vec<(String, String, String, glob::Pattern)> = rules
        .iter()
        .filter_map(|r| {
            glob::Pattern::new(&r.pattern)
                .ok()
                .map(|p| (r.id.clone(), r.name.clone(), r.tag.clone(), p))
        })
        .collect();

    // Get files
    let files = db
        .list_files_by_source(&source_id, 100000)
        .map_err(|e| format!("Failed to list files: {}", e))?;

    // Match files against patterns
    let mut rule_matches: HashMap<String, (String, String, String, u64, u64, Vec<String>)> =
        HashMap::new();
    for (rule_id, rule_name, tag, _) in &compiled_patterns {
        rule_matches.insert(
            rule_id.clone(),
            (rule_name.clone(), tag.clone(), String::new(), 0, 0, Vec::new()),
        );
    }

    let mut overlaps: HashMap<(String, String), (u64, Vec<String>)> = HashMap::new();
    let mut untagged_count = 0u64;
    let mut untagged_bytes = 0u64;
    let mut untagged_samples = Vec::new();
    let mut total_bytes = 0u64;
    let mut tagged_files = 0u64;
    let mut tagged_bytes = 0u64;

    for file in &files {
        total_bytes += file.size;

        // Find all patterns that match this file
        let matching_rules: Vec<&str> = compiled_patterns
            .iter()
            .filter(|(_, _, _, pattern)| pattern.matches(&file.rel_path))
            .map(|(rule_id, _, _, _)| rule_id.as_str())
            .collect();

        let match_count = matching_rules.len();

        if match_count == 0 {
            untagged_count += 1;
            untagged_bytes += file.size;
            if untagged_samples.len() < 10 {
                untagged_samples.push(file.rel_path.clone());
            }
        } else {
            tagged_files += 1;
            tagged_bytes += file.size;

            // Update each matching rule's stats
            for rule_id in &matching_rules {
                if let Some((_name, _tag, pattern, count, bytes, samples)) =
                    rule_matches.get_mut(*rule_id)
                {
                    *count += 1;
                    *bytes += file.size;
                    if samples.len() < 5 {
                        samples.push(file.rel_path.clone());
                    }
                    // Get pattern from compiled_patterns
                    if pattern.is_empty() {
                        if let Some((_, _, _, p)) =
                            compiled_patterns.iter().find(|(id, _, _, _)| id == *rule_id)
                        {
                            *pattern = p.as_str().to_string();
                        }
                    }
                }
            }

            // Track overlaps
            if match_count > 1 {
                for i in 0..matching_rules.len() {
                    for j in (i + 1)..matching_rules.len() {
                        let key = if matching_rules[i] < matching_rules[j] {
                            (matching_rules[i].to_string(), matching_rules[j].to_string())
                        } else {
                            (matching_rules[j].to_string(), matching_rules[i].to_string())
                        };

                        let entry = overlaps.entry(key).or_insert((0, Vec::new()));
                        entry.0 += 1;
                        if entry.1.len() < 5 {
                            entry.1.push(file.rel_path.clone());
                        }
                    }
                }
            }
        }
    }

    // Build response
    let rules_stats: Vec<TagCoverageStats> = rule_matches
        .into_iter()
        .map(|(rule_id, (name, tag, pattern, count, bytes, samples))| TagCoverageStats {
            rule_id,
            rule_name: name,
            pattern,
            tag,
            matched_count: count,
            matched_bytes: bytes,
            sample_files: samples,
        })
        .collect();

    let overlap_stats: Vec<RuleOverlap> = overlaps
        .into_iter()
        .map(|((r1, r2), (count, samples))| {
            let r1_name = rules.iter().find(|r| r.id == r1).map(|r| r.name.clone()).unwrap_or_default();
            let r2_name = rules.iter().find(|r| r.id == r2).map(|r| r.name.clone()).unwrap_or_default();
            RuleOverlap {
                rule1_id: r1,
                rule1_name: r1_name,
                rule2_id: r2,
                rule2_name: r2_name,
                overlap_count: count,
                sample_files: samples,
            }
        })
        .collect();

    Ok(TagCoverage {
        rules: rules_stats,
        untagged_count,
        untagged_bytes,
        untagged_samples,
        overlaps: overlap_stats,
        total_files: files.len() as u64,
        total_bytes,
        tagged_files,
        tagged_bytes,
    })
}

// ============================================================================
// Retry Commands
// ============================================================================

#[tauri::command]
pub fn scout_retry_failed(state: State<'_, ScoutState>, source_id: String) -> Result<u64, String> {
    let db = state.get_db()?;
    let count = db
        .retry_failed_files(&source_id)
        .map_err(|e| format!("Failed to retry failed files: {}", e))?;
    info!("Reset {} failed files to pending in source '{}'", count, source_id);
    Ok(count)
}

// ============================================================================
// Manual Override Commands
// ============================================================================

/// Set manual plugin override for a file
#[tauri::command]
pub fn scout_set_manual_plugin(
    state: State<'_, ScoutState>,
    file_id: i64,
    plugin_name: String,
) -> Result<(), String> {
    let db = state.get_db()?;
    db.set_manual_plugin(file_id, &plugin_name)
        .map_err(|e| format!("Failed to set manual plugin: {}", e))?;
    info!("Set manual plugin '{}' for file {}", plugin_name, file_id);
    Ok(())
}

/// Clear all manual overrides for a file (reset to auto)
#[tauri::command]
pub fn scout_clear_manual_overrides(
    state: State<'_, ScoutState>,
    file_id: i64,
) -> Result<(), String> {
    let db = state.get_db()?;
    db.clear_manual_overrides(file_id)
        .map_err(|e| format!("Failed to clear manual overrides: {}", e))?;
    info!("Cleared manual overrides for file {}", file_id);
    Ok(())
}

/// List files with manual overrides (tag_source = 'manual' OR manual_plugin IS NOT NULL)
#[tauri::command]
pub fn scout_list_manual_files(
    state: State<'_, ScoutState>,
    source_id: String,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    let db = state.get_db()?;
    let limit = limit.unwrap_or(1000);

    let files = db
        .list_manual_files(&source_id, limit)
        .map_err(|e| format!("Failed to list manual files: {}", e))?;

    Ok(files.into_iter().map(FileInfo::from).collect())
}

/// Get detailed file information including matched rule
#[tauri::command]
pub fn scout_get_file(
    state: State<'_, ScoutState>,
    file_id: i64,
) -> Result<Option<FileInfo>, String> {
    let db = state.get_db()?;
    let file = db
        .get_file(file_id)
        .map_err(|e| format!("Failed to get file: {}", e))?;
    Ok(file.map(FileInfo::from))
}

// ============================================================================
// Shredder Commands
// ============================================================================

use casparian_worker::{analyzer, shredder};
use cf_protocol::{ShredConfig, ShredStrategy, DetectionConfidence};
use chrono::Utc;

/// Shredder analysis result for frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShredAnalysisResult {
    pub strategy: ShredStrategyInfo,
    pub confidence: String,
    pub sample_keys: Vec<String>,
    pub estimated_shard_count: usize,
    pub head_bytes: usize,
    pub reasoning: String,
    pub warning: Option<String>,
}

/// Shred strategy info for frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShredStrategyInfo {
    pub strategy_type: String,
    pub delimiter: Option<String>,
    pub col_index: Option<usize>,
    pub has_header: Option<bool>,
    pub key_path: Option<String>,
    pub pattern: Option<String>,
    pub key_group: Option<String>,
}

impl From<&ShredStrategy> for ShredStrategyInfo {
    fn from(s: &ShredStrategy) -> Self {
        match s {
            ShredStrategy::CsvColumn { delimiter, col_index, has_header } => Self {
                strategy_type: "CsvColumn".to_string(),
                delimiter: Some((*delimiter as char).to_string()),
                col_index: Some(*col_index),
                has_header: Some(*has_header),
                key_path: None,
                pattern: None,
                key_group: None,
            },
            ShredStrategy::JsonKey { key_path } => Self {
                strategy_type: "JsonKey".to_string(),
                delimiter: None,
                col_index: None,
                has_header: None,
                key_path: Some(key_path.clone()),
                pattern: None,
                key_group: None,
            },
            ShredStrategy::Regex { pattern, key_group } => Self {
                strategy_type: "Regex".to_string(),
                delimiter: None,
                col_index: None,
                has_header: None,
                key_path: None,
                pattern: Some(pattern.clone()),
                key_group: Some(key_group.clone()),
            },
            ShredStrategy::Passthrough => Self {
                strategy_type: "Passthrough".to_string(),
                delimiter: None,
                col_index: None,
                has_header: None,
                key_path: None,
                pattern: None,
                key_group: None,
            },
        }
    }
}

/// Shred result for frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShredResultInfo {
    pub shards: Vec<ShardInfo>,
    pub freezer_path: Option<String>,
    pub freezer_key_count: usize,
    pub total_rows: u64,
    pub duration_ms: u64,
    pub lineage_index_path: String,
}

/// Shard info for frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShardInfo {
    pub path: String,
    pub key: String,
    pub row_count: u64,
    pub byte_size: u64,
    pub has_header: bool,
}

/// Full file analysis result for frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullAnalysisInfo {
    /// Map of key -> row count
    pub key_counts: Vec<(String, u64)>,
    pub total_rows: u64,
    pub bytes_scanned: u64,
    pub duration_ms: u64,
}

/// Analyze a file to detect format and propose shred strategy (heuristic-based, fast)
#[tauri::command]
pub fn shredder_analyze(path: String) -> Result<ShredAnalysisResult, String> {
    let path = std::path::Path::new(&path);

    let result = analyzer::analyze_file_head(path)
        .map_err(|e| format!("Analysis failed: {}", e))?;

    let confidence = match result.confidence {
        DetectionConfidence::High => "High",
        DetectionConfidence::Medium => "Medium",
        DetectionConfidence::Low => "Low",
        DetectionConfidence::Unknown => "Unknown",
    };

    Ok(ShredAnalysisResult {
        strategy: ShredStrategyInfo::from(&result.strategy),
        confidence: confidence.to_string(),
        sample_keys: result.sample_keys,
        estimated_shard_count: result.estimated_shard_count,
        head_bytes: result.head_bytes,
        reasoning: result.reasoning,
        warning: result.warning,
    })
}

/// LLM-powered file analysis - uses Claude to understand the data structure
/// This is smarter than heuristics and can handle arbitrary formats
#[tauri::command]
pub fn shredder_analyze_smart(path: String) -> Result<ShredAnalysisResult, String> {
    // Use the chat system with an empty conversation
    let response = shredder_chat(path, "[]".to_string(), None)?;

    // Extract strategy from response if available
    if let Some(strategy) = response.proposed_strategy {
        Ok(ShredAnalysisResult {
            strategy,
            confidence: "High".to_string(),
            sample_keys: vec![],
            estimated_shard_count: 0,
            head_bytes: 0,
            reasoning: response.message,
            warning: None,
        })
    } else {
        // No strategy yet - return the message as reasoning
        Ok(ShredAnalysisResult {
            strategy: ShredStrategyInfo {
                strategy_type: "Unknown".to_string(),
                delimiter: None,
                col_index: None,
                has_header: None,
                key_path: None,
                pattern: None,
                key_group: None,
            },
            confidence: "Low".to_string(),
            sample_keys: vec![],
            estimated_shard_count: 0,
            head_bytes: 0,
            reasoning: response.message,
            warning: Some("Needs more information - use chat to clarify".to_string()),
        })
    }
}

/// Chat message for conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,  // "user" or "assistant"
    pub content: String,
}

/// Response from shredder chat
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub message: String,
    pub proposed_strategy: Option<ShredStrategyInfo>,
    pub suggested_replies: Vec<String>,
    pub is_ready: bool,
    pub sample_preview: Vec<String>,
}

/// Interactive chat for file analysis - user can guide the LLM
#[tauri::command]
pub fn shredder_chat(
    path: String,
    messages_json: String,  // JSON array of ChatMessage
    user_input: Option<String>,
) -> Result<ChatResponse, String> {
    // Parse messages from JSON
    let messages: Vec<ChatMessage> = if messages_json.is_empty() || messages_json == "[]" {
        vec![]
    } else {
        serde_json::from_str(&messages_json)
            .map_err(|e| format!("Failed to parse messages: {}", e))?
    };
    // Read sample lines
    let sample_lines = read_sample_rows(&path, 30)
        .map_err(|e| format!("Failed to read sample: {}", e))?;

    if sample_lines.is_empty() {
        return Err("File is empty".to_string());
    }

    let sample_text = sample_lines.join("\n");

    // Build conversation history for context
    let mut conversation_context = String::new();
    for msg in &messages {
        conversation_context.push_str(&format!("{}: {}\n\n", msg.role.to_uppercase(), msg.content));
    }

    // Add user's new input if provided
    if let Some(input) = &user_input {
        conversation_context.push_str(&format!("USER: {}\n\n", input));
    }

    // Build the prompt
    let prompt = if messages.is_empty() && user_input.is_none() {
        // Initial analysis - no conversation yet
        format!(r#"You are helping a user split a data file into separate files by message type.

Here's a sample of the file (first 30 lines):
```
{}
```

Analyze this data and explain what you see. Identify:
- The delimiter used (comma, tab, pipe, etc.)
- Whether there's a header row
- Which column appears to contain the message type/category

Ask the user if your understanding is correct before proposing a final strategy.

Keep your response conversational and brief (2-3 short paragraphs). End with a question to confirm your understanding.

If you're confident about the structure, include this JSON at the END of your response (after your conversational message):
STRATEGY_JSON: {{"delimiter": ",", "has_header": false, "shard_column": 1}}

Only include STRATEGY_JSON if you're reasonably confident. It's okay to ask clarifying questions first."#, sample_text)
    } else {
        // Continuing conversation
        format!(r#"You are helping a user split a data file into separate files by message type.

File sample (first 30 lines):
```
{}
```

Previous conversation:
{}

Continue the conversation. Help the user configure how to split this file.

Keep responses brief and focused. If the user has confirmed a configuration, include:
STRATEGY_JSON: {{"delimiter": ",", "has_header": false, "shard_column": 1}}

Suggest 2-3 quick reply options the user might want to choose from."#, sample_text, conversation_context)
    };

    // Call Claude
    let llm_response = call_claude_cli(&prompt)?;

    // Parse the response - look for STRATEGY_JSON if present
    let (message, strategy) = parse_chat_response(&llm_response);

    // Generate suggested replies based on context
    let suggested_replies = if strategy.is_some() {
        vec![
            "Looks good, proceed".to_string(),
            "Change the column".to_string(),
            "Show me more samples".to_string(),
        ]
    } else {
        vec![
            "Yes, that's correct".to_string(),
            "No, let me explain".to_string(),
            "Show column values".to_string(),
        ]
    };

    Ok(ChatResponse {
        message,
        proposed_strategy: strategy,
        suggested_replies,
        is_ready: false,  // User must explicitly confirm
        sample_preview: sample_lines.iter().take(5).cloned().collect(),
    })
}

/// Parse chat response to extract message and optional strategy JSON
fn parse_chat_response(response: &str) -> (String, Option<ShredStrategyInfo>) {
    // Look for STRATEGY_JSON: {...}
    if let Some(json_start) = response.find("STRATEGY_JSON:") {
        let json_part = &response[json_start + 14..].trim();

        // Find the JSON object
        if let Some(end) = json_part.find('}') {
            let json_str = &json_part[..=end];

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                let strategy = ShredStrategyInfo {
                    strategy_type: "CsvColumn".to_string(),
                    delimiter: parsed["delimiter"].as_str().map(String::from),
                    col_index: parsed["shard_column"].as_u64().map(|n| n as usize),
                    has_header: parsed["has_header"].as_bool(),
                    key_path: None,
                    pattern: None,
                    key_group: None,
                };

                // Remove the JSON part from the message
                let message = response[..json_start].trim().to_string();
                return (message, Some(strategy));
            }
        }
    }

    // No strategy found, return full message
    (response.trim().to_string(), None)
}

/// Analyze entire file to get complete key distribution.
/// This scans the whole file (can be slow for large files) but gives
/// accurate counts of all shard keys that will be created.
#[tauri::command]
pub fn shredder_analyze_full(
    path: String,
    col_index: usize,
    delimiter: String,
    has_header: bool,
) -> Result<FullAnalysisInfo, String> {
    let input_path = std::path::Path::new(&path);

    // Parse delimiter
    let delim_byte = match delimiter.as_str() {
        "," => b',',
        "\\t" | "tab" | "\t" => b'\t',
        "|" => b'|',
        ";" => b';',
        s if s.len() == 1 => s.as_bytes()[0],
        _ => return Err(format!("Invalid delimiter: {}", delimiter)),
    };

    let strategy = ShredStrategy::CsvColumn {
        delimiter: delim_byte,
        col_index,
        has_header,
    };

    let result = analyzer::analyze_file_full(input_path, &strategy)
        .map_err(|e| format!("Full analysis failed: {}", e))?;

    // Sort by count descending
    let mut key_counts: Vec<(String, u64)> = result.all_keys.into_iter().collect();
    key_counts.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(FullAnalysisInfo {
        key_counts,
        total_rows: result.total_rows,
        bytes_scanned: result.bytes_scanned,
        duration_ms: result.duration_ms,
    })
}

/// Execute shredding with given parameters
#[tauri::command]
pub fn shredder_run(
    state: State<'_, ScoutState>,
    path: String,
    output_dir: String,
    col_index: usize,
    delimiter: String,
    has_header: bool,
    top_n: Option<usize>,
) -> Result<ShredResultInfo, String> {
    let input_path = std::path::Path::new(&path);
    let output_path = std::path::PathBuf::from(&output_dir);

    // Parse delimiter
    let delim_byte = match delimiter.as_str() {
        "," => b',',
        "\\t" | "tab" | "\t" => b'\t',
        "|" => b'|',
        ";" => b';',
        s if s.len() == 1 => s.as_bytes()[0],
        _ => return Err(format!("Invalid delimiter: {}", delimiter)),
    };

    let strategy = ShredStrategy::CsvColumn {
        delimiter: delim_byte,
        col_index,
        has_header,
    };

    let config = ShredConfig {
        strategy,
        output_dir: output_path,
        max_handles: 200,
        top_n_shards: top_n.unwrap_or(5),
        buffer_size: 65536,
        promotion_threshold: 1000,
    };

    let shredder_instance = shredder::Shredder::new(config);
    let result = shredder_instance.shred(input_path)
        .map_err(|e| format!("Shredding failed: {}", e))?;

    // Register shards as Scout files
    let db = state.get_db()?;

    // Ensure "shredder" source exists (required for foreign key)
    let shredder_source = Source {
        id: "shredder".to_string(),
        name: "Shredder Output".to_string(),
        source_type: SourceType::Local,
        path: output_dir.clone(),
        poll_interval_secs: 0,  // No polling - files are added explicitly
        enabled: true,
    };
    db.upsert_source(&shredder_source)
        .map_err(|e| format!("Failed to create shredder source: {}", e))?;

    for shard in &result.shards {
        let shard_file = ScannedFile {
            id: None,
            source_id: "shredder".to_string(),
            path: shard.path.to_string_lossy().to_string(),
            rel_path: shard.path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            size: shard.byte_size,
            mtime: Utc::now().timestamp(),
            first_seen_at: Utc::now(),
            last_seen_at: Utc::now(),
            processed_at: None,
            sentinel_job_id: None,
            content_hash: None,
            status: FileStatus::Tagged,
            tag: Some(shard.key.clone()),
            tag_source: Some("shredder".to_string()),
            rule_id: None,
            manual_plugin: None,
            error: None,
        };

        db.upsert_file(&shard_file)
            .map_err(|e| format!("Failed to register shard as Scout file: {}", e))?;
    }

    info!(
        "Shredded {} into {} shards, registered in Scout",
        path,
        result.shards.len()
    );

    Ok(ShredResultInfo {
        shards: result.shards.iter().map(|s| ShardInfo {
            path: s.path.to_string_lossy().to_string(),
            key: s.key.clone(),
            row_count: s.row_count,
            byte_size: s.byte_size,
            has_header: s.has_header,
        }).collect(),
        freezer_path: result.freezer_path.map(|p| p.to_string_lossy().to_string()),
        freezer_key_count: result.freezer_key_count,
        total_rows: result.total_rows,
        duration_ms: result.duration_ms,
        lineage_index_path: result.lineage_index_path.to_string_lossy().to_string(),
    })
}

// ============================================================================
// Parser Generation Commands
// ============================================================================

/// LLM config for frontend
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigInfo {
    pub provider: String,
    pub api_key: Option<String>,
    pub model: String,
}

/// Parser draft for frontend
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserDraftInfo {
    pub shard_key: String,
    pub source_code: String,
    pub sample_input: Vec<String>,
    pub sample_output: Option<String>,
    pub validation_error: Option<String>,
    /// "single" for DataFrame return, "multi" for dict[str, DataFrame]
    pub output_mode: String,
    /// Detected topic names when output_mode is "multi"
    pub detected_topics: Option<Vec<String>>,
}

/// Save LLM configuration
#[tauri::command]
pub fn save_llm_config(
    state: State<'_, ScoutState>,
    config: LlmConfigInfo,
) -> Result<(), String> {
    let db = state.get_db()?;

    // Store config in settings table
    db.set_setting("llm_provider", &config.provider)
        .map_err(|e| format!("Failed to save LLM provider: {}", e))?;

    if let Some(key) = &config.api_key {
        db.set_setting("llm_api_key", key)
            .map_err(|e| format!("Failed to save API key: {}", e))?;
    }

    db.set_setting("llm_model", &config.model)
        .map_err(|e| format!("Failed to save model: {}", e))?;

    info!("Saved LLM config: provider={}", config.provider);
    Ok(())
}

/// Load LLM configuration
#[tauri::command]
pub fn load_llm_config(state: State<'_, ScoutState>) -> Result<LlmConfigInfo, String> {
    let db = state.get_db()?;

    let provider = db.get_setting("llm_provider")
        .map_err(|e| format!("Failed to load LLM provider: {}", e))?
        .unwrap_or_else(|| "anthropic".to_string());

    let api_key = db.get_setting("llm_api_key")
        .map_err(|e| format!("Failed to load API key: {}", e))?;

    let model = db.get_setting("llm_model")
        .map_err(|e| format!("Failed to load model: {}", e))?
        .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

    Ok(LlmConfigInfo {
        provider,
        api_key,
        model,
    })
}

/// Generate a parser draft for a shard file using Claude Code CLI
#[tauri::command]
pub async fn generate_parser_draft(
    _state: State<'_, ScoutState>,
    shard_path: String,
) -> Result<ParserDraftInfo, String> {
    // Read sample rows from shard
    let sample_rows = read_sample_rows(&shard_path, 10)?;

    if sample_rows.is_empty() {
        return Err("Shard file is empty".to_string());
    }

    // Extract shard key from filename
    let shard_key = std::path::Path::new(&shard_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Build prompt for Claude
    let prompt = build_parser_prompt(&sample_rows, &shard_key);

    // Call Claude Code CLI
    let source_code = call_claude_cli(&prompt)?;

    Ok(ParserDraftInfo {
        shard_key,
        source_code,
        sample_input: sample_rows,
        sample_output: None,
        validation_error: None,
        output_mode: "single".to_string(),  // Not validated yet, default to single
        detected_topics: None,
    })
}

/// Call Claude Code CLI to generate code
fn call_claude_cli(prompt: &str) -> Result<String, String> {
    use std::process::Command;

    // Try to find claude in PATH
    // Use --tools "" to disable tool use and get immediate text response
    let output = Command::new("claude")
        .arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("text")
        .arg("--tools")
        .arg("")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "Claude Code CLI not found. Please install it with: npm install -g @anthropic-ai/claude-code".to_string()
            } else {
                format!("Failed to run Claude Code CLI: {}", e)
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude Code CLI error: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Clean up the code (remove markdown if present)
    let code = stdout
        .trim()
        .trim_start_matches("```python")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    if code.is_empty() {
        return Err("Claude Code CLI returned empty response".to_string());
    }

    Ok(code)
}

/// Read first N rows from a file
fn read_sample_rows(path: &str, n: usize) -> Result<Vec<String>, String> {
    use std::io::{BufRead, BufReader};
    use std::fs::File;

    let file = File::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;
    let reader = BufReader::new(file);

    let rows: Vec<String> = reader.lines()
        .take(n)
        .filter_map(|l| l.ok())
        .collect();

    Ok(rows)
}

/// Build the prompt for parser generation
fn build_parser_prompt(sample_rows: &[String], shard_key: &str) -> String {
    let sample = sample_rows.join("\n");

    format!(r#"You are a data engineer. Generate a Python parser for this CSV data.

Shard key: {shard_key}

Sample data (first {n} rows):
```
{sample}
```

Requirements:
1. Use polars for performance
2. Define a function: def parse(path: str) -> pl.DataFrame
3. Cast columns to appropriate types (datetime, float, int, string)
4. Handle common errors gracefully
5. The first row appears to be a header

Output ONLY the Python code, no explanation or markdown code blocks.
Start directly with "import polars as pl".
"#, n = sample_rows.len())
}

/// Get app data directory for shredder output
/// Returns path like ~/.local/share/casparian/shards on Linux
/// or ~/Library/Application Support/casparian/shards on macOS
#[tauri::command]
pub fn get_shredder_output_dir() -> Result<String, String> {
    let base = dirs::data_local_dir()
        .ok_or("Could not determine app data directory")?;

    let shards_dir = base.join("casparian").join("shards");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&shards_dir)
        .map_err(|e| format!("Failed to create shards directory: {}", e))?;

    Ok(shards_dir.to_string_lossy().to_string())
}

/// Get the parsers directory path (for file dialog default)
#[tauri::command]
pub fn get_parsers_dir() -> Result<String, String> {
    let home = dirs::home_dir()
        .ok_or_else(|| "Could not find home directory".to_string())?;

    let parsers_dir = home.join(".casparian_flow").join("parsers");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&parsers_dir)
        .map_err(|e| format!("Failed to create parsers directory: {}", e))?;

    Ok(parsers_dir.to_string_lossy().to_string())
}

// ============================================================================
// ShredderEnvManager - Managed Python environment for parser validation
// ============================================================================

/// Manages a dedicated Python virtual environment for shredder parser validation.
/// Uses `uv` (Astral's fast Python package manager) to create and manage the env.
///
/// Environment location: ~/.casparian_flow/shredder_env
/// Pre-installed packages: polars, pandas, pyarrow
struct ShredderEnvManager {
    /// Path to the virtual environment
    env_path: PathBuf,
    /// Path to the Python interpreter within the venv
    python_path: PathBuf,
}

impl ShredderEnvManager {
    /// Create a new ShredderEnvManager with default path
    fn new() -> Result<Self, String> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| "Could not determine home directory")?;

        let env_path = PathBuf::from(&home).join(".casparian_flow").join("shredder_env");
        let python_path = if cfg!(windows) {
            env_path.join("Scripts").join("python.exe")
        } else {
            env_path.join("bin").join("python")
        };

        Ok(Self { env_path, python_path })
    }

    /// Ensure the environment is initialized with required packages.
    /// Creates the venv and installs packages if not already present.
    /// Returns quickly if already initialized (cached).
    fn ensure_initialized(&self) -> Result<(), String> {
        use std::process::Command;

        // Quick check: if Python interpreter exists, env is ready
        if self.python_path.exists() {
            return Ok(());
        }

        info!("ShredderEnvManager: Setting up Python environment...");

        // Find uv
        let uv = find_uv()?;

        // Create parent directory
        if let Some(parent) = self.env_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create shredder env directory: {}", e))?;
        }

        // Create venv
        info!("ShredderEnvManager: Creating virtual environment at {}", self.env_path.display());
        let output = Command::new(&uv)
            .args(["venv", &self.env_path.to_string_lossy()])
            .output()
            .map_err(|e| format!("Failed to run uv venv: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "uv venv failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Install required packages
        info!("ShredderEnvManager: Installing packages (polars, pandas, pyarrow)...");
        let output = Command::new(&uv)
            .args([
                "pip", "install",
                "--python", &self.python_path.to_string_lossy(),
                "polars", "pandas", "pyarrow"
            ])
            .output()
            .map_err(|e| format!("Failed to run uv pip install: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "uv pip install failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        info!("ShredderEnvManager: Environment ready");
        Ok(())
    }

    /// Get the path to the Python interpreter
    fn python(&self) -> &PathBuf {
        &self.python_path
    }
}

/// Find the uv binary (Astral's Python package manager)
fn find_uv() -> Result<PathBuf, String> {
    // Check PATH first
    if let Ok(path) = which::which("uv") {
        return Ok(path);
    }

    // Check common locations
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.cargo/bin/uv", home),
        format!("{}/.local/bin/uv", home),
        "/usr/local/bin/uv".to_string(),
        "/opt/homebrew/bin/uv".to_string(),
    ];

    for candidate in candidates {
        let path = PathBuf::from(&candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    Err("uv not found. Install: curl -LsSf https://astral.sh/uv/install.sh | sh".to_string())
}

/// Get the shredder Python environment (lazy singleton pattern)
fn get_shredder_env() -> Result<ShredderEnvManager, String> {
    ShredderEnvManager::new()
}

/// Preview first N rows of a shard file
#[tauri::command]
pub fn preview_shard(path: String, num_rows: Option<usize>) -> Result<Vec<String>, String> {
    let n = num_rows.unwrap_or(5);
    read_sample_rows(&path, n)
}

/// Validate a parser by running it on sample data
#[tauri::command]
pub async fn validate_parser(
    shard_path: String,
    source_code: String,
) -> Result<ParserDraftInfo, String> {
    use std::process::Command;

    // Extract shard key from filename
    let shard_key = std::path::Path::new(&shard_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Read sample input
    let sample_input = read_sample_rows(&shard_path, 10)?;

    // Create temp file with parser code + test harness
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join("parser_test.py");

    let test_script = format!(r#"
{source_code}

# Test harness with multi-output detection
if __name__ == "__main__":
    import sys
    import json
    import re

    # Topic name validation pattern: lowercase, alphanumeric + underscore, starts with letter
    TOPIC_PATTERN = re.compile(r'^[a-z][a-z0-9_]*$')

    try:
        result = parse("{shard_path}")

        # Detect return type
        if isinstance(result, dict):
            # Validate all values are DataFrames
            tables = []
            for topic_name, df in result.items():
                # Validate topic name format
                if not TOPIC_PATTERN.match(topic_name):
                    print(f"ERROR: Invalid topic name '{{topic_name}}'. Must be lowercase, alphanumeric + underscore, start with letter. Example: 'line_items'")
                    sys.exit(1)
                if df is None:
                    print(f"ERROR: Topic '{{topic_name}}' returned None instead of DataFrame")
                    sys.exit(1)
                if not hasattr(df, 'head') or not hasattr(df, '__len__'):
                    print(f"ERROR: Topic '{{topic_name}}' is not a DataFrame (got {{type(df).__name__}})")
                    sys.exit(1)
                tables.append({{
                    "name": topic_name,
                    "rows": len(df),
                    "preview": df.head(5).to_pandas().to_string()
                }})

            if len(tables) == 0:
                print("ERROR: Parser returned empty dict - no tables")
                sys.exit(1)

            # Output as JSON for reliable parsing
            output = {{"mode": "multi", "tables": tables}}
            print("SUCCESS:" + json.dumps(output))
        else:
            # Single output: DataFrame
            if result is None:
                print("ERROR: Parser returned None")
                sys.exit(1)
            output = {{"mode": "single", "preview": result.head(5).to_pandas().to_string()}}
            print("SUCCESS:" + json.dumps(output))
    except Exception as e:
        print(f"ERROR: {{e}}")
        sys.exit(1)
"#);

    std::fs::write(&script_path, &test_script)
        .map_err(|e| format!("Failed to write test script: {}", e))?;

    // Get the shredder Python environment (with polars, pandas, pyarrow)
    let env = get_shredder_env()?;
    env.ensure_initialized()?;

    // Run the parser using the managed Python environment
    let output = Command::new(env.python())
        .arg(&script_path)
        .output()
        .map_err(|e| format!("Failed to run parser: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Clean up
    let _ = std::fs::remove_file(&script_path);

    if output.status.success() && stdout.starts_with("SUCCESS:") {
        // Parse JSON output from harness
        let json_str = stdout.trim_start_matches("SUCCESS:");

        #[derive(serde::Deserialize)]
        struct TableInfo {
            name: String,
            rows: i64,
            preview: String,
        }

        #[derive(serde::Deserialize)]
        struct HarnessOutput {
            mode: String,
            tables: Option<Vec<TableInfo>>,
            preview: Option<String>,
        }

        let parsed: Result<HarnessOutput, _> = serde_json::from_str(json_str);

        match parsed {
            Ok(harness_output) => {
                let output_mode = harness_output.mode.clone();
                let (sample_output, detected_topics) = if output_mode == "multi" {
                    // Multi-output: format tables for display
                    let tables = harness_output.tables.unwrap_or_default();
                    let topics: Vec<String> = tables.iter().map(|t| t.name.clone()).collect();
                    let formatted: Vec<String> = tables.iter().map(|t| {
                        format!("=== {} ({} rows) ===\n{}", t.name, t.rows, t.preview)
                    }).collect();
                    (formatted.join("\n\n"), Some(topics))
                } else {
                    // Single output
                    (harness_output.preview.unwrap_or_default(), None)
                };

                Ok(ParserDraftInfo {
                    shard_key,
                    source_code,
                    sample_input,
                    sample_output: Some(sample_output),
                    validation_error: None,
                    output_mode,
                    detected_topics,
                })
            }
            Err(e) => {
                Ok(ParserDraftInfo {
                    shard_key,
                    source_code,
                    sample_input,
                    sample_output: None,
                    validation_error: Some(format!("Failed to parse harness output: {}", e)),
                    output_mode: "single".to_string(),
                    detected_topics: None,
                })
            }
        }
    } else {
        let error = if stderr.is_empty() {
            stdout.trim_start_matches("ERROR: ").to_string()
        } else {
            stderr
        };

        Ok(ParserDraftInfo {
            shard_key,
            source_code,
            sample_input,
            sample_output: None,
            validation_error: Some(error),
            output_mode: "single".to_string(),
            detected_topics: None,
        })
    }
}

// ============================================================================
// Parser Refinement Chat Commands
// ============================================================================

/// Response from parser refinement chat
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserChatResponse {
    /// LLM's response text (analysis, explanation)
    pub message: String,
    /// Updated parser code (if any)
    pub refined_code: Option<String>,
    /// Quick-reply buttons for user
    pub suggested_replies: Vec<String>,
    /// Should UI auto-validate after receiving this?
    pub auto_validate: bool,
}

/// Interactive chat for parser refinement - user can guide the LLM to fix parser issues
#[tauri::command]
pub fn parser_refinement_chat(
    shard_path: String,
    parser_code: String,
    validation_error: Option<String>,
    messages_json: String,
    user_input: Option<String>,
) -> Result<ParserChatResponse, String> {
    // Parse messages from JSON
    let messages: Vec<ChatMessage> = if messages_json.is_empty() || messages_json == "[]" {
        vec![]
    } else {
        serde_json::from_str(&messages_json)
            .map_err(|e| format!("Failed to parse messages: {}", e))?
    };

    // Read sample lines
    let sample_lines = read_sample_rows(&shard_path, 15)
        .map_err(|e| format!("Failed to read sample: {}", e))?;

    if sample_lines.is_empty() {
        return Err("File is empty".to_string());
    }

    // Build conversation history for context
    let mut conversation_context = String::new();
    for msg in &messages {
        conversation_context.push_str(&format!("{}: {}\n\n", msg.role.to_uppercase(), msg.content));
    }

    // Build the prompt
    let prompt = build_parser_refinement_prompt(
        &sample_lines,
        &parser_code,
        validation_error.as_deref(),
        &conversation_context,
        user_input.as_deref().unwrap_or("Please analyze this parser and help fix any issues."),
    );

    // Call Claude
    let llm_response = call_claude_cli(&prompt)?;

    // Parse the response
    let response = parse_parser_chat_response(&llm_response);

    Ok(response)
}

/// Build the prompt for parser refinement
fn build_parser_refinement_prompt(
    sample_data: &[String],
    current_code: &str,
    validation_error: Option<&str>,
    conversation_history: &str,
    user_input: &str,
) -> String {
    let sample = sample_data.join("\n");

    let validation_section = validation_error
        .map(|e| format!("## Validation Error\n```\n{}\n```\n\nThis error occurred when running the parser. Analyze it and suggest fixes.", e))
        .unwrap_or_default();

    format!(r#"You are helping refine a Python parser for CSV data.

## Sample Data (first 15 rows)
```
{sample}
```

## Current Parser Code
```python
{current_code}
```

{validation_section}

## Conversation History
{conversation_history}

## User's Message
{user_input}

## Instructions
1. Analyze the error and user feedback
2. Explain what went wrong in a brief, clear way
3. If you have a code fix, include it in a REFINED_CODE block:

REFINED_CODE:
```python
# your fixed code here - must include complete parse() function
```

4. Suggest 2-3 quick reply options the user might want

SUGGESTED_REPLIES: ["option1", "option2", "option3"]

Keep your explanation brief (2-3 sentences). Focus on the fix.
If the parser works correctly, congratulate the user and suggest they approve it."#)
}

/// Parse the LLM response to extract message, code, and suggested replies
fn parse_parser_chat_response(response: &str) -> ParserChatResponse {
    let mut refined_code = None;
    let mut suggested_replies = vec![
        "Validate again".to_string(),
        "Show more sample data".to_string(),
        "I'll fix it manually".to_string(),
    ];
    let mut message = response.to_string();

    // Extract REFINED_CODE: block if present
    if let Some(code_start) = response.find("REFINED_CODE:") {
        let after_marker = &response[code_start + 13..];

        // Find the Python code block
        if let Some(block_start) = after_marker.find("```python") {
            let code_section = &after_marker[block_start + 9..];
            if let Some(block_end) = code_section.find("```") {
                let code = code_section[..block_end].trim().to_string();
                if !code.is_empty() {
                    refined_code = Some(code);
                }
            }
        } else if let Some(block_start) = after_marker.find("```") {
            // Plain code block without language specifier
            let code_section = &after_marker[block_start + 3..];
            if let Some(block_end) = code_section.find("```") {
                let code = code_section[..block_end].trim().to_string();
                if !code.is_empty() {
                    refined_code = Some(code);
                }
            }
        }

        // Remove the REFINED_CODE section from message
        message = response[..code_start].trim().to_string();
    }

    // Extract SUGGESTED_REPLIES: JSON array if present
    if let Some(replies_start) = response.find("SUGGESTED_REPLIES:") {
        let after_marker = &response[replies_start + 18..].trim();

        // Find the JSON array
        if let Some(array_start) = after_marker.find('[') {
            if let Some(array_end) = after_marker[array_start..].find(']') {
                let json_str = &after_marker[array_start..=array_start + array_end];
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(json_str) {
                    if !parsed.is_empty() {
                        suggested_replies = parsed;
                    }
                }
            }
        }

        // Remove from message if not already removed
        if let Some(idx) = message.find("SUGGESTED_REPLIES:") {
            message = message[..idx].trim().to_string();
        }
    }

    // Clean up any remaining markdown artifacts
    message = message
        .trim()
        .trim_end_matches("```")
        .trim()
        .to_string();

    ParserChatResponse {
        message,
        refined_code: refined_code.clone(),
        suggested_replies,
        auto_validate: refined_code.is_some(),
    }
}

// ============================================================================
// Schema Proposal & Parser Publishing Commands
// ============================================================================

/// A column in the proposed schema
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaColumn {
    pub name: String,
    pub inferred_type: String, // "string"|"int64"|"float64"|"datetime"|"boolean"
    pub nullable: bool,
    pub description: Option<String>,
}

/// Schema proposal from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaProposal {
    pub columns: Vec<SchemaColumn>,
    pub suggested_sink: String,        // "parquet" | "csv" | "sqlite"
    pub suggested_output_path: String,
    pub reasoning: String,
}

/// Receipt from publishing a parser
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserPublishReceipt {
    pub success: bool,
    pub plugin_name: String,
    pub parser_file_path: String,
    pub manifest_id: Option<i64>,
    pub config_id: Option<i64>,
    pub topic_config_id: Option<i64>,
    pub message: String,
}

/// Use LLM to propose a schema based on sample parser output
#[tauri::command]
pub fn propose_schema(
    sample_output: String,
    shard_key: String,
) -> Result<SchemaProposal, String> {
    let prompt = build_schema_proposal_prompt(&sample_output, &shard_key);
    let response = call_claude_cli(&prompt)?;
    parse_schema_proposal(&response, &shard_key)
}

fn build_schema_proposal_prompt(sample_output: &str, shard_key: &str) -> String {
    // Get home directory for output path suggestion
    let home_dir = dirs::home_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~".to_string());

    format!(r#"You are analyzing parser output to propose a schema.

## Sample Output (DataFrame as string)
```
{sample_output}
```

## Shard Key: {shard_key}

Based on this output, propose:
1. Schema: For each column, provide:
   - name: column name in snake_case
   - inferredType: one of "string", "int64", "float64", "datetime", "boolean"
   - nullable: true or false
   - description: brief description of what this column contains

2. Sink Type: recommend one of:
   - "parquet" - Best for analytics (typed, compressed, columnar)
   - "csv" - Best for interoperability
   - "sqlite" - Best for queryable storage

3. Output Path: suggest a path like "{home_dir}/.casparian_flow/output/{shard_key}/{shard_key}.parquet"

Return ONLY valid JSON (no markdown, no code blocks, no explanation):
{{"columns": [{{"name": "col1", "inferredType": "string", "nullable": false, "description": "..."}}], "suggestedSink": "parquet", "suggestedOutputPath": "...", "reasoning": "Brief explanation"}}"#)
}

fn parse_schema_proposal(response: &str, _shard_key: &str) -> Result<SchemaProposal, String> {
    // Try to find JSON in the response
    let json_str = response.trim();

    // Handle case where response might have markdown code blocks
    let json_str = json_str
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    // Try to parse as JSON
    match serde_json::from_str::<SchemaProposal>(json_str) {
        Ok(proposal) => Ok(proposal),
        Err(e) => {
            Err(format!(
                "Failed to parse LLM response as schema. Error: {}. Response: {}",
                e,
                &response[..response.len().min(500)]
            ))
        }
    }
}

/// Result of subscription tag validation
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagValidationResult {
    pub valid: bool,
    pub exists: bool,
    pub existing_plugin_name: Option<String>,
}

/// Validate a subscription tag for uniqueness
#[tauri::command]
pub async fn validate_subscription_tag(
    state: tauri::State<'_, crate::SentinelState>,
    tag: String,
    current_parser_id: Option<String>,
) -> Result<TagValidationResult, String> {
    // Basic format validation - alphanumeric, underscore, hyphen, dot only
    let is_valid_format = !tag.is_empty() && tag.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
    });

    if !is_valid_format {
        return Ok(TagValidationResult {
            valid: false,
            exists: false,
            existing_plugin_name: None,
        });
    }

    // Ignore current_parser_id for now (reserved for future use)
    let _ = current_parser_id;

    // Check database for existing plugins with this tag
    let pool_guard = state.db_pool.lock().await;
    let pool = match pool_guard.as_ref() {
        Some(p) => p,
        None => {
            // No database connection - treat as valid (can't check)
            return Ok(TagValidationResult {
                valid: true,
                exists: false,
                existing_plugin_name: None,
            });
        }
    };

    // Tags are stored as-is, no prefix
    let subscription_tag = tag.clone();

    // Look for existing plugin config with this subscription tag
    let result: Option<(String,)> = sqlx::query_as(
        "SELECT plugin_name FROM cf_plugin_config WHERE subscription_tags = ? LIMIT 1"
    )
    .bind(&subscription_tag)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match result {
        Some((plugin_name,)) => {
            // Check if this is the same parser (allow re-deploying with same tag)
            // For now, we just report it exists - the UI can decide what to do
            Ok(TagValidationResult {
                valid: true,
                exists: true,
                existing_plugin_name: Some(plugin_name),
            })
        }
        None => {
            Ok(TagValidationResult {
                valid: true,
                exists: false,
                existing_plugin_name: None,
            })
        }
    }
}

/// Publish a parser: register in database (source of truth), then write cache file
///
/// Architecture:
/// - Database (cf_plugin_manifest) is the source of truth
/// - File on disk is an execution cache, regenerated on-demand
/// - All DB operations must succeed or the whole publish fails
#[tauri::command]
pub async fn publish_parser(
    state: tauri::State<'_, crate::SentinelState>,
    parser_key: String,
    source_code: String,
    schema: Vec<SchemaColumn>,
    sink_type: String,
    output_path: String,
    output_mode: Option<String>,
    topic_uris_json: Option<String>,
) -> Result<ParserPublishReceipt, String> {
    use sha2::{Sha256, Digest};

    // 1. Calculate source hash
    let mut hasher = Sha256::new();
    hasher.update(source_code.as_bytes());
    let source_hash = format!("{:x}", hasher.finalize());

    // 2. Get database connection
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let plugin_name = parser_key.clone();
    let version = "1.0.0";

    // 3. Insert into cf_plugin_manifest (SOURCE OF TRUTH)
    // This stores the actual code - file is just a cache
    let manifest_result = sqlx::query(
        "INSERT OR REPLACE INTO cf_plugin_manifest (plugin_name, version, source_code, source_hash, status, deployed_at) VALUES (?, ?, ?, ?, 'ACTIVE', datetime('now'))"
    )
    .bind(&plugin_name)
    .bind(version)
    .bind(&source_code)
    .bind(&source_hash)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to register plugin in manifest: {}", e))?;

    let manifest_id = manifest_result.last_insert_rowid();

    // 4. Insert into cf_plugin_config with subscription_tags
    // Tags are stored as-is, no prefix. Matching is exact.
    let subscription_tag = parser_key.clone();
    let config_result = sqlx::query(
        "INSERT OR REPLACE INTO cf_plugin_config (plugin_name, subscription_tags, enabled) VALUES (?, ?, 1)"
    )
    .bind(&plugin_name)
    .bind(&subscription_tag)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to register plugin config: {}", e))?;

    let config_id = config_result.last_insert_rowid();

    // 5. Insert into cf_topic_config for output routing
    let is_multi = output_mode.as_deref() == Some("multi");

    let topic_config_id = if is_multi && topic_uris_json.is_some() {
        // Multi-output: Create one cf_topic_config per topic
        // Frontend sends structured format: { "topic": { "type": "sqlite", "uri": "sqlite://...", "config": {} } }
        #[derive(serde::Deserialize)]
        struct TopicSinkInfo {
            #[serde(rename = "type")]
            sink_type: String,
            uri: String,
            #[allow(dead_code)]
            config: Option<serde_json::Value>,
        }

        // Try new structured format first, fall back to old URI-only format
        let topic_configs: Vec<(String, String, String)> = topic_uris_json
            .as_ref()
            .and_then(|json| {
                // Try structured format first
                if let Ok(structured) = serde_json::from_str::<std::collections::HashMap<String, TopicSinkInfo>>(json) {
                    Some(structured.into_iter().map(|(topic, info)| (topic, info.uri, info.sink_type)).collect())
                } else if let Ok(simple) = serde_json::from_str::<std::collections::HashMap<String, String>>(json) {
                    // Fall back to old simple URI format
                    Some(simple.into_iter().map(|(topic, uri)| {
                        let sink_type = if uri.starts_with("sqlite://") { "sqlite" }
                            else if uri.starts_with("csv://") { "csv" }
                            else { "parquet" };
                        (topic, uri, sink_type.to_string())
                    }).collect())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let mut last_id: i64 = 0;
        for (topic_name, uri, topic_sink_type) in topic_configs {
            let topic_result = sqlx::query(
                "INSERT OR REPLACE INTO cf_topic_config (plugin_name, topic_name, uri, sink_type, enabled) VALUES (?, ?, ?, ?, 1)"
            )
            .bind(&plugin_name)
            .bind(&topic_name)
            .bind(&uri)
            .bind(&topic_sink_type)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to register topic config for {}: {}", topic_name, e))?;

            last_id = topic_result.last_insert_rowid();
        }
        last_id
    } else {
        // Single-output: Create one cf_topic_config
        let output_uri = match sink_type.as_str() {
            "parquet" => format!("parquet://{}", output_path),
            "csv" => format!("csv://{}", output_path),
            "sqlite" => format!("sqlite://{}", output_path),
            _ => format!("file://{}", output_path),
        };

        let topic_name = format!("{}_output", parser_key);
        let topic_result = sqlx::query(
            "INSERT OR REPLACE INTO cf_topic_config (plugin_name, topic_name, uri, sink_type, enabled) VALUES (?, ?, ?, ?, 1)"
        )
        .bind(&plugin_name)
        .bind(&topic_name)
        .bind(&output_uri)
        .bind(&sink_type)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to register topic config: {}", e))?;

        topic_result.last_insert_rowid()
    };

    // 6. Write execution cache file (derived from DB, can be regenerated)
    let home_dir = dirs::home_dir()
        .ok_or_else(|| "Could not determine home directory".to_string())?;
    let parsers_dir = home_dir.join(".casparian_flow").join("parsers");
    std::fs::create_dir_all(&parsers_dir)
        .map_err(|e| format!("Failed to create parsers directory: {}", e))?;

    let parser_filename = format!("{}.py", sanitize_filename(&parser_key));
    let parser_path = parsers_dir.join(&parser_filename);
    std::fs::write(&parser_path, &source_code)
        .map_err(|e| format!("Failed to write parser cache file: {}", e))?;

    // 7. Save schema as JSON sidecar file
    let schema_path = parsers_dir.join(format!("{}.schema.json", sanitize_filename(&parser_key)));
    let schema_json = serde_json::to_string_pretty(&schema)
        .map_err(|e| format!("Failed to serialize schema: {}", e))?;
    std::fs::write(&schema_path, &schema_json)
        .map_err(|e| format!("Failed to write schema file: {}", e))?;

    Ok(ParserPublishReceipt {
        success: true,
        plugin_name,
        parser_file_path: parser_path.display().to_string(),
        manifest_id: Some(manifest_id),
        config_id: Some(config_id),
        topic_config_id: Some(topic_config_id),
        message: format!(
            "Parser published successfully. File: {}, Sink: {} → {}",
            parser_path.display(),
            sink_type,
            output_path
        ),
    })
}

/// Sanitize a string for use as a filename
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

// ============================================================================
// Splitter Session Persistence (formerly Shredder)
// ============================================================================

/// Splitter session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SplitterSessionStatus {
    New,
    Analyzing,
    Analyzed,
    Configured,
    Shredding,
    Shredded,
    Complete,
}

impl SplitterSessionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Analyzing => "analyzing",
            Self::Analyzed => "analyzed",
            Self::Configured => "configured",
            Self::Shredding => "shredding",
            Self::Shredded => "shredded",
            Self::Complete => "complete",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "analyzing" => Self::Analyzing,
            "analyzed" => Self::Analyzed,
            "configured" => Self::Configured,
            "shredding" => Self::Shredding,
            "shredded" => Self::Shredded,
            "complete" => Self::Complete,
            _ => Self::New,
        }
    }
}

/// A splitter session - tracks the entire file splitting workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitterSession {
    pub id: String,
    pub name: String,
    pub source_file_path: String,
    pub output_dir: Option<String>,
    pub col_index: Option<i32>,
    pub delimiter: Option<String>,
    pub has_header: Option<bool>,
    pub analysis_messages_json: Option<String>,
    pub analysis_result_json: Option<String>,
    pub full_analysis_json: Option<String>,
    pub shred_result_json: Option<String>,
    pub status: SplitterSessionStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Summary of a splitter session for list view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitterSessionSummary {
    pub id: String,
    pub name: String,
    pub source_file_path: String,
    pub status: SplitterSessionStatus,
    pub shard_count: i32,
    pub parsers_ready: i32,
    pub updated_at: i64,
}

/// Parser draft - tracks parser development per shard
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitterParserDraft {
    pub id: String,
    pub session_id: String,
    pub shard_key: String,
    pub shard_path: String,
    pub current_code: Option<String>,
    pub validation_status: String, // "pending" | "valid" | "invalid"
    pub validation_error: Option<String>,
    pub validation_output: Option<String>,
    pub messages_json: Option<String>,
    pub schema_json: Option<String>,
    pub sink_type: Option<String>,
    pub output_path: Option<String>,
    pub phase: String, // "refining" | "configuring" | "published"
    pub published_plugin_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Create a new splitter session
#[tauri::command]
pub fn splitter_create_session(
    state: State<'_, ScoutState>,
    source_file_path: String,
) -> Result<SplitterSession, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    // Generate name from filename
    let name = std::path::Path::new(&source_file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled")
        .to_string();

    conn.execute(
        r#"INSERT INTO splitter_sessions
           (id, name, source_file_path, status, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?5)"#,
        rusqlite::params![id, name, source_file_path, "new", now],
    ).map_err(|e| format!("Failed to create session: {}", e))?;

    Ok(SplitterSession {
        id,
        name,
        source_file_path,
        output_dir: None,
        col_index: None,
        delimiter: Some(",".to_string()),
        has_header: Some(true),
        analysis_messages_json: None,
        analysis_result_json: None,
        full_analysis_json: None,
        shred_result_json: None,
        status: SplitterSessionStatus::New,
        created_at: now,
        updated_at: now,
    })
}

/// Get a splitter session by ID
#[tauri::command]
pub fn splitter_get_session(
    state: State<'_, ScoutState>,
    session_id: String,
) -> Result<Option<SplitterSession>, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let result = conn.query_row(
        r#"SELECT id, name, source_file_path, output_dir, col_index, delimiter, has_header,
                  analysis_messages_json, analysis_result_json, full_analysis_json, shred_result_json,
                  status, created_at, updated_at
           FROM splitter_sessions WHERE id = ?1"#,
        rusqlite::params![session_id],
        |row: &rusqlite::Row| {
            Ok(SplitterSession {
                id: row.get(0)?,
                name: row.get(1)?,
                source_file_path: row.get(2)?,
                output_dir: row.get(3)?,
                col_index: row.get(4)?,
                delimiter: row.get(5)?,
                has_header: row.get::<_, Option<i32>>(6)?.map(|v| v != 0),
                analysis_messages_json: row.get(7)?,
                analysis_result_json: row.get(8)?,
                full_analysis_json: row.get(9)?,
                shred_result_json: row.get(10)?,
                status: SplitterSessionStatus::from_str(row.get::<_, String>(11)?.as_str()),
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        },
    );

    match result {
        Ok(session) => Ok(Some(session)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get session: {}", e)),
    }
}

/// Update a splitter session
#[tauri::command]
pub fn splitter_update_session(
    state: State<'_, ScoutState>,
    session: SplitterSession,
) -> Result<(), String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let now = chrono::Utc::now().timestamp_millis();

    conn.execute(
        r#"UPDATE splitter_sessions SET
           name = ?2, source_file_path = ?3, output_dir = ?4, col_index = ?5,
           delimiter = ?6, has_header = ?7, analysis_messages_json = ?8,
           analysis_result_json = ?9, full_analysis_json = ?10, shred_result_json = ?11,
           status = ?12, updated_at = ?13
           WHERE id = ?1"#,
        rusqlite::params![
            session.id,
            session.name,
            session.source_file_path,
            session.output_dir,
            session.col_index,
            session.delimiter,
            session.has_header.map(|v| if v { 1 } else { 0 }),
            session.analysis_messages_json,
            session.analysis_result_json,
            session.full_analysis_json,
            session.shred_result_json,
            session.status.as_str(),
            now,
        ],
    ).map_err(|e| format!("Failed to update session: {}", e))?;

    Ok(())
}

/// List recent splitter sessions
#[tauri::command]
pub fn splitter_list_sessions(
    state: State<'_, ScoutState>,
    limit: Option<i32>,
) -> Result<Vec<SplitterSessionSummary>, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let limit = limit.unwrap_or(20);

    let mut stmt = conn.prepare(
        r#"SELECT s.id, s.name, s.source_file_path, s.status, s.updated_at,
                  (SELECT COUNT(*) FROM splitter_parser_drafts WHERE session_id = s.id) as shard_count,
                  (SELECT COUNT(*) FROM splitter_parser_drafts WHERE session_id = s.id AND validation_status = 'valid') as parsers_ready
           FROM splitter_sessions s
           ORDER BY s.updated_at DESC
           LIMIT ?1"#,
    ).map_err(|e| format!("Failed to prepare query: {}", e))?;

    let sessions = stmt.query_map(rusqlite::params![limit], |row: &rusqlite::Row| {
        Ok(SplitterSessionSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            source_file_path: row.get(2)?,
            status: SplitterSessionStatus::from_str(row.get::<_, String>(3)?.as_str()),
            updated_at: row.get(4)?,
            shard_count: row.get(5)?,
            parsers_ready: row.get(6)?,
        })
    }).map_err(|e| format!("Failed to query sessions: {}", e))?;

    sessions.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect sessions: {}", e))
}

/// Delete a splitter session and its parser drafts
#[tauri::command]
pub fn splitter_delete_session(
    state: State<'_, ScoutState>,
    session_id: String,
) -> Result<(), String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    // Parser drafts will be deleted by CASCADE
    conn.execute(
        "DELETE FROM splitter_sessions WHERE id = ?1",
        rusqlite::params![session_id],
    ).map_err(|e| format!("Failed to delete session: {}", e))?;

    Ok(())
}

/// Save or update a parser draft
#[tauri::command]
pub fn splitter_save_parser_draft(
    state: State<'_, ScoutState>,
    draft: SplitterParserDraft,
) -> Result<(), String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let now = chrono::Utc::now().timestamp_millis();

    conn.execute(
        r#"INSERT INTO splitter_parser_drafts
           (id, session_id, shard_key, shard_path, current_code, validation_status,
            validation_error, validation_output, messages_json, schema_json,
            sink_type, output_path, phase, published_plugin_id, created_at, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
           ON CONFLICT(session_id, shard_key) DO UPDATE SET
            current_code = excluded.current_code,
            validation_status = excluded.validation_status,
            validation_error = excluded.validation_error,
            validation_output = excluded.validation_output,
            messages_json = excluded.messages_json,
            schema_json = excluded.schema_json,
            sink_type = excluded.sink_type,
            output_path = excluded.output_path,
            phase = excluded.phase,
            published_plugin_id = excluded.published_plugin_id,
            updated_at = ?15"#,
        rusqlite::params![
            draft.id,
            draft.session_id,
            draft.shard_key,
            draft.shard_path,
            draft.current_code,
            draft.validation_status,
            draft.validation_error,
            draft.validation_output,
            draft.messages_json,
            draft.schema_json,
            draft.sink_type,
            draft.output_path,
            draft.phase,
            draft.published_plugin_id,
            now,
        ],
    ).map_err(|e| format!("Failed to save parser draft: {}", e))?;

    Ok(())
}

/// Get a parser draft by session and shard key
#[tauri::command]
pub fn splitter_get_parser_draft(
    state: State<'_, ScoutState>,
    session_id: String,
    shard_key: String,
) -> Result<Option<SplitterParserDraft>, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let result = conn.query_row(
        r#"SELECT id, session_id, shard_key, shard_path, current_code, validation_status,
                  validation_error, validation_output, messages_json, schema_json,
                  sink_type, output_path, phase, published_plugin_id, created_at, updated_at
           FROM splitter_parser_drafts
           WHERE session_id = ?1 AND shard_key = ?2"#,
        rusqlite::params![session_id, shard_key],
        |row: &rusqlite::Row| {
            Ok(SplitterParserDraft {
                id: row.get(0)?,
                session_id: row.get(1)?,
                shard_key: row.get(2)?,
                shard_path: row.get(3)?,
                current_code: row.get(4)?,
                validation_status: row.get(5)?,
                validation_error: row.get(6)?,
                validation_output: row.get(7)?,
                messages_json: row.get(8)?,
                schema_json: row.get(9)?,
                sink_type: row.get(10)?,
                output_path: row.get(11)?,
                phase: row.get(12)?,
                published_plugin_id: row.get(13)?,
                created_at: row.get(14)?,
                updated_at: row.get(15)?,
            })
        },
    );

    match result {
        Ok(draft) => Ok(Some(draft)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get parser draft: {}", e)),
    }
}

/// List all parser drafts for a session
#[tauri::command]
pub fn splitter_list_parser_drafts(
    state: State<'_, ScoutState>,
    session_id: String,
) -> Result<Vec<SplitterParserDraft>, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let mut stmt = conn.prepare(
        r#"SELECT id, session_id, shard_key, shard_path, current_code, validation_status,
                  validation_error, validation_output, messages_json, schema_json,
                  sink_type, output_path, phase, published_plugin_id, created_at, updated_at
           FROM splitter_parser_drafts
           WHERE session_id = ?1
           ORDER BY shard_key"#,
    ).map_err(|e| format!("Failed to prepare query: {}", e))?;

    let drafts = stmt.query_map(rusqlite::params![session_id], |row: &rusqlite::Row| {
        Ok(SplitterParserDraft {
            id: row.get(0)?,
            session_id: row.get(1)?,
            shard_key: row.get(2)?,
            shard_path: row.get(3)?,
            current_code: row.get(4)?,
            validation_status: row.get(5)?,
            validation_error: row.get(6)?,
            validation_output: row.get(7)?,
            messages_json: row.get(8)?,
            schema_json: row.get(9)?,
            sink_type: row.get(10)?,
            output_path: row.get(11)?,
            phase: row.get(12)?,
            published_plugin_id: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
        })
    }).map_err(|e| format!("Failed to query parser drafts: {}", e))?;

    drafts.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect parser drafts: {}", e))
}

// ============================================================================
// Parser Lab (v6) - Parser-centric Development Workspace
// ============================================================================

/// A test file associated with a parser
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserLabTestFile {
    pub id: String,
    pub parser_id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: Option<i64>,
    pub created_at: i64,
}

/// A parser (top-level entity in Parser Lab v6)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserLabParser {
    pub id: String,
    pub name: String,
    pub file_pattern: String,           // What files this parser applies to
    pub pattern_type: String,            // "all" | "key_column" | "glob"
    pub source_code: Option<String>,
    pub validation_status: String,       // "pending" | "valid" | "invalid"
    pub validation_error: Option<String>,
    pub validation_output: Option<String>,
    pub last_validated_at: Option<i64>,
    pub messages_json: Option<String>,
    pub schema_json: Option<String>,
    pub sink_type: String,               // "parquet" | "csv" | "sqlite"
    pub sink_config_json: Option<String>,
    pub published_at: Option<i64>,
    pub published_plugin_id: Option<i64>,
    pub is_sample: bool,                 // Is this a bundled sample?
    pub output_mode: String,             // "single" | "multi" (for dict returns)
    pub detected_topics_json: Option<String>,  // ["header", "line_items", "totals"]
    pub created_at: i64,
    pub updated_at: i64,
}

/// Summary of a parser for list view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserLabParserSummary {
    pub id: String,
    pub name: String,
    pub file_pattern: String,
    pub pattern_type: String,
    pub validation_status: String,
    pub is_sample: bool,
    pub test_file_count: i32,
    pub updated_at: i64,
}

/// Parquet sink configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParquetSinkConfig {
    pub output_dir: String,
    pub compression: String,  // "snappy" | "gzip" | "lz4" | "none"
    pub partition_by: Option<String>,
    pub row_group_size: Option<i64>,
}

/// SQLite sink configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqliteSinkConfig {
    pub database_path: String,
    pub table_name: String,
    pub create_if_not_exists: bool,
    pub write_mode: String,  // "append" | "replace" | "fail"
}

/// CSV sink configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvSinkConfig {
    pub output_dir: String,
    pub delimiter: String,  // "," | "\t" | "|"
    pub include_header: bool,
    pub quote_all: bool,
}

// ============================================================================
// Parser Lab Commands (v6 - Parser-centric)
// ============================================================================

/// Create a new parser
#[tauri::command]
pub fn parser_lab_create_parser(
    state: State<'_, ScoutState>,
    name: String,
    file_pattern: Option<String>,
) -> Result<ParserLabParser, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let pattern = file_pattern.unwrap_or_default();

    conn.execute(
        r#"INSERT INTO parser_lab_parsers
           (id, name, file_pattern, pattern_type, validation_status, sink_type, is_sample, created_at, updated_at)
           VALUES (?1, ?2, ?3, 'all', 'pending', 'parquet', 0, ?4, ?4)"#,
        rusqlite::params![id, name, pattern, now],
    ).map_err(|e| format!("Failed to create parser: {}", e))?;

    Ok(ParserLabParser {
        id,
        name,
        file_pattern: pattern,
        pattern_type: "all".to_string(),
        source_code: None,
        validation_status: "pending".to_string(),
        validation_error: None,
        validation_output: None,
        last_validated_at: None,
        messages_json: None,
        schema_json: None,
        sink_type: "parquet".to_string(),
        sink_config_json: None,
        published_at: None,
        published_plugin_id: None,
        is_sample: false,
        output_mode: "single".to_string(),
        detected_topics_json: None,
        created_at: now,
        updated_at: now,
    })
}

/// Get a parser by ID
#[tauri::command]
pub fn parser_lab_get_parser(
    state: State<'_, ScoutState>,
    parser_id: String,
) -> Result<Option<ParserLabParser>, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let result = conn.query_row(
        r#"SELECT id, name, file_pattern, pattern_type, source_code,
                  validation_status, validation_error, validation_output, last_validated_at,
                  messages_json, schema_json, sink_type, sink_config_json,
                  published_at, published_plugin_id, is_sample, output_mode, detected_topics_json,
                  created_at, updated_at
           FROM parser_lab_parsers WHERE id = ?1"#,
        rusqlite::params![parser_id],
        |row: &rusqlite::Row| {
            Ok(ParserLabParser {
                id: row.get(0)?,
                name: row.get(1)?,
                file_pattern: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                pattern_type: row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "all".to_string()),
                source_code: row.get(4)?,
                validation_status: row.get::<_, Option<String>>(5)?.unwrap_or_else(|| "pending".to_string()),
                validation_error: row.get(6)?,
                validation_output: row.get(7)?,
                last_validated_at: row.get(8)?,
                messages_json: row.get(9)?,
                schema_json: row.get(10)?,
                sink_type: row.get::<_, Option<String>>(11)?.unwrap_or_else(|| "parquet".to_string()),
                sink_config_json: row.get(12)?,
                published_at: row.get(13)?,
                published_plugin_id: row.get(14)?,
                is_sample: row.get::<_, Option<i32>>(15)?.unwrap_or(0) != 0,
                output_mode: row.get::<_, Option<String>>(16)?.unwrap_or_else(|| "single".to_string()),
                detected_topics_json: row.get(17)?,
                created_at: row.get(18)?,
                updated_at: row.get(19)?,
            })
        },
    );

    match result {
        Ok(parser) => Ok(Some(parser)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get parser: {}", e)),
    }
}

/// Update a parser
#[tauri::command]
pub fn parser_lab_update_parser(
    state: State<'_, ScoutState>,
    parser: ParserLabParser,
) -> Result<(), String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let now = chrono::Utc::now().timestamp_millis();

    conn.execute(
        r#"UPDATE parser_lab_parsers SET
           name = ?2, file_pattern = ?3, pattern_type = ?4, source_code = ?5,
           validation_status = ?6, validation_error = ?7, validation_output = ?8,
           last_validated_at = ?9, messages_json = ?10, schema_json = ?11,
           sink_type = ?12, sink_config_json = ?13, published_at = ?14,
           published_plugin_id = ?15, output_mode = ?16, detected_topics_json = ?17,
           updated_at = ?18
           WHERE id = ?1"#,
        rusqlite::params![
            parser.id,
            parser.name,
            parser.file_pattern,
            parser.pattern_type,
            parser.source_code,
            parser.validation_status,
            parser.validation_error,
            parser.validation_output,
            parser.last_validated_at,
            parser.messages_json,
            parser.schema_json,
            parser.sink_type,
            parser.sink_config_json,
            parser.published_at,
            parser.published_plugin_id,
            parser.output_mode,
            parser.detected_topics_json,
            now,
        ],
    ).map_err(|e| format!("Failed to update parser: {}", e))?;

    Ok(())
}

/// List all parsers with summaries
#[tauri::command]
pub fn parser_lab_list_parsers(
    state: State<'_, ScoutState>,
    limit: Option<i32>,
) -> Result<Vec<ParserLabParserSummary>, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let limit = limit.unwrap_or(50);

    let mut stmt = conn.prepare(
        r#"SELECT p.id, p.name, p.file_pattern, p.pattern_type, p.validation_status,
                  p.is_sample, p.updated_at,
                  (SELECT COUNT(*) FROM parser_lab_test_files WHERE parser_id = p.id) as test_file_count
           FROM parser_lab_parsers p
           ORDER BY p.updated_at DESC
           LIMIT ?1"#,
    ).map_err(|e| format!("Failed to prepare query: {}", e))?;

    let parsers = stmt.query_map(rusqlite::params![limit], |row: &rusqlite::Row| {
        Ok(ParserLabParserSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            file_pattern: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            pattern_type: row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "all".to_string()),
            validation_status: row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "pending".to_string()),
            is_sample: row.get::<_, Option<i32>>(5)?.unwrap_or(0) != 0,
            updated_at: row.get(6)?,
            test_file_count: row.get(7)?,
        })
    }).map_err(|e| format!("Failed to query parsers: {}", e))?;

    parsers.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect parsers: {}", e))
}

/// Delete a parser and all associated test files
#[tauri::command]
pub fn parser_lab_delete_parser(
    state: State<'_, ScoutState>,
    parser_id: String,
) -> Result<(), String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    // Test files will be deleted by CASCADE
    conn.execute(
        "DELETE FROM parser_lab_parsers WHERE id = ?1",
        rusqlite::params![parser_id],
    ).map_err(|e| format!("Failed to delete parser: {}", e))?;

    Ok(())
}

// ============================================================================
// Parser Lab Test File Commands
// ============================================================================

/// Add a test file to a parser
#[tauri::command]
pub fn parser_lab_add_test_file(
    state: State<'_, ScoutState>,
    parser_id: String,
    file_path: String,
) -> Result<ParserLabTestFile, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    // Extract filename
    let file_name = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Get file size
    let file_size = std::fs::metadata(&file_path)
        .map(|m| m.len() as i64)
        .ok();

    conn.execute(
        r#"INSERT INTO parser_lab_test_files (id, parser_id, file_path, file_name, file_size, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
        rusqlite::params![id, parser_id, file_path, file_name, file_size, now],
    ).map_err(|e| format!("Failed to add test file: {}", e))?;

    // Update parser timestamp
    conn.execute(
        "UPDATE parser_lab_parsers SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now, parser_id],
    ).map_err(|e| format!("Failed to update parser: {}", e))?;

    Ok(ParserLabTestFile {
        id,
        parser_id,
        file_path,
        file_name,
        file_size,
        created_at: now,
    })
}

/// Remove a test file
#[tauri::command]
pub fn parser_lab_remove_test_file(
    state: State<'_, ScoutState>,
    test_file_id: String,
) -> Result<(), String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    conn.execute(
        "DELETE FROM parser_lab_test_files WHERE id = ?1",
        rusqlite::params![test_file_id],
    ).map_err(|e| format!("Failed to remove test file: {}", e))?;

    Ok(())
}

/// List test files for a parser
#[tauri::command]
pub fn parser_lab_list_test_files(
    state: State<'_, ScoutState>,
    parser_id: String,
) -> Result<Vec<ParserLabTestFile>, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let mut stmt = conn.prepare(
        r#"SELECT id, parser_id, file_path, file_name, file_size, created_at
           FROM parser_lab_test_files
           WHERE parser_id = ?1
           ORDER BY created_at DESC"#,
    ).map_err(|e| format!("Failed to prepare query: {}", e))?;

    let files = stmt.query_map(rusqlite::params![parser_id], |row: &rusqlite::Row| {
        Ok(ParserLabTestFile {
            id: row.get(0)?,
            parser_id: row.get(1)?,
            file_path: row.get(2)?,
            file_name: row.get(3)?,
            file_size: row.get(4)?,
            created_at: row.get(5)?,
        })
    }).map_err(|e| format!("Failed to query test files: {}", e))?;

    files.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect test files: {}", e))
}

// ============================================================================
// Parser Lab Validation
// ============================================================================

/// Validate a parser against a test file
#[tauri::command]
pub async fn parser_lab_validate_parser(
    state: State<'_, ScoutState>,
    parser_id: String,
    test_file_id: String,
) -> Result<ParserLabParser, String> {
    // Get parser and test file info before any async operations
    let (source_code, file_path) = {
        let parser = parser_lab_get_parser(state.clone(), parser_id.clone())?
            .ok_or_else(|| "Parser not found".to_string())?;

        let source_code = parser.source_code.ok_or_else(|| "Parser has no source code".to_string())?;

        let db = state.get_db()?;
        let conn = db.raw_connection();

        let file_path: String = conn.query_row(
            "SELECT file_path FROM parser_lab_test_files WHERE id = ?1",
            rusqlite::params![test_file_id],
            |row| row.get(0),
        ).map_err(|e| format!("Failed to get test file: {}", e))?;

        (source_code, file_path)
    };

    // Validate using the existing validate_parser function (async)
    let result = validate_parser(file_path, source_code).await?;

    // Update parser with validation result
    {
        let now = chrono::Utc::now().timestamp_millis();
        let (status, error, output) = if result.validation_error.is_some() {
            ("invalid".to_string(), result.validation_error, None)
        } else {
            ("valid".to_string(), None, result.sample_output)
        };

        // Convert detected topics to JSON
        let detected_topics_json = result.detected_topics.map(|topics| {
            serde_json::to_string(&topics).unwrap_or_default()
        });

        let db = state.get_db()?;
        let conn = db.raw_connection();

        conn.execute(
            r#"UPDATE parser_lab_parsers SET
               validation_status = ?2, validation_error = ?3, validation_output = ?4,
               last_validated_at = ?5, output_mode = ?6, detected_topics_json = ?7,
               updated_at = ?5
               WHERE id = ?1"#,
            rusqlite::params![parser_id, status, error, output, now, result.output_mode, detected_topics_json],
        ).map_err(|e| format!("Failed to update parser: {}", e))?;
    }

    // Return updated parser
    parser_lab_get_parser(state, parser_id)?
        .ok_or_else(|| "Parser not found after update".to_string())
}

/// Import an existing plugin file as a new parser
#[tauri::command]
pub fn parser_lab_import_plugin(
    state: State<'_, ScoutState>,
    plugin_path: String,
) -> Result<ParserLabParser, String> {
    // Read the plugin file
    let source_code = std::fs::read_to_string(&plugin_path)
        .map_err(|e| format!("Failed to read plugin file: {}", e))?;

    // Extract name from filename
    let name = std::path::Path::new(&plugin_path)
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("imported_plugin")
        .to_string();

    // Create the parser with the imported code
    let db = state.get_db()?;
    let conn = db.raw_connection();

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    conn.execute(
        r#"INSERT INTO parser_lab_parsers
           (id, name, file_pattern, pattern_type, source_code, validation_status, sink_type, is_sample, created_at, updated_at)
           VALUES (?1, ?2, '', 'all', ?3, 'pending', 'parquet', 0, ?4, ?4)"#,
        rusqlite::params![id, name, source_code, now],
    ).map_err(|e| format!("Failed to create parser: {}", e))?;

    Ok(ParserLabParser {
        id,
        name,
        file_pattern: String::new(),
        pattern_type: "all".to_string(),
        source_code: Some(source_code),
        validation_status: "pending".to_string(),
        validation_error: None,
        validation_output: None,
        last_validated_at: None,
        messages_json: None,
        schema_json: None,
        sink_type: "parquet".to_string(),
        sink_config_json: None,
        published_at: None,
        published_plugin_id: None,
        is_sample: false,
        output_mode: "single".to_string(),
        detected_topics_json: None,
        created_at: now,
        updated_at: now,
    })
}

/// Sample parser code for the bundled sample
const SAMPLE_PARSER_CODE: &str = r#"import polars as pl

def parse(input_path: str) -> pl.DataFrame:
    """
    Parse transaction records into a clean DataFrame.

    This sample parser demonstrates:
    - Reading CSV files with polars
    - Type conversions (date, float)
    - Basic data cleaning
    """
    df = pl.read_csv(input_path)

    # Convert types
    df = df.with_columns([
        pl.col("date").str.strptime(pl.Date, "%Y-%m-%d"),
        pl.col("amount").cast(pl.Float64),
    ])

    return df
"#;

/// Load or create the sample parser
#[tauri::command]
pub fn parser_lab_load_sample(
    state: State<'_, ScoutState>,
    app: tauri::AppHandle,
) -> Result<ParserLabParser, String> {
    let db = state.get_db()?;
    let conn = db.raw_connection();

    // Check if sample parser already exists
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM parser_lab_parsers WHERE is_sample = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        // Return existing sample parser
        return parser_lab_get_parser_by_id(&conn, &id);
    }

    // Create new sample parser
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    conn.execute(
        r#"INSERT INTO parser_lab_parsers
           (id, name, file_pattern, pattern_type, source_code, validation_status, sink_type, is_sample, created_at, updated_at)
           VALUES (?1, 'Sample Parser', '', 'all', ?2, 'pending', 'parquet', 1, ?3, ?3)"#,
        rusqlite::params![id, SAMPLE_PARSER_CODE, now],
    ).map_err(|e| format!("Failed to create sample parser: {}", e))?;

    // Copy sample CSV to a known location
    let home = dirs::home_dir()
        .ok_or_else(|| "Could not find home directory".to_string())?;
    let samples_dir = home.join(".casparian_flow").join("samples");
    std::fs::create_dir_all(&samples_dir)
        .map_err(|e| format!("Failed to create samples directory: {}", e))?;

    let sample_csv_dest = samples_dir.join("transactions.csv");

    // Get the bundled sample CSV from resources
    let resource_path = app.path()
        .resource_dir()
        .map_err(|e| format!("Failed to get resource dir: {}", e))?
        .join("resources")
        .join("sample_transactions.csv");

    if resource_path.exists() {
        std::fs::copy(&resource_path, &sample_csv_dest)
            .map_err(|e| format!("Failed to copy sample CSV: {}", e))?;
    } else {
        // Fallback: create sample data directly
        let sample_data = r#"id,type,date,amount,category,description
1,SALE,2024-01-15,150.00,electronics,Wireless headphones
2,REFUND,2024-01-16,-25.00,electronics,Defective charger return
3,SALE,2024-01-16,89.99,books,Programming textbook
4,SALE,2024-01-17,34.50,office,Notebook set
5,SALE,2024-01-18,299.00,electronics,Bluetooth speaker
"#;
        std::fs::write(&sample_csv_dest, sample_data)
            .map_err(|e| format!("Failed to write sample CSV: {}", e))?;
    }

    // Add test file
    let test_file_id = uuid::Uuid::new_v4().to_string();
    let file_size = std::fs::metadata(&sample_csv_dest)
        .map(|m| m.len() as i64)
        .ok();

    conn.execute(
        r#"INSERT INTO parser_lab_test_files
           (id, parser_id, file_path, file_name, file_size, created_at)
           VALUES (?1, ?2, ?3, 'transactions.csv', ?4, ?5)"#,
        rusqlite::params![
            test_file_id,
            id,
            sample_csv_dest.to_string_lossy().to_string(),
            file_size,
            now
        ],
    ).map_err(|e| format!("Failed to add sample test file: {}", e))?;

    parser_lab_get_parser_by_id(&conn, &id)
}

fn parser_lab_get_parser_by_id(conn: &rusqlite::Connection, id: &str) -> Result<ParserLabParser, String> {
    conn.query_row(
        r#"SELECT id, name, file_pattern, pattern_type, source_code, validation_status,
           validation_error, validation_output, last_validated_at, messages_json,
           schema_json, sink_type, sink_config_json, published_at, published_plugin_id,
           is_sample, output_mode, detected_topics_json, created_at, updated_at
           FROM parser_lab_parsers WHERE id = ?1"#,
        [id],
        |row| {
            Ok(ParserLabParser {
                id: row.get(0)?,
                name: row.get(1)?,
                file_pattern: row.get(2)?,
                pattern_type: row.get(3)?,
                source_code: row.get(4)?,
                validation_status: row.get(5)?,
                validation_error: row.get(6)?,
                validation_output: row.get(7)?,
                last_validated_at: row.get(8)?,
                messages_json: row.get(9)?,
                schema_json: row.get(10)?,
                sink_type: row.get(11)?,
                sink_config_json: row.get(12)?,
                published_at: row.get(13)?,
                published_plugin_id: row.get(14)?,
                is_sample: row.get::<_, i32>(15)? != 0,
                output_mode: row.get::<_, Option<String>>(16)?.unwrap_or_else(|| "single".to_string()),
                detected_topics_json: row.get(17)?,
                created_at: row.get(18)?,
                updated_at: row.get(19)?,
            })
        },
    ).map_err(|e| format!("Failed to get parser: {}", e))
}

/// List existing plugins that can be imported
/// Returns plugin names (not file paths) for use in manual plugin selection
#[tauri::command]
pub fn parser_lab_list_importable_plugins() -> Result<Vec<String>, String> {
    // Look in ~/.casparian_flow/parsers/
    let home = dirs::home_dir()
        .ok_or_else(|| "Could not find home directory".to_string())?;

    let parsers_dir = home.join(".casparian_flow").join("parsers");

    if !parsers_dir.exists() {
        return Ok(vec![]);
    }

    let mut plugins = Vec::new();
    for entry in std::fs::read_dir(&parsers_dir)
        .map_err(|e| format!("Failed to read parsers directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();
        if path.extension().map(|e| e == "py").unwrap_or(false) {
            // Return plugin name (file stem), not full path
            if let Some(name) = path.file_stem() {
                plugins.push(name.to_string_lossy().to_string());
            }
        }
    }

    Ok(plugins)
}

/// List plugins registered in the database (source of truth)
/// Use this for manual plugin selection - returns only properly registered plugins
#[tauri::command]
pub async fn list_registered_plugins(
    state: tauri::State<'_, crate::SentinelState>,
) -> Result<Vec<String>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let plugins: Vec<(String,)> = sqlx::query_as(
        "SELECT plugin_name FROM cf_plugin_manifest WHERE status = 'ACTIVE' ORDER BY plugin_name"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query registered plugins: {}", e))?;

    Ok(plugins.into_iter().map(|(name,)| name).collect())
}

/// Ensure a plugin's cache file exists, regenerating from DB if needed
#[tauri::command]
pub async fn ensure_plugin_cached(
    state: tauri::State<'_, crate::SentinelState>,
    plugin_name: String,
) -> Result<String, String> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| "Could not determine home directory".to_string())?;
    let parsers_dir = home_dir.join(".casparian_flow").join("parsers");
    let parser_path = parsers_dir.join(format!("{}.py", sanitize_filename(&plugin_name)));

    // Check if cache file exists
    if parser_path.exists() {
        return Ok(parser_path.display().to_string());
    }

    // Cache miss - regenerate from database
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let result: Option<(String,)> = sqlx::query_as(
        "SELECT source_code FROM cf_plugin_manifest WHERE plugin_name = ? AND status = 'ACTIVE'"
    )
    .bind(&plugin_name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to query plugin source: {}", e))?;

    let source_code = result
        .map(|(code,)| code)
        .ok_or_else(|| format!("Plugin '{}' not found in registry", plugin_name))?;

    // Write cache file
    std::fs::create_dir_all(&parsers_dir)
        .map_err(|e| format!("Failed to create parsers directory: {}", e))?;
    std::fs::write(&parser_path, &source_code)
        .map_err(|e| format!("Failed to write parser cache: {}", e))?;

    Ok(parser_path.display().to_string())
}

/// Chat with Claude to generate/refine parser code
///
/// This calls the Claude CLI to get AI assistance for parser development.
/// The prompt includes file preview and current code context.
#[tauri::command]
pub async fn parser_lab_chat(
    file_preview: String,
    current_code: String,
    user_message: String,
) -> Result<String, String> {
    use std::process::Stdio;
    use tokio::process::Command;
    use tokio::io::AsyncWriteExt;

    // Build the system prompt for parser generation
    let system_prompt = r#"You are an expert Python developer helping users write data parsers using Polars.

## Parser Output Modes

**SINGLE OUTPUT** - When the file contains one logical table:
```python
import polars as pl

def parse(input_path: str) -> pl.DataFrame:
    return pl.read_csv(input_path)
```

**MULTI OUTPUT (Demuxing)** - When the file contains MULTIPLE logical sections that should go to different sinks:
```python
import polars as pl

def parse(input_path: str) -> dict[str, pl.DataFrame]:
    # Read file once, split into logical sections
    content = open(input_path).read()

    return {
        "header": extract_header(content),      # e.g., invoice metadata
        "line_items": extract_items(content),   # e.g., product rows
        "totals": extract_totals(content),      # e.g., subtotal, tax, total
    }
```

## When to Demux (Multi-Output)

Use `dict[str, pl.DataFrame]` when the file contains:
- **Header + Detail pattern**: Invoice header info + line items
- **Multiple record types**: Logs with different event schemas
- **Sections with different schemas**: PDF with tables + metadata
- **Parent-child relationships**: Order header + order lines

Signs you should demux:
- Different parts have different column structures
- Data would naturally go to different database tables
- Mixing them in one DataFrame would require lots of null columns

## Topic Naming Rules

Topic names (dict keys) MUST be:
- Lowercase letters, numbers, underscores only
- Start with a letter
- Examples: `header`, `line_items`, `order_details`, `metadata_v2`

BAD: `"Line Items"`, `"Header!"`, `"123data"`, `"lineItems"`

## Code Guidelines

- Use Polars (pl) for all data manipulation
- Read the file ONCE, then split/extract sections
- Handle edge cases (empty sections, malformed data)
- Keep extraction logic in helper functions for clarity
- Every dict value MUST be a pl.DataFrame (not None, not a list)

Wrap code in ```python blocks so it can be applied to the editor."#;

    // Build the user context
    let context = if !file_preview.is_empty() || !current_code.is_empty() {
        let mut ctx = String::new();
        if !file_preview.is_empty() {
            ctx.push_str("Here's a preview of the input file:\n```\n");
            ctx.push_str(&file_preview);
            ctx.push_str("\n```\n\n");
        }
        if !current_code.is_empty() {
            ctx.push_str("Here's the current parser code:\n```python\n");
            ctx.push_str(&current_code);
            ctx.push_str("\n```\n\n");
        }
        ctx.push_str("User request: ");
        ctx.push_str(&user_message);
        ctx
    } else {
        user_message
    };

    // Call claude CLI
    let mut child = Command::new("claude")
        .args(["--print", "--dangerously-skip-permissions"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start claude CLI: {}. Make sure Claude Code is installed.", e))?;

    // Write the prompt to stdin
    let full_prompt = format!("{}\n\n{}", system_prompt, context);
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(full_prompt.as_bytes()).await
            .map_err(|e| format!("Failed to write to claude stdin: {}", e))?;
    }

    // Wait for the response
    let output = child.wait_with_output().await
        .map_err(|e| format!("Failed to get claude output: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude CLI failed: {}", stderr));
    }

    let response = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_schema_proposal_valid_camelcase() {
        // This is the format the LLM should return (camelCase to match serde rename_all)
        let response = r#"{
            "columns": [
                {"name": "id", "inferredType": "int64", "nullable": false, "description": "Primary key"},
                {"name": "created_at", "inferredType": "datetime", "nullable": true, "description": "Creation timestamp"}
            ],
            "suggestedSink": "parquet",
            "suggestedOutputPath": "/Users/test/.casparian_flow/output/test/test.parquet",
            "reasoning": "Parquet is best for analytics workloads"
        }"#;

        let result = parse_schema_proposal(response, "test_shard");
        assert!(result.is_ok(), "Failed to parse valid schema: {:?}", result);

        let proposal = result.unwrap();
        assert_eq!(proposal.columns.len(), 2);
        assert_eq!(proposal.columns[0].name, "id");
        assert_eq!(proposal.columns[0].inferred_type, "int64");
        assert!(!proposal.columns[0].nullable);
        assert_eq!(proposal.suggested_sink, "parquet");
    }

    #[test]
    fn test_parse_schema_proposal_rejects_snake_case() {
        // This is the OLD format that caused the bug - should fail
        let response = r#"{
            "columns": [
                {"name": "id", "inferred_type": "int64", "nullable": false, "description": "Primary key"}
            ],
            "suggested_sink": "parquet",
            "suggested_output_path": "/test/path",
            "reasoning": "Test"
        }"#;

        let result = parse_schema_proposal(response, "test_shard");
        assert!(result.is_err(), "Should reject snake_case format");
    }

    #[test]
    fn test_parse_schema_proposal_handles_markdown_wrapper() {
        // LLMs sometimes wrap JSON in markdown code blocks
        let response = r#"```json
{
    "columns": [
        {"name": "value", "inferredType": "float64", "nullable": false, "description": "Numeric value"}
    ],
    "suggestedSink": "csv",
    "suggestedOutputPath": "/test/output.csv",
    "reasoning": "CSV for interop"
}
```"#;

        let result = parse_schema_proposal(response, "test_shard");
        assert!(result.is_ok(), "Should handle markdown-wrapped JSON: {:?}", result);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("valid_name-123"), "valid_name-123");
        assert_eq!(sanitize_filename("has spaces"), "has_spaces");
        assert_eq!(sanitize_filename("special!@#chars"), "special___chars");
        assert_eq!(sanitize_filename("RFC_DB:record"), "RFC_DB_record");
    }
}
