//! Scout integration for Tauri
//!
//! Scout is the File Discovery + Tagging layer.
//! It discovers files and assigns tags based on patterns.
//! Actual processing happens in Sentinel (Tag → Plugin → Sink).

use casparian_scout::{
    Database as ScoutDatabase, FileStatus, ScannedFile, Scanner, Source, SourceType, TaggingRule, Tagger,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::State;
use tokio::sync::Mutex;
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
    /// Parser environment manager
    parser_env: ParserEnvManager,
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
            parser_env: ParserEnvManager::new(),
        }
    }

    /// Get or initialize the database
    pub async fn get_db(&self) -> Result<ScoutDatabase, String> {
        let mut db_guard = self.database.lock().await;

        if db_guard.is_none() {
            let path = self.db_path.lock().await;
            let db = ScoutDatabase::open(&path)
                .await
                .map_err(|e| format!("Failed to open Scout database: {}", e))?;
            *db_guard = Some(db);
        }

        Ok(db_guard.as_ref().unwrap().clone())
    }

    /// Get the parser environment manager
    pub fn parser_env(&self) -> &ParserEnvManager {
        &self.parser_env
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

/// Scanned file information for the frontend
/// NOTE: This wrapper is needed because it transforms FileStatus enum to String
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
pub async fn scout_init_db(state: State<'_, ScoutState>, path: Option<String>) -> Result<(), String> {
    // If path provided, update the stored path
    if let Some(p) = path {
        let mut path_guard = state.db_path.lock().await;
        *path_guard = PathBuf::from(p);
    }

    // Initialize the database (get_db will create if needed)
    let _ = state.get_db().await?;
    info!("Scout database initialized");
    Ok(())
}

// ============================================================================
// Source Commands
// ============================================================================

#[tauri::command]
pub async fn scout_list_sources(state: State<'_, ScoutState>) -> Result<Vec<Source>, String> {
    let db = state.get_db().await?;
    db.list_sources()
        .await
        .map_err(|e| format!("Failed to list sources: {}", e))
}

#[tauri::command]
pub async fn scout_add_source(
    state: State<'_, ScoutState>,
    id: String,
    name: String,
    path: String,
) -> Result<(), String> {
    let db = state.get_db().await?;

    let source = Source {
        id,
        name,
        source_type: SourceType::Local,
        path,
        poll_interval_secs: 30,
        enabled: true,
    };

    db.upsert_source(&source)
        .await
        .map_err(|e| format!("Failed to add source: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn scout_remove_source(state: State<'_, ScoutState>, source_id: String) -> Result<(), String> {
    let db = state.get_db().await?;
    db.delete_source(&source_id)
        .await
        .map_err(|e| format!("Failed to remove source: {}", e))?;
    Ok(())
}

// ============================================================================
// Tagging Rule Commands
// ============================================================================

#[tauri::command]
pub async fn scout_list_tagging_rules(
    state: State<'_, ScoutState>,
) -> Result<Vec<TaggingRule>, String> {
    let db = state.get_db().await?;
    db.list_tagging_rules()
        .await
        .map_err(|e| format!("Failed to list tagging rules: {}", e))
}

#[tauri::command]
pub async fn scout_list_tagging_rules_for_source(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<Vec<TaggingRule>, String> {
    let db = state.get_db().await?;
    db.list_tagging_rules_for_source(&source_id)
        .await
        .map_err(|e| format!("Failed to list tagging rules: {}", e))
}

#[tauri::command]
pub async fn scout_add_tagging_rule(
    state: State<'_, ScoutState>,
    id: String,
    name: String,
    source_id: String,
    pattern: String,
    tag: String,
    priority: Option<i32>,
) -> Result<TaggingRule, String> {
    let db = state.get_db().await?;

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
        .await
        .map_err(|e| format!("Failed to add tagging rule: {}", e))?;

    Ok(rule)
}

#[tauri::command]
pub async fn scout_remove_tagging_rule(
    state: State<'_, ScoutState>,
    rule_id: String,
) -> Result<(), String> {
    let db = state.get_db().await?;
    db.delete_tagging_rule(&rule_id)
        .await
        .map_err(|e| format!("Failed to remove tagging rule: {}", e))?;
    Ok(())
}

// ============================================================================
// File Commands
// ============================================================================

#[tauri::command]
pub async fn scout_list_files(
    state: State<'_, ScoutState>,
    source_id: String,
    status: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    let db = state.get_db().await?;
    let limit = limit.unwrap_or(1000);

    let files = if let Some(status_str) = status {
        if let Some(file_status) = FileStatus::parse(&status_str) {
            db.list_files_by_source_and_status(&source_id, file_status, limit).await
        } else {
            db.list_files_by_source(&source_id, limit).await
        }
    } else {
        db.list_files_by_source(&source_id, limit).await
    }
    .map_err(|e| format!("Failed to list files: {}", e))?;

    Ok(files.into_iter().map(FileInfo::from).collect())
}

#[tauri::command]
pub async fn scout_list_untagged_files(
    state: State<'_, ScoutState>,
    source_id: String,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    let db = state.get_db().await?;
    let files = db.list_untagged_files(&source_id, limit.unwrap_or(1000))
        .await
        .map_err(|e| format!("Failed to list untagged files: {}", e))?;

    Ok(files.into_iter().map(FileInfo::from).collect())
}

#[tauri::command]
pub async fn scout_list_files_by_tag(
    state: State<'_, ScoutState>,
    tag: String,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    let db = state.get_db().await?;
    let files = db.list_files_by_tag(&tag, limit.unwrap_or(1000))
        .await
        .map_err(|e| format!("Failed to list files by tag: {}", e))?;

    Ok(files.into_iter().map(FileInfo::from).collect())
}

#[tauri::command]
pub async fn scout_tag_files(
    state: State<'_, ScoutState>,
    file_ids: Vec<i64>,
    tag: String,
) -> Result<u64, String> {
    let db = state.get_db().await?;
    db.tag_files(&file_ids, &tag)
        .await
        .map_err(|e| format!("Failed to tag files: {}", e))
}

#[tauri::command]
pub async fn scout_set_manual_plugin(
    state: State<'_, ScoutState>,
    file_id: i64,
    plugin_name: String,
) -> Result<(), String> {
    let db = state.get_db().await?;
    db.set_manual_plugin(file_id, &plugin_name)
        .await
        .map_err(|e| format!("Failed to set manual plugin: {}", e))
}

#[tauri::command]
pub async fn scout_clear_manual_overrides(
    state: State<'_, ScoutState>,
    file_id: i64,
) -> Result<(), String> {
    let db = state.get_db().await?;
    db.clear_manual_overrides(file_id)
        .await
        .map_err(|e| format!("Failed to clear manual overrides: {}", e))
}

// ============================================================================
// File Lookup Commands
// ============================================================================

#[tauri::command]
pub async fn scout_get_file(
    state: State<'_, ScoutState>,
    file_id: i64,
) -> Result<Option<ScannedFile>, String> {
    let db = state.get_db().await?;
    db.get_file(file_id)
        .await
        .map_err(|e| format!("Failed to get file: {}", e))
}

#[tauri::command]
pub async fn scout_list_manual_files(
    state: State<'_, ScoutState>,
    source_id: String,
    limit: usize,
) -> Result<Vec<ScannedFile>, String> {
    let db = state.get_db().await?;
    db.list_manual_files(&source_id, limit)
        .await
        .map_err(|e| format!("Failed to list manual files: {}", e))
}

// ============================================================================
// Scanning Commands
// ============================================================================

#[tauri::command]
pub async fn scout_scan_source(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<ScanStats, String> {
    let db = state.get_db().await?;

    // Get the source
    let source = db.get_source(&source_id)
        .await
        .map_err(|e| format!("Failed to get source: {}", e))?
        .ok_or_else(|| format!("Source not found: {}", source_id))?;

    // Scan
    let scanner = Scanner::new(db);
    let result = scanner.scan_source(&source)
        .await
        .map_err(|e| format!("Scan failed: {}", e))?;

    Ok(ScanStats {
        files_discovered: result.stats.files_discovered,
        files_new: result.stats.files_new,
        files_changed: result.stats.files_changed,
        files_deleted: result.stats.files_deleted,
        bytes_scanned: result.stats.bytes_scanned,
        duration_ms: result.stats.duration_ms,
        errors: result.errors.into_iter().map(|(p, e)| format!("{}: {}", p, e)).collect(),
    })
}

#[tauri::command]
pub async fn scout_auto_tag(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<u64, String> {
    let db = state.get_db().await?;

    // Get tagging rules for this source
    let rules = db.list_tagging_rules_for_source(&source_id)
        .await
        .map_err(|e| format!("Failed to get tagging rules: {}", e))?;

    if rules.is_empty() {
        return Ok(0);
    }

    // Create tagger
    let tagger = Tagger::new(rules)
        .map_err(|e| format!("Failed to create tagger: {}", e))?;

    // Get pending files
    let pending = db.list_pending_files(&source_id, 10000)
        .await
        .map_err(|e| format!("Failed to list pending files: {}", e))?;

    let mut tagged = 0u64;
    for file in pending {
        if let Some((tag, rule_id)) = tagger.get_tag_with_rule_id(&file) {
            db.tag_file_by_rule(file.id.unwrap(), tag, rule_id)
                .await
                .map_err(|e| format!("Failed to tag file: {}", e))?;
            tagged += 1;
        }
    }

    Ok(tagged)
}

// ============================================================================
// Status Commands
// ============================================================================

#[tauri::command]
pub async fn scout_status(state: State<'_, ScoutState>) -> Result<ScoutStatus, String> {
    let db = state.get_db().await?;
    let stats = db.get_stats()
        .await
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
pub async fn scout_tag_stats(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<Vec<TagStats>, String> {
    let db = state.get_db().await?;
    let stats = db.get_tag_stats(&source_id)
        .await
        .map_err(|e| format!("Failed to get tag stats: {}", e))?;

    Ok(stats.into_iter().map(|(tag, total, processed, failed)| {
        TagStats { tag, total, processed, failed }
    }).collect())
}

// ============================================================================
// Pattern Preview Commands
// ============================================================================

#[tauri::command]
pub async fn scout_preview_pattern(
    state: State<'_, ScoutState>,
    source_id: String,
    pattern: String,
) -> Result<PatternPreview, String> {
    let db = state.get_db().await?;

    // Get all files for the source
    let files = db.list_files_by_source(&source_id, 10000)
        .await
        .map_err(|e| format!("Failed to list files: {}", e))?;

    // Try to compile the pattern
    let glob = match glob::Pattern::new(&pattern) {
        Ok(g) => g,
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

    // Match files
    let mut matched_count = 0u64;
    let mut matched_bytes = 0u64;
    let mut sample_files = Vec::new();

    for file in files {
        if glob.matches(&file.rel_path) {
            matched_count += 1;
            matched_bytes += file.size;
            if sample_files.len() < 10 {
                sample_files.push(file.rel_path);
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
pub async fn scout_analyze_coverage(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<TagCoverage, String> {
    let db = state.get_db().await?;

    // Get all files
    let files = db.list_files_by_source(&source_id, 100000)
        .await
        .map_err(|e| format!("Failed to list files: {}", e))?;

    // Get tagging rules
    let rules = db.list_tagging_rules_for_source(&source_id)
        .await
        .map_err(|e| format!("Failed to list rules: {}", e))?;

    // Create tagger
    let tagger = Tagger::new(rules.clone())
        .map_err(|e| format!("Failed to create tagger: {}", e))?;

    // Track statistics
    let mut rule_stats: HashMap<String, TagCoverageStats> = HashMap::new();
    let mut file_matches: HashMap<i64, Vec<String>> = HashMap::new();
    let mut untagged_files = Vec::new();
    let mut total_bytes = 0u64;
    let mut tagged_bytes = 0u64;

    // Initialize rule stats
    for rule in &rules {
        rule_stats.insert(rule.id.clone(), TagCoverageStats {
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            pattern: rule.pattern.clone(),
            tag: rule.tag.clone(),
            matched_count: 0,
            matched_bytes: 0,
            sample_files: vec![],
        });
    }

    // Analyze each file
    for file in &files {
        total_bytes += file.size;

        // Find matching rules
        let matching_rules = tagger.match_file(file);

        if matching_rules.is_empty() {
            if untagged_files.len() < 10 {
                untagged_files.push(file.rel_path.clone());
            }
        } else {
            tagged_bytes += file.size;

            // Track matches for overlap detection
            file_matches.insert(
                file.id.unwrap_or(0),
                matching_rules.iter().map(|r| r.id.clone()).collect()
            );

            // Update rule stats
            for rule in matching_rules {
                if let Some(stats) = rule_stats.get_mut(&rule.id) {
                    stats.matched_count += 1;
                    stats.matched_bytes += file.size;
                    if stats.sample_files.len() < 5 {
                        stats.sample_files.push(file.rel_path.clone());
                    }
                }
            }
        }
    }

    // Detect overlaps (files matched by multiple rules)
    let mut overlaps: Vec<RuleOverlap> = Vec::new();
    let mut overlap_pairs: HashMap<(String, String), (u64, Vec<String>)> = HashMap::new();

    for (file_id, rule_ids) in &file_matches {
        if rule_ids.len() > 1 {
            // This file matched multiple rules
            for i in 0..rule_ids.len() {
                for j in (i+1)..rule_ids.len() {
                    let pair = if rule_ids[i] < rule_ids[j] {
                        (rule_ids[i].clone(), rule_ids[j].clone())
                    } else {
                        (rule_ids[j].clone(), rule_ids[i].clone())
                    };

                    let entry = overlap_pairs.entry(pair).or_insert((0, Vec::new()));
                    entry.0 += 1;
                    if entry.1.len() < 3 {
                        // Find the file path
                        if let Some(file) = files.iter().find(|f| f.id == Some(*file_id)) {
                            entry.1.push(file.rel_path.clone());
                        }
                    }
                }
            }
        }
    }

    // Convert overlap map to list
    for ((rule1_id, rule2_id), (count, samples)) in overlap_pairs {
        let rule1 = rules.iter().find(|r| r.id == rule1_id);
        let rule2 = rules.iter().find(|r| r.id == rule2_id);

        if let (Some(r1), Some(r2)) = (rule1, rule2) {
            overlaps.push(RuleOverlap {
                rule1_id,
                rule1_name: r1.name.clone(),
                rule2_id,
                rule2_name: r2.name.clone(),
                overlap_count: count,
                sample_files: samples,
            });
        }
    }

    let total_files = files.len() as u64;
    let untagged_count = files.iter().filter(|f| !file_matches.contains_key(&f.id.unwrap_or(0))).count() as u64;
    let untagged_bytes = files.iter()
        .filter(|f| !file_matches.contains_key(&f.id.unwrap_or(0)))
        .map(|f| f.size)
        .sum();

    Ok(TagCoverage {
        rules: rule_stats.into_values().collect(),
        untagged_count,
        untagged_bytes,
        untagged_samples: untagged_files,
        overlaps,
        total_files,
        total_bytes,
        tagged_files: total_files - untagged_count,
        tagged_bytes,
    })
}

// ============================================================================
// Failed Files Commands
// ============================================================================

#[tauri::command]
pub async fn scout_list_failed_files(
    state: State<'_, ScoutState>,
    source_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<FileInfo>, String> {
    let db = state.get_db().await?;
    let limit = limit.unwrap_or(100);

    let files = if let Some(sid) = source_id {
        db.list_failed_files_for_source(&sid, limit).await
    } else {
        db.list_all_failed_files(limit).await
    }
    .map_err(|e| format!("Failed to list failed files: {}", e))?;

    Ok(files.into_iter().map(FileInfo::from).collect())
}

#[tauri::command]
pub async fn scout_retry_failed(
    state: State<'_, ScoutState>,
    source_id: String,
) -> Result<u64, String> {
    let db = state.get_db().await?;
    db.retry_failed_files(&source_id)
        .await
        .map_err(|e| format!("Failed to retry files: {}", e))
}

// ============================================================================
// Environment Management (for Python parser execution)
// ============================================================================

/// Environment manager for parser execution
pub struct ParserEnvManager {
    env_path: PathBuf,
}

impl ParserEnvManager {
    pub fn new() -> Self {
        let env_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".casparian_flow")
            .join("parser_env");

        Self { env_path }
    }

    pub async fn ensure_env(&self) -> Result<PathBuf, String> {
        if self.env_path.join("pyvenv.cfg").exists() {
            return Ok(self.env_path.clone());
        }

        // Create virtual environment
        info!("Creating parser environment at {:?}", self.env_path);

        let output = tokio::process::Command::new("python3")
            .args(["-m", "venv", self.env_path.to_str().unwrap()])
            .output()
            .await
            .map_err(|e| format!("Failed to create venv: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to create venv: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Install dependencies
        let pip = self.env_path.join("bin").join("pip");
        let output = tokio::process::Command::new(&pip)
            .args(["install", "polars", "pandas", "pyarrow"])
            .output()
            .await
            .map_err(|e| format!("Failed to install deps: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to install deps: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(self.env_path.clone())
    }

    pub fn python_path(&self) -> PathBuf {
        self.env_path.join("bin").join("python")
    }
}

impl Default for ParserEnvManager {
    fn default() -> Self {
        Self::new()
    }
}

#[tauri::command]
pub async fn ensure_parser_env(state: State<'_, ScoutState>) -> Result<String, String> {
    let path = state.parser_env().ensure_env().await?;
    Ok(path.to_string_lossy().to_string())
}

// ============================================================================
// Parser Lab Types
// ============================================================================

/// Parser Lab parser (full details)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserLabParser {
    pub id: String,
    pub name: String,
    pub file_pattern: String,
    pub pattern_type: String,
    pub source_code: Option<String>,
    pub validation_status: String,
    pub validation_error: Option<String>,
    pub validation_output: Option<String>,
    pub last_validated_at: Option<i64>,
    pub messages_json: Option<String>,
    pub schema_json: Option<String>,
    pub sink_type: String,
    pub sink_config_json: Option<String>,
    pub published_at: Option<i64>,
    pub published_plugin_id: Option<i64>,
    pub is_sample: bool,
    pub output_mode: String,
    pub detected_topics_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Parser Lab parser summary (for list view)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserLabParserSummary {
    pub id: String,
    pub name: String,
    pub file_pattern: String,
    pub pattern_type: String,
    pub validation_status: String,
    pub is_sample: bool,
    pub updated_at: i64,
    pub test_file_count: i64,
}

/// Parser Lab test file
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

// ============================================================================
// Parser Lab Parser Commands
// ============================================================================

/// SQL for selecting all parser fields
const PARSER_SELECT_SQL: &str = r#"
    SELECT id, name, file_pattern, pattern_type, source_code,
           validation_status, validation_error, validation_output, last_validated_at,
           messages_json, schema_json, sink_type, sink_config_json,
           published_at, published_plugin_id, is_sample, output_mode, detected_topics_json,
           created_at, updated_at
    FROM parser_lab_parsers"#;

/// Map an sqlx Row to ParserLabParser
fn row_to_parser(row: &sqlx::sqlite::SqliteRow) -> ParserLabParser {
    ParserLabParser {
        id: row.get(0),
        name: row.get(1),
        file_pattern: row.get::<Option<String>, _>(2).unwrap_or_default(),
        pattern_type: row.get::<Option<String>, _>(3).unwrap_or_else(|| "all".to_string()),
        source_code: row.get(4),
        validation_status: row.get::<Option<String>, _>(5).unwrap_or_else(|| "pending".to_string()),
        validation_error: row.get(6),
        validation_output: row.get(7),
        last_validated_at: row.get(8),
        messages_json: row.get(9),
        schema_json: row.get(10),
        sink_type: row.get::<Option<String>, _>(11).unwrap_or_else(|| "parquet".to_string()),
        sink_config_json: row.get(12),
        published_at: row.get(13),
        published_plugin_id: row.get(14),
        is_sample: row.get::<Option<i32>, _>(15).unwrap_or(0) != 0,
        output_mode: row.get::<Option<String>, _>(16).unwrap_or_else(|| "single".to_string()),
        detected_topics_json: row.get(17),
        created_at: row.get(18),
        updated_at: row.get(19),
    }
}

/// Create a new parser
#[tauri::command]
pub async fn parser_lab_create_parser(
    state: State<'_, ScoutState>,
    name: String,
    file_pattern: Option<String>,
) -> Result<ParserLabParser, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let pattern = file_pattern.unwrap_or_default();

    sqlx::query(
        r#"INSERT INTO parser_lab_parsers
           (id, name, file_pattern, pattern_type, validation_status, sink_type, is_sample, created_at, updated_at)
           VALUES (?, ?, ?, 'all', 'pending', 'parquet', 0, ?, ?)"#,
    )
    .bind(&id)
    .bind(&name)
    .bind(&pattern)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create parser: {}", e))?;

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
pub async fn parser_lab_get_parser(
    state: State<'_, ScoutState>,
    parser_id: String,
) -> Result<Option<ParserLabParser>, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    let sql = format!("{} WHERE id = ?", PARSER_SELECT_SQL);
    let result = sqlx::query(&sql)
        .bind(&parser_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to get parser: {}", e))?;

    Ok(result.map(|row| row_to_parser(&row)))
}

/// Update a parser
#[tauri::command]
pub async fn parser_lab_update_parser(
    state: State<'_, ScoutState>,
    parser: ParserLabParser,
) -> Result<(), String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"UPDATE parser_lab_parsers SET
           name = ?, file_pattern = ?, pattern_type = ?, source_code = ?,
           validation_status = ?, validation_error = ?, validation_output = ?,
           last_validated_at = ?, messages_json = ?, schema_json = ?,
           sink_type = ?, sink_config_json = ?, published_at = ?,
           published_plugin_id = ?, output_mode = ?, detected_topics_json = ?,
           updated_at = ?
           WHERE id = ?"#,
    )
    .bind(&parser.name)
    .bind(&parser.file_pattern)
    .bind(&parser.pattern_type)
    .bind(&parser.source_code)
    .bind(&parser.validation_status)
    .bind(&parser.validation_error)
    .bind(&parser.validation_output)
    .bind(parser.last_validated_at)
    .bind(&parser.messages_json)
    .bind(&parser.schema_json)
    .bind(&parser.sink_type)
    .bind(&parser.sink_config_json)
    .bind(parser.published_at)
    .bind(parser.published_plugin_id)
    .bind(&parser.output_mode)
    .bind(&parser.detected_topics_json)
    .bind(now)
    .bind(&parser.id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to update parser: {}", e))?;

    Ok(())
}

/// List all parsers with summaries
#[tauri::command]
pub async fn parser_lab_list_parsers(
    state: State<'_, ScoutState>,
    limit: Option<i32>,
) -> Result<Vec<ParserLabParserSummary>, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    let limit = limit.unwrap_or(50);

    let rows = sqlx::query(
        r#"SELECT p.id, p.name, p.file_pattern, p.pattern_type, p.validation_status,
                  p.is_sample, p.updated_at,
                  (SELECT COUNT(*) FROM parser_lab_test_files WHERE parser_id = p.id) as test_file_count
           FROM parser_lab_parsers p
           ORDER BY p.updated_at DESC
           LIMIT ?"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query parsers: {}", e))?;

    Ok(rows.iter().map(|row| {
        ParserLabParserSummary {
            id: row.get(0),
            name: row.get(1),
            file_pattern: row.get::<Option<String>, _>(2).unwrap_or_default(),
            pattern_type: row.get::<Option<String>, _>(3).unwrap_or_else(|| "all".to_string()),
            validation_status: row.get::<Option<String>, _>(4).unwrap_or_else(|| "pending".to_string()),
            is_sample: row.get::<Option<i32>, _>(5).unwrap_or(0) != 0,
            updated_at: row.get(6),
            test_file_count: row.get(7),
        }
    }).collect())
}

/// Delete a parser and all associated test files
#[tauri::command]
pub async fn parser_lab_delete_parser(
    state: State<'_, ScoutState>,
    parser_id: String,
) -> Result<(), String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    // Test files will be deleted by CASCADE
    sqlx::query("DELETE FROM parser_lab_parsers WHERE id = ?")
        .bind(&parser_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to delete parser: {}", e))?;

    Ok(())
}

// ============================================================================
// Parser Lab Test File Commands
// ============================================================================

/// Add a test file to a parser
#[tauri::command]
pub async fn parser_lab_add_test_file(
    state: State<'_, ScoutState>,
    parser_id: String,
    file_path: String,
) -> Result<ParserLabTestFile, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    // Get file name and size
    let path = std::path::Path::new(&file_path);
    let file_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.clone());

    let file_size = std::fs::metadata(&file_path)
        .map(|m| m.len() as i64)
        .ok();

    sqlx::query(
        r#"INSERT INTO parser_lab_test_files (id, parser_id, file_path, file_name, file_size, created_at)
           VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&parser_id)
    .bind(&file_path)
    .bind(&file_name)
    .bind(file_size)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to add test file: {}", e))?;

    Ok(ParserLabTestFile {
        id,
        parser_id,
        file_path,
        file_name,
        file_size,
        created_at: now,
    })
}

/// List test files for a parser
#[tauri::command]
pub async fn parser_lab_list_test_files(
    state: State<'_, ScoutState>,
    parser_id: String,
) -> Result<Vec<ParserLabTestFile>, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    let rows = sqlx::query(
        r#"SELECT id, parser_id, file_path, file_name, file_size, created_at
           FROM parser_lab_test_files
           WHERE parser_id = ?
           ORDER BY created_at DESC"#,
    )
    .bind(&parser_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list test files: {}", e))?;

    Ok(rows.iter().map(|row| {
        ParserLabTestFile {
            id: row.get(0),
            parser_id: row.get(1),
            file_path: row.get(2),
            file_name: row.get(3),
            file_size: row.get(4),
            created_at: row.get(5),
        }
    }).collect())
}

/// Remove a test file
#[tauri::command]
pub async fn parser_lab_remove_test_file(
    state: State<'_, ScoutState>,
    test_file_id: String,
) -> Result<(), String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    sqlx::query("DELETE FROM parser_lab_test_files WHERE id = ?")
        .bind(&test_file_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to remove test file: {}", e))?;

    Ok(())
}

// ============================================================================
// Parser Validation Commands
// ============================================================================

/// Validate a parser by running it against a test file
#[tauri::command]
pub async fn parser_lab_validate_parser(
    state: State<'_, ScoutState>,
    parser_id: String,
    test_file_id: String,
) -> Result<ParserLabParser, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    // Get parser
    let parser = parser_lab_get_parser(state.clone(), parser_id.clone())
        .await?
        .ok_or_else(|| "Parser not found".to_string())?;

    // Get test file
    let test_file_row = sqlx::query(
        "SELECT file_path FROM parser_lab_test_files WHERE id = ?",
    )
    .bind(&test_file_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to get test file: {}", e))?
    .ok_or_else(|| "Test file not found".to_string())?;

    let test_file_path: String = test_file_row.get(0);

    // Get source code
    let source_code = parser.source_code
        .ok_or_else(|| "No source code to validate".to_string())?;

    // Ensure environment
    let _env_path = state.parser_env().ensure_env().await?;
    let python = state.parser_env().python_path();

    // Create temp directory for parser files
    let temp_dir = std::env::temp_dir();
    let session_id = uuid::Uuid::new_v4();
    let parser_file = temp_dir.join(format!("parser_{}.py", session_id));
    let types_file = temp_dir.join(format!("casparian_types_{}.py", session_id));
    let wrapper_file = temp_dir.join(format!("validate_{}.py", session_id));

    // Write casparian_types.py
    let casparian_types = r#"
from typing import NamedTuple, Any

class Output(NamedTuple):
    name: str
    data: Any
    sink: str
    table: str | None = None
    compression: str = "snappy"

VALID_SINKS = frozenset(["parquet", "sqlite", "csv"])

def validate_output(output):
    if not output.name:
        raise ValueError("Output name cannot be empty")
    if not output.name[0].isalpha():
        raise ValueError(f"Output name must start with a letter: {output.name}")
    if not all(c.isalnum() or c == "_" for c in output.name):
        raise ValueError(f"Output name must be alphanumeric + underscore only: {output.name}")
    if output.name != output.name.lower():
        raise ValueError(f"Output name must be lowercase: {output.name}")
    if output.sink not in VALID_SINKS:
        raise ValueError(f"Invalid sink '{output.sink}'. Must be one of: {', '.join(sorted(VALID_SINKS))}")
    if output.data is None:
        raise ValueError(f"Output '{output.name}' has None data")
"#;
    std::fs::write(&types_file, casparian_types)
        .map_err(|e| format!("Failed to write types file: {}", e))?;

    // Write parser code
    std::fs::write(&parser_file, &source_code)
        .map_err(|e| format!("Failed to write parser file: {}", e))?;

    // Write validation wrapper that imports the parser and calls parse()
    let wrapper_code = format!(r#"
import sys
import json

# Add temp dir to path so we can import casparian_types and parser
sys.path.insert(0, "{temp_dir}")

# Import types first
import casparian_types_{session_id} as casparian_types
sys.modules['casparian_types'] = casparian_types

# Now import the parser
import parser_{session_id} as parser_module

# Get parse function
if not hasattr(parser_module, 'parse'):
    print(json.dumps({{"error": "Parser must define a 'parse' function"}}))
    sys.exit(1)

# Get TOPIC and SINK constants for single output
topic = getattr(parser_module, 'TOPIC', 'default')
sink = getattr(parser_module, 'SINK', 'parquet')

# Call parse
try:
    result = parser_module.parse("{test_file}")
except Exception as e:
    import traceback
    print(json.dumps({{"error": str(e), "traceback": traceback.format_exc()}}))
    sys.exit(1)

# Analyze result
outputs = []

if result is None:
    # Empty result
    pass
elif isinstance(result, list) and len(result) > 0 and isinstance(result[0], casparian_types.Output):
    # Multi-output: list[Output]
    for out in result:
        casparian_types.validate_output(out)
        row_count = len(out.data) if hasattr(out.data, '__len__') else 0
        if hasattr(out.data, 'shape'):
            row_count = out.data.shape[0]
        outputs.append({{
            "name": out.name,
            "sink": out.sink,
            "table": out.table,
            "compression": out.compression,
            "rows": row_count,
        }})
elif hasattr(result, 'shape') or hasattr(result, '__len__'):
    # Single output: bare DataFrame/Table
    row_count = result.shape[0] if hasattr(result, 'shape') else len(result)
    outputs.append({{
        "name": topic,
        "sink": sink,
        "table": None,
        "compression": "snappy",
        "rows": row_count,
    }})
else:
    print(json.dumps({{"error": f"parse() must return DataFrame, Table, or list[Output], got {{type(result).__name__}}"}}))
    sys.exit(1)

# Output results as JSON
print(json.dumps({{
    "success": True,
    "outputs": outputs,
    "output_mode": "multi" if len(outputs) > 1 else "single",
}}))
"#,
        temp_dir = temp_dir.to_str().unwrap().replace("\\", "\\\\"),
        session_id = session_id,
        test_file = test_file_path.replace("\\", "\\\\"),
    );
    std::fs::write(&wrapper_file, wrapper_code)
        .map_err(|e| format!("Failed to write wrapper file: {}", e))?;

    // Run validation
    let output = tokio::process::Command::new(&python)
        .arg(wrapper_file.to_str().unwrap())
        .output()
        .await
        .map_err(|e| format!("Failed to run parser: {}", e))?;

    // Clean up temp files
    let _ = std::fs::remove_file(&parser_file);
    let _ = std::fs::remove_file(&types_file);
    let _ = std::fs::remove_file(&wrapper_file);

    let now = chrono::Utc::now().timestamp_millis();

    // Parse validation result
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let (status, error, validation_output, output_mode, detected_topics_json) =
        if output.status.success() {
            // Try to parse JSON output
            match serde_json::from_str::<serde_json::Value>(&stdout) {
                Ok(json) if json.get("success").and_then(|v| v.as_bool()) == Some(true) => {
                    let outputs = json.get("outputs").and_then(|v| v.as_array());
                    let mode = json.get("output_mode")
                        .and_then(|v| v.as_str())
                        .unwrap_or("single")
                        .to_string();

                    // Build human-readable output
                    let mut display = String::new();
                    if let Some(outputs) = outputs {
                        for out in outputs {
                            let name = out.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                            let rows = out.get("rows").and_then(|v| v.as_i64()).unwrap_or(0);
                            let sink = out.get("sink").and_then(|v| v.as_str()).unwrap_or("parquet");
                            display.push_str(&format!("=== {} ({} rows) [{}] ===\n", name, rows, sink));
                        }
                    }

                    // Extract topic names for detected_topics_json
                    let topics: Vec<String> = outputs
                        .map(|arr| arr.iter()
                            .filter_map(|o| o.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
                            .collect())
                        .unwrap_or_default();
                    let topics_json = if topics.len() > 1 {
                        serde_json::to_string(&topics).ok()
                    } else {
                        None
                    };

                    ("valid".to_string(), None, Some(display), mode, topics_json)
                }
                Ok(json) if json.get("error").is_some() => {
                    let err = json.get("error").and_then(|v| v.as_str()).unwrap_or("Unknown error");
                    let tb = json.get("traceback").and_then(|v| v.as_str()).unwrap_or("");
                    let full_error = format!("{}\n{}", err, tb);
                    ("invalid".to_string(), Some(err.to_string()), Some(full_error), "single".to_string(), None)
                }
                _ => {
                    // Non-JSON output (legacy or print statements)
                    ("valid".to_string(), None, Some(stdout), "single".to_string(), None)
                }
            }
        } else {
            let combined = format!("{}\n{}", stderr, stdout);
            ("invalid".to_string(), Some(stderr.clone()), Some(combined), "single".to_string(), None)
        };

    // Update parser with output_mode and detected_topics
    sqlx::query(
        r#"UPDATE parser_lab_parsers SET
           validation_status = ?,
           validation_error = ?,
           validation_output = ?,
           last_validated_at = ?,
           output_mode = ?,
           detected_topics_json = ?,
           updated_at = ?
           WHERE id = ?"#,
    )
    .bind(&status)
    .bind(&error)
    .bind(&validation_output)
    .bind(now)
    .bind(&output_mode)
    .bind(&detected_topics_json)
    .bind(now)
    .bind(&parser_id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to update validation: {}", e))?;

    // Return updated parser
    parser_lab_get_parser(state, parser_id)
        .await?
        .ok_or_else(|| "Parser not found after update".to_string())
}

// ============================================================================
// Sample Parser Commands
// ============================================================================

/// Load the bundled sample parser
#[tauri::command]
pub async fn parser_lab_load_sample(
    state: State<'_, ScoutState>,
) -> Result<ParserLabParser, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    // Check if sample already exists
    let existing = sqlx::query("SELECT id FROM parser_lab_parsers WHERE is_sample = 1")
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to check sample: {}", e))?;

    if let Some(row) = existing {
        let id: String = row.get(0);
        return parser_lab_get_parser(state, id)
            .await?
            .ok_or_else(|| "Sample parser not found".to_string());
    }

    // Create sample directory
    let sample_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".casparian_flow")
        .join("samples");
    std::fs::create_dir_all(&sample_dir)
        .map_err(|e| format!("Failed to create sample dir: {}", e))?;

    // Write sample CSV
    let sample_csv = sample_dir.join("transactions.csv");
    if !sample_csv.exists() {
        let csv_content = r#"date,amount,category,description
2024-01-15,125.50,groceries,Weekly shopping
2024-01-16,-45.00,transfer,Payment to John
2024-01-17,2500.00,income,Salary deposit
2024-01-18,89.99,utilities,Electric bill
2024-01-19,15.00,entertainment,Movie tickets"#;
        std::fs::write(&sample_csv, csv_content)
            .map_err(|e| format!("Failed to write sample CSV: {}", e))?;
    }

    // Sample parser code - uses new Output contract
    let sample_code = r#""""Sample parser that processes transaction data."""
import polars as pl

TOPIC = "transactions"
SINK = "parquet"

def parse(input_path: str) -> pl.DataFrame:
    """
    Parse transaction CSV file.

    Args:
        input_path: Path to input CSV file

    Returns:
        Processed DataFrame with computed columns
    """
    # Read CSV with polars
    df = pl.read_csv(input_path)

    # Convert amount to proper numeric type
    df = df.with_columns([
        pl.col("amount").cast(pl.Float64),
        pl.col("date").str.to_date("%Y-%m-%d")
    ])

    # Add computed columns
    df = df.with_columns([
        (pl.col("amount") > 0).alias("is_credit"),
        pl.col("amount").abs().alias("abs_amount")
    ])

    return df
"#;

    // Create sample parser
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"INSERT INTO parser_lab_parsers
           (id, name, file_pattern, pattern_type, source_code, validation_status, sink_type, is_sample, created_at, updated_at)
           VALUES (?, 'Sample: Transaction Parser', '*.csv', 'glob', ?, 'pending', 'parquet', 1, ?, ?)"#,
    )
    .bind(&id)
    .bind(sample_code)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create sample parser: {}", e))?;

    // Add sample test file
    let test_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO parser_lab_test_files (id, parser_id, file_path, file_name, file_size, created_at)
           VALUES (?, ?, ?, 'transactions.csv', ?, ?)"#,
    )
    .bind(&test_id)
    .bind(&id)
    .bind(sample_csv.to_string_lossy().to_string())
    .bind(std::fs::metadata(&sample_csv).map(|m| m.len() as i64).ok())
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to add sample test file: {}", e))?;

    parser_lab_get_parser(state, id)
        .await?
        .ok_or_else(|| "Sample parser not found after creation".to_string())
}

// ============================================================================
// Import Parser from File
// ============================================================================

/// Import a parser from a Python file
#[tauri::command]
pub async fn parser_lab_import_plugin(
    state: State<'_, ScoutState>,
    plugin_path: String,
) -> Result<ParserLabParser, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    let path = std::path::Path::new(&plugin_path);

    // Read file
    let source_code = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // Get name from filename
    let name = path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Imported Parser".to_string());

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();

    sqlx::query(
        r#"INSERT INTO parser_lab_parsers
           (id, name, file_pattern, pattern_type, source_code, validation_status, sink_type, is_sample, created_at, updated_at)
           VALUES (?, ?, '', 'all', ?, 'pending', 'parquet', 0, ?, ?)"#,
    )
    .bind(&id)
    .bind(&name)
    .bind(&source_code)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to import parser: {}", e))?;

    parser_lab_get_parser(state, id)
        .await?
        .ok_or_else(|| "Parser not found after import".to_string())
}

// ============================================================================
// File Preview and Directory Commands
// ============================================================================

/// Preview first N lines of a file (for UI display)
#[tauri::command]
pub fn preview_shard(path: String, num_lines: Option<usize>) -> Result<Vec<String>, String> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let num_lines = num_lines.unwrap_or(30);

    let file = File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;
    let reader = BufReader::new(file);

    let lines: Vec<String> = reader
        .lines()
        .take(num_lines)
        .filter_map(|l| l.ok())
        .collect();

    Ok(lines)
}

/// Get the parsers directory path
#[tauri::command]
pub fn get_parsers_dir() -> Result<String, String> {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".casparian_flow")
        .join("parsers");

    // Ensure directory exists
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create parsers dir: {}", e))?;

    Ok(dir.to_string_lossy().to_string())
}

/// List all registered (active) plugins
#[tauri::command]
pub async fn list_registered_plugins(
    state: State<'_, ScoutState>,
) -> Result<Vec<String>, String> {
    let db = state.get_db().await?;
    let pool = db.pool();

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT plugin_name FROM cf_plugin_manifest WHERE status = 'ACTIVE' ORDER BY plugin_name"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list plugins: {}", e))?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

// ============================================================================
// Parser Publishing
// ============================================================================

/// Receipt returned after publishing a parser
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserPublishReceipt {
    pub success: bool,
    pub plugin_name: String,
    pub version: String,
    pub message: Option<String>,
}

/// Publish a parser as a plugin
#[tauri::command]
pub async fn publish_parser(
    state: State<'_, ScoutState>,
    parser_key: String,
    source_code: String,
    sink_type: String,
    output_path: Option<String>,
    output_mode: Option<String>,
    topic_uris_json: Option<String>,
    version: Option<String>,
) -> Result<ParserPublishReceipt, String> {
    use sha2::{Sha256, Digest};
    use std::fs;

    let db = state.get_db().await?;
    let pool = db.pool();

    let plugin_name = parser_key.clone();
    let plugin_version = version.unwrap_or_else(|| "1.0.0".to_string());
    let output_mode = output_mode.unwrap_or_else(|| "single".to_string());

    // Calculate source hash
    let mut hasher = Sha256::new();
    hasher.update(source_code.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    // Save parser to file
    let parsers_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".casparian_flow")
        .join("parsers");
    fs::create_dir_all(&parsers_dir).map_err(|e| format!("Failed to create parsers dir: {}", e))?;

    let safe_name = parser_key.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
    let parser_path = parsers_dir.join(format!("{}.py", safe_name));
    fs::write(&parser_path, &source_code).map_err(|e| format!("Failed to write parser: {}", e))?;

    // Insert into cf_plugin_manifest
    sqlx::query(
        "INSERT OR REPLACE INTO cf_plugin_manifest
         (plugin_name, version, source_code, source_hash, status, deployed_at)
         VALUES (?, ?, ?, ?, 'ACTIVE', datetime('now'))"
    )
    .bind(&plugin_name)
    .bind(&plugin_version)
    .bind(&source_code)
    .bind(&hash)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to insert plugin manifest: {}", e))?;

    // Insert into cf_plugin_config with subscription tag
    sqlx::query(
        "INSERT OR REPLACE INTO cf_plugin_config
         (plugin_name, subscription_tags, enabled)
         VALUES (?, ?, 1)"
    )
    .bind(&plugin_name)
    .bind(&parser_key) // subscription tag = parser key
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to insert plugin config: {}", e))?;

    // Handle topic configuration for multi-output mode
    if output_mode == "multi" {
        if let Some(json) = topic_uris_json {
            if let Ok(topics) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(obj) = topics.as_object() {
                    for (topic_name, value) in obj {
                        let (uri, topic_sink_type) = if let Some(s) = value.as_str() {
                            // Old format: string URI
                            let st = if s.starts_with("sqlite://") { "sqlite" }
                                     else if s.starts_with("csv://") { "csv" }
                                     else { "parquet" };
                            (s.to_string(), st.to_string())
                        } else if let Some(o) = value.as_object() {
                            // New format: { type, uri, path }
                            let st = o.get("type").and_then(|v| v.as_str()).unwrap_or("parquet");
                            let u = o.get("uri").and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| {
                                    let p = o.get("path").and_then(|v| v.as_str()).unwrap_or("");
                                    format!("{}://{}", st, p)
                                });
                            (u, st.to_string())
                        } else {
                            continue;
                        };

                        sqlx::query(
                            "INSERT OR REPLACE INTO cf_topic_config
                             (plugin_name, topic_name, uri, sink_type, enabled)
                             VALUES (?, ?, ?, ?, 1)"
                        )
                        .bind(&plugin_name)
                        .bind(topic_name)
                        .bind(&uri)
                        .bind(&topic_sink_type)
                        .execute(pool)
                        .await
                        .map_err(|e| format!("Failed to insert topic config: {}", e))?;
                    }
                }
            }
        }
    } else {
        // Single output mode - create default topic config
        let default_uri = output_path.unwrap_or_else(|| {
            format!("{}://~/.casparian_flow/output/{}/", sink_type, plugin_name)
        });

        sqlx::query(
            "INSERT OR REPLACE INTO cf_topic_config
             (plugin_name, topic_name, uri, sink_type, enabled)
             VALUES (?, 'default', ?, ?, 1)"
        )
        .bind(&plugin_name)
        .bind(&default_uri)
        .bind(&sink_type)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to insert topic config: {}", e))?;
    }

    Ok(ParserPublishReceipt {
        success: true,
        plugin_name: plugin_name.clone(),
        version: plugin_version,
        message: Some(format!("Plugin {} deployed successfully", plugin_name)),
    })
}

// ============================================================================
// Tag Validation
// ============================================================================

/// Result of subscription tag validation
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagValidationResult {
    pub valid: bool,
    pub exists: bool,
    pub existing_plugin_name: Option<String>,
}

/// Validate a subscription tag for uniqueness
#[tauri::command]
pub async fn validate_subscription_tag(
    state: State<'_, ScoutState>,
    tag: String,
    current_parser_id: Option<String>,
) -> Result<TagValidationResult, String> {
    // Basic format validation - alphanumeric, underscore, hyphen, dot only
    let valid_format = tag.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.');
    if !valid_format {
        return Ok(TagValidationResult {
            valid: false,
            exists: false,
            existing_plugin_name: None,
        });
    }

    let db = state.get_db().await?;
    let pool = db.pool();

    // Check if any plugin uses this tag
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT plugin_name FROM cf_plugin_config WHERE subscription_tags = ? LIMIT 1"
    )
    .bind(&tag)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to check tag: {}", e))?;

    if let Some((plugin_name,)) = existing {
        // Tag exists - check if it belongs to current parser
        // (allow updating own tag)
        let is_own_tag = current_parser_id
            .map(|id| id == plugin_name)
            .unwrap_or(false);

        if is_own_tag {
            return Ok(TagValidationResult {
                valid: true,
                exists: false,
                existing_plugin_name: None,
            });
        }

        return Ok(TagValidationResult {
            valid: true,
            exists: true,
            existing_plugin_name: Some(plugin_name),
        });
    }

    Ok(TagValidationResult {
        valid: true,
        exists: false,
        existing_plugin_name: None,
    })
}

// ============================================================================
// Parquet Query
// ============================================================================

/// Result of a parquet query
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub execution_time_ms: u64,
}

/// Query a parquet file with optional SQL
#[tauri::command]
pub async fn query_parquet(
    file_path: String,
    _sql: Option<String>,
) -> Result<QueryResult, String> {
    use parquet::file::reader::FileReader;
    use std::time::Instant;

    let start = Instant::now();

    // Read parquet file
    let file = std::fs::File::open(&file_path)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let reader = parquet::file::reader::SerializedFileReader::new(file)
        .map_err(|e| format!("Failed to read parquet: {}", e))?;

    let metadata = reader.metadata();
    let schema = metadata.file_metadata().schema_descr();

    // Get column names
    let columns: Vec<String> = schema
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();

    // Read rows (limited to first 1000 for preview)
    let mut rows: Vec<Vec<serde_json::Value>> = Vec::new();
    let mut row_count = 0;

    // Use arrow to read data
    let arrow_reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        std::fs::File::open(&file_path).map_err(|e| format!("Failed to reopen file: {}", e))?
    )
    .map_err(|e| format!("Failed to create arrow reader: {}", e))?
    .with_batch_size(1000)
    .build()
    .map_err(|e| format!("Failed to build reader: {}", e))?;

    for batch_result in arrow_reader {
        let batch = batch_result.map_err(|e| format!("Failed to read batch: {}", e))?;
        row_count += batch.num_rows();

        // Convert batch to rows (limit to 1000 total)
        if rows.len() < 1000 {
            for row_idx in 0..batch.num_rows().min(1000 - rows.len()) {
                let mut row: Vec<serde_json::Value> = Vec::new();
                for col_idx in 0..batch.num_columns() {
                    let col = batch.column(col_idx);
                    let value = arrow_value_to_json(col, row_idx);
                    row.push(value);
                }
                rows.push(row);
            }
        }
    }

    let execution_time_ms = start.elapsed().as_millis() as u64;

    Ok(QueryResult {
        columns,
        rows,
        row_count,
        execution_time_ms,
    })
}

/// Convert an arrow array value to JSON
fn arrow_value_to_json(array: &std::sync::Arc<dyn arrow::array::Array>, row: usize) -> serde_json::Value {
    use arrow::array::*;
    use arrow::datatypes::DataType;

    if array.is_null(row) {
        return serde_json::Value::Null;
    }

    match array.data_type() {
        DataType::Null => serde_json::Value::Null,
        DataType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            serde_json::Value::Bool(arr.value(row))
        }
        DataType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            serde_json::json!(arr.value(row))
        }
        DataType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            serde_json::Value::String(arr.value(row).to_string())
        }
        DataType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            serde_json::Value::String(arr.value(row).to_string())
        }
        _ => serde_json::Value::String(format!("<{:?}>", array.data_type())),
    }
}

// ============================================================================
// AI Chat (Stub)
// ============================================================================

/// AI chat for parser development assistance
/// Currently returns a placeholder - will be connected to LLM in future
#[tauri::command]
pub async fn parser_lab_chat(
    _file_preview: String,
    _current_code: String,
    user_message: String,
) -> Result<String, String> {
    // For now, return a helpful placeholder message
    // In the future, this will call an LLM API
    Ok(format!(
        "AI assistance is not yet configured. Your message: '{}'\n\n\
         To enable AI features, please configure an API key in settings.",
        user_message
    ))
}
