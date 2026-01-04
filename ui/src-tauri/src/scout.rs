//! Scout integration for Tauri
//!
//! Scout is the File Discovery + Tagging layer.
//! It discovers files and assigns tags based on patterns.
//! Actual processing happens in Sentinel (Tag → Plugin → Sink).

use casparian_scout::{
    Database as ScoutDatabase, FileStatus, Scanner, Source, SourceType, TaggingRule, Tagger,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;
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
        Self {
            database: Mutex::new(None),
            db_path: Mutex::new(PathBuf::from("scout.db")),
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
    pub error: Option<String>,
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

    Ok(files
        .into_iter()
        .map(|f| FileInfo {
            id: f.id.unwrap_or(0),
            source_id: f.source_id,
            path: f.path,
            rel_path: f.rel_path,
            size: f.size,
            status: f.status.as_str().to_string(),
            tag: f.tag,
            error: f.error,
        })
        .collect())
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

    Ok(files
        .into_iter()
        .map(|f| FileInfo {
            id: f.id.unwrap_or(0),
            source_id: f.source_id,
            path: f.path,
            rel_path: f.rel_path,
            size: f.size,
            status: f.status.as_str().to_string(),
            tag: f.tag,
            error: f.error,
        })
        .collect())
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

    Ok(files
        .into_iter()
        .map(|f| FileInfo {
            id: f.id.unwrap_or(0),
            source_id: f.source_id,
            path: f.path,
            rel_path: f.rel_path,
            size: f.size,
            status: f.status.as_str().to_string(),
            tag: f.tag,
            error: f.error,
        })
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
        if let Some(tag) = tagger.get_tag(file) {
            if let Some(file_id) = file.id {
                db.tag_file(file_id, tag)
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
