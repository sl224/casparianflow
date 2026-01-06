//! Casparian Deck - Tauri Desktop Application
//!
//! Embeds the Sentinel and provides real-time system monitoring via Tauri events.

mod scout;

use casparian::publish::analyze_plugin;
use casparian_sentinel::{Sentinel, SentinelConfig, METRICS};
use cf_security::Gatekeeper;
use serde::Serialize;
use sha2::{Sha256, Digest};
use sqlx::{Pool, Sqlite, SqlitePool};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, Mutex};
use tracing::{error, info};

/// System pulse event - emitted periodically with current metrics
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPulse {
    /// Number of connected workers (active - cleaned up)
    pub connected_workers: u64,
    /// Jobs completed in total
    pub jobs_completed: u64,
    /// Jobs failed in total
    pub jobs_failed: u64,
    /// Jobs dispatched in total
    pub jobs_dispatched: u64,
    /// Jobs currently in-flight (dispatched - completed - failed)
    pub jobs_in_flight: u64,
    /// Average dispatch latency in milliseconds
    pub avg_dispatch_ms: f64,
    /// Average conclude latency in milliseconds
    pub avg_conclude_ms: f64,
    /// Messages sent via ZMQ
    pub messages_sent: u64,
    /// Messages received via ZMQ
    pub messages_received: u64,
    /// Unix timestamp of this pulse
    pub timestamp: u64,
}

impl SystemPulse {
    /// Create from current metrics snapshot
    fn from_metrics() -> Self {
        let snapshot = METRICS.snapshot();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Calculate in-flight jobs (dispatched but not concluded)
        let concluded = snapshot.jobs_completed + snapshot.jobs_failed;
        let in_flight = snapshot.jobs_dispatched.saturating_sub(concluded);

        // Active workers = registered - cleaned up
        let active_workers = snapshot
            .workers_registered
            .saturating_sub(snapshot.workers_cleaned_up);

        SystemPulse {
            connected_workers: active_workers,
            jobs_completed: snapshot.jobs_completed,
            jobs_failed: snapshot.jobs_failed,
            jobs_dispatched: snapshot.jobs_dispatched,
            jobs_in_flight: in_flight,
            avg_dispatch_ms: snapshot.avg_dispatch_time_ms(),
            avg_conclude_ms: snapshot.avg_conclude_time_ms(),
            messages_sent: snapshot.messages_sent,
            messages_received: snapshot.messages_received,
            timestamp: now,
        }
    }
}

// ============================================================================
// Pipeline Topology Types
// ============================================================================

/// A node in the pipeline topology (plugin or topic)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyNode {
    pub id: String,
    pub label: String,
    pub node_type: String, // "plugin" or "topic"
    pub status: Option<String>,
    pub metadata: HashMap<String, String>,
    /// X position for layout (calculated by backend)
    pub x: f64,
    /// Y position for layout (calculated by backend)
    pub y: f64,
}

/// An edge connecting nodes in the pipeline
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: Option<String>,
    pub animated: bool,
}

/// Complete pipeline topology
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineTopology {
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

/// Job output information
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobOutput {
    pub job_id: i32,
    pub plugin_name: String,
    pub status: String,
    pub output_path: Option<String>,
    pub completed_at: Option<String>,
}

/// Detailed job information including logs/errors
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDetails {
    pub job_id: i32,
    pub plugin_name: String,
    pub status: String,
    pub output_path: Option<String>,
    pub error_message: Option<String>,
    pub result_summary: Option<String>,
    pub claim_time: Option<String>,
    pub end_time: Option<String>,
    pub retry_count: i32,
    /// Captured logs (stdout, stderr, logging) from plugin execution
    pub logs: Option<String>,
}

// ============================================================================
// Processing Failure Types (W3 - Failure Capture)
// ============================================================================

/// Detailed processing failure with line context for debugging
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingFailure {
    pub id: i64,
    pub job_id: Option<i64>,
    pub parser_id: Option<String>,
    pub test_file_id: Option<String>,
    pub file_path: Option<String>,
    pub line_number: Option<i64>,
    pub column_number: Option<i64>,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
    pub context_before: Option<String>,
    pub context_after: Option<String>,
    pub stack_trace: Option<String>,
    pub raw_input_sample: Option<String>,
    pub created_at: Option<String>,
}

// ============================================================================
// Routing & Configuration Types
// ============================================================================

/// A routing rule that maps file patterns to tags
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRule {
    pub id: i32,
    pub pattern: String,
    pub tag: String,
    pub priority: i32,
    pub enabled: bool,
    pub description: Option<String>,
}

/// Topic configuration for plugin outputs
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicConfig {
    pub id: i32,
    pub plugin_name: String,
    pub topic_name: String,
    pub uri: String,
    pub mode: String,
}

/// Sentinel state managed by Tauri
struct SentinelState {
    /// Signal to stop the sentinel (consumed on shutdown)
    #[allow(dead_code)]
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Flag indicating if sentinel is running
    running: Arc<AtomicBool>,
    /// The address the sentinel is bound to
    bind_addr: String,
    /// Database connection pool
    db_pool: Arc<Mutex<Option<Pool<Sqlite>>>>,
    /// Database URL (kept for reconnection if needed)
    #[allow(dead_code)]
    database_url: String,
}

/// Get current system metrics
#[tauri::command]
fn get_system_pulse() -> SystemPulse {
    SystemPulse::from_metrics()
}

/// Get metrics in Prometheus format
#[tauri::command]
fn get_prometheus_metrics() -> String {
    METRICS.prometheus_format()
}

/// Check if sentinel is running
#[tauri::command]
fn is_sentinel_running(state: tauri::State<'_, SentinelState>) -> bool {
    state.running.load(Ordering::Relaxed)
}

/// Get the sentinel bind address
#[tauri::command]
fn get_bind_address(state: tauri::State<'_, SentinelState>) -> String {
    state.bind_addr.clone()
}

/// Get pipeline topology from database
#[tauri::command]
async fn get_topology(state: tauri::State<'_, SentinelState>) -> Result<PipelineTopology, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    // Query plugins
    let plugins: Vec<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT plugin_name, subscription_tags, default_parameters
        FROM cf_plugin_config
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query plugins: {}", e))?;

    // Query topics
    let topics: Vec<(i32, String, String, String, String)> = sqlx::query_as(
        r#"
        SELECT id, plugin_name, topic_name, uri, mode
        FROM cf_topic_config
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query topics: {}", e))?;

    // Query subscriptions
    let subscriptions: Vec<(String, String, bool)> = sqlx::query_as(
        r#"
        SELECT plugin_name, topic_name, is_active
        FROM cf_plugin_subscriptions
        WHERE is_active = 1
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query subscriptions: {}", e))?;

    // Build nodes with layout positions
    // Layout: Plugins on left (x=100), Topics on right (x=500)
    // Vertical spacing: 120px between nodes
    const PLUGIN_X: f64 = 100.0;
    const TOPIC_X: f64 = 500.0;
    const VERTICAL_SPACING: f64 = 120.0;
    const START_Y: f64 = 50.0;

    let mut nodes = Vec::new();
    let mut topic_owners: HashMap<String, String> = HashMap::new();

    // Plugin nodes (left column)
    for (idx, (name, tags, _params)) in plugins.iter().enumerate() {
        let mut metadata = HashMap::new();
        metadata.insert("tags".to_string(), tags.clone());

        nodes.push(TopologyNode {
            id: format!("plugin:{}", name),
            label: name.clone(),
            node_type: "plugin".to_string(),
            status: Some("active".to_string()),
            metadata,
            x: PLUGIN_X,
            y: START_Y + (idx as f64 * VERTICAL_SPACING),
        });
    }

    // Topic nodes (right column)
    for (idx, (_id, plugin_name, topic_name, uri, mode)) in topics.iter().enumerate() {
        let mut metadata = HashMap::new();
        metadata.insert("uri".to_string(), uri.clone());
        metadata.insert("mode".to_string(), mode.clone());
        metadata.insert("owner".to_string(), plugin_name.clone());

        let topic_id = format!("topic:{}:{}", plugin_name, topic_name);
        topic_owners.insert(format!("{}:{}", plugin_name, topic_name), topic_id.clone());

        nodes.push(TopologyNode {
            id: topic_id,
            label: topic_name.clone(),
            node_type: "topic".to_string(),
            status: None,
            metadata,
            x: TOPIC_X,
            y: START_Y + (idx as f64 * VERTICAL_SPACING),
        });
    }

    // Build edges
    let mut edges = Vec::new();
    let mut edge_id = 0;

    // Plugin -> Topic (publish) edges
    for (_id, plugin_name, topic_name, _uri, mode) in &topics {
        if mode == "write" || mode == "rw" {
            let topic_id = format!("topic:{}:{}", plugin_name, topic_name);
            edges.push(TopologyEdge {
                id: format!("e{}", edge_id),
                source: format!("plugin:{}", plugin_name),
                target: topic_id,
                label: Some("publishes".to_string()),
                animated: true,
            });
            edge_id += 1;
        }
    }

    // Subscription edges (Topic -> Plugin)
    for (plugin_name, topic_name, is_active) in &subscriptions {
        // Find the topic owner - topic_name could be "owner:topic" format
        let topic_key = if topic_name.contains(':') {
            topic_name.clone()
        } else {
            // Try to find the topic by name
            topics
                .iter()
                .find(|(_, _, t, _, _)| t == topic_name)
                .map(|(_, p, t, _, _)| format!("{}:{}", p, t))
                .unwrap_or_else(|| format!("unknown:{}", topic_name))
        };

        if let Some(topic_id) = topic_owners.get(&topic_key) {
            edges.push(TopologyEdge {
                id: format!("e{}", edge_id),
                source: topic_id.clone(),
                target: format!("plugin:{}", plugin_name),
                label: Some("subscribes".to_string()),
                animated: *is_active,
            });
            edge_id += 1;
        }
    }

    Ok(PipelineTopology { nodes, edges })
}

/// Get list of completed and failed jobs with their outputs
#[tauri::command]
async fn get_job_outputs(
    state: tauri::State<'_, SentinelState>,
    limit: Option<i32>,
) -> Result<Vec<JobOutput>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let limit = limit.unwrap_or(50);

    // Include all job statuses - RUNNING/QUEUED first, then recent completed/failed
    let jobs: Vec<(i32, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT id, plugin_name, status, result_summary, end_time
        FROM cf_processing_queue
        WHERE status IN ('QUEUED', 'RUNNING', 'COMPLETED', 'FAILED')
        ORDER BY
            CASE status
                WHEN 'RUNNING' THEN 1
                WHEN 'QUEUED' THEN 2
                ELSE 3
            END,
            id DESC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query jobs: {}", e))?;

    Ok(jobs
        .into_iter()
        .map(|(id, plugin, status, summary, end_time)| {
            // Extract output path from result summary if available
            let output_path = summary
                .as_ref()
                .and_then(|s| {
                    if s.ends_with(".parquet") || s.ends_with(".csv") || s.ends_with(".json") {
                        Some(s.clone())
                    } else {
                        None
                    }
                });

            JobOutput {
                job_id: id,
                plugin_name: plugin,
                status,
                output_path,
                completed_at: end_time,
            }
        })
        .collect())
}

/// Get detailed information for a specific job (for LogViewer)
#[tauri::command]
async fn get_job_details(
    state: tauri::State<'_, SentinelState>,
    job_id: i32,
) -> Result<JobDetails, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    // Query job details from processing queue
    let job: (i32, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, i32) = sqlx::query_as(
        r#"
        SELECT id, plugin_name, status, result_summary, error_message,
               result_summary, claim_time, end_time, retry_count
        FROM cf_processing_queue
        WHERE id = ?
        "#,
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Job not found: {}", e))?;

    // Query logs from cold storage table (separate from hot queue)
    let logs: Option<String> = sqlx::query_scalar(
        r#"
        SELECT log_text
        FROM cf_job_logs
        WHERE job_id = ?
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to query logs: {}", e))?
    .flatten();

    // Extract output path from result_summary if it's a file path
    let output_path = job.3
        .as_ref()
        .and_then(|s| {
            if s.ends_with(".parquet") || s.ends_with(".csv") || s.ends_with(".json") {
                Some(s.clone())
            } else {
                None
            }
        });

    Ok(JobDetails {
        job_id: job.0,
        plugin_name: job.1,
        status: job.2,
        output_path,
        error_message: job.4,
        result_summary: job.5,
        claim_time: job.6,
        end_time: job.7,
        retry_count: job.8,
        logs,
    })
}

/// Cancel a running job
///
/// Marks the job as CANCELLED in the database. For ZMQ-based workers, this would
/// also send an ABORT message, but for subprocess-based execution, the process
/// will continue until completion (the status is just updated for display).
#[tauri::command]
async fn cancel_job(
    state: tauri::State<'_, SentinelState>,
    job_id: i64,
) -> Result<String, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    // Only cancel jobs that are currently RUNNING or QUEUED
    let result = sqlx::query(
        r#"
        UPDATE cf_processing_queue
        SET status = 'CANCELLED',
            error_message = 'Cancelled by user',
            end_time = datetime('now')
        WHERE id = ? AND status IN ('RUNNING', 'QUEUED')
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to cancel job: {}", e))?;

    if result.rows_affected() > 0 {
        info!(job_id, "Job cancelled by user");
        Ok(format!("Job {} cancelled", job_id))
    } else {
        Err(format!("Job {} not found or not in cancellable state", job_id))
    }
}

/// Information about a deployed plugin
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployedPlugin {
    pub plugin_name: String,
    pub version: String,
    pub status: String,
    pub deployed_at: Option<String>,
}

/// List all deployed plugins from the manifest
#[tauri::command]
async fn list_deployed_plugins(
    state: tauri::State<'_, SentinelState>,
) -> Result<Vec<DeployedPlugin>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let plugins: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT plugin_name, version, status, deployed_at
        FROM cf_plugin_manifest
        ORDER BY deployed_at DESC, plugin_name
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query plugins: {}", e))?;

    Ok(plugins
        .into_iter()
        .map(|(name, version, status, deployed_at)| DeployedPlugin {
            plugin_name: name,
            version,
            status,
            deployed_at,
        })
        .collect())
}

// ============================================================================
// Routing Rules CRUD Commands
// ============================================================================

/// Get all routing rules
#[tauri::command]
async fn get_routing_rules(
    state: tauri::State<'_, SentinelState>,
) -> Result<Vec<RoutingRule>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let rules: Vec<(i32, String, String, i32, i32, Option<String>)> = sqlx::query_as(
        r#"
        SELECT id, pattern, tag, priority, enabled, description
        FROM cf_routing_rules
        ORDER BY priority DESC, id
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query routing rules: {}", e))?;

    Ok(rules
        .into_iter()
        .map(|(id, pattern, tag, priority, enabled, description)| RoutingRule {
            id,
            pattern,
            tag,
            priority,
            enabled: enabled != 0,
            description,
        })
        .collect())
}

/// Create a new routing rule
#[tauri::command]
async fn create_routing_rule(
    state: tauri::State<'_, SentinelState>,
    pattern: String,
    tag: String,
    priority: i32,
    description: Option<String>,
) -> Result<i32, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let result = sqlx::query(
        r#"
        INSERT INTO cf_routing_rules (pattern, tag, priority, enabled, description)
        VALUES (?, ?, ?, 1, ?)
        "#,
    )
    .bind(&pattern)
    .bind(&tag)
    .bind(priority)
    .bind(&description)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create routing rule: {}", e))?;

    Ok(result.last_insert_rowid() as i32)
}

/// Update an existing routing rule
#[tauri::command]
async fn update_routing_rule(
    state: tauri::State<'_, SentinelState>,
    rule: RoutingRule,
) -> Result<(), String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    sqlx::query(
        r#"
        UPDATE cf_routing_rules
        SET pattern = ?, tag = ?, priority = ?, enabled = ?, description = ?
        WHERE id = ?
        "#,
    )
    .bind(&rule.pattern)
    .bind(&rule.tag)
    .bind(rule.priority)
    .bind(if rule.enabled { 1 } else { 0 })
    .bind(&rule.description)
    .bind(rule.id)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to update routing rule: {}", e))?;

    Ok(())
}

/// Delete a routing rule
#[tauri::command]
async fn delete_routing_rule(
    state: tauri::State<'_, SentinelState>,
    id: i32,
) -> Result<(), String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    sqlx::query("DELETE FROM cf_routing_rules WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to delete routing rule: {}", e))?;

    Ok(())
}

// ============================================================================
// Topic Configuration Commands
// ============================================================================

/// Get all topic configurations
#[tauri::command]
async fn get_topic_configs(
    state: tauri::State<'_, SentinelState>,
) -> Result<Vec<TopicConfig>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let topics: Vec<(i32, String, String, String, String)> = sqlx::query_as(
        r#"
        SELECT id, plugin_name, topic_name, uri, mode
        FROM cf_topic_config
        ORDER BY plugin_name, topic_name
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query topic configs: {}", e))?;

    Ok(topics
        .into_iter()
        .map(|(id, plugin_name, topic_name, uri, mode)| TopicConfig {
            id,
            plugin_name,
            topic_name,
            uri,
            mode,
        })
        .collect())
}

/// Update a topic's URI
#[tauri::command]
async fn update_topic_uri(
    state: tauri::State<'_, SentinelState>,
    id: i32,
    uri: String,
) -> Result<(), String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    sqlx::query("UPDATE cf_topic_config SET uri = ? WHERE id = ?")
        .bind(&uri)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to update topic URI: {}", e))?;

    Ok(())
}

// ============================================================================
// Scout-Sentinel Bridge Commands
// ============================================================================

/// Result of submitting tagged files to Sentinel
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitResult {
    /// Number of files submitted
    pub submitted: usize,
    /// Number of files skipped (no matching plugin)
    pub skipped: usize,
    /// Job IDs created (maps file_id -> job_id)
    pub job_ids: Vec<(i64, i64)>,
    /// Files skipped due to no plugin (file_id, tag)
    pub no_plugin: Vec<(i64, String)>,
}

/// Bridge a Scout file to Sentinel's file tracking tables.
/// Creates entries in cf_source_root, cf_file_location, cf_file_hash_registry, cf_file_version.
/// Returns the file_version_id to use in cf_processing_queue.
async fn ensure_file_in_sentinel(
    pool: &sqlx::SqlitePool,
    source_path: &str,
    rel_path: &str,
    content_hash: &str,
    size: u64,
    mtime: i64,
    tag: &str,
) -> Result<i64, String> {
    // 1. Ensure source_root exists (use path as unique key)
    let source_root_id: i64 = match sqlx::query_scalar::<_, i64>(
        "SELECT id FROM cf_source_root WHERE path = ?"
    )
    .bind(source_path)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to query source_root: {}", e))?
    {
        Some(id) => id,
        None => {
            sqlx::query("INSERT INTO cf_source_root (path) VALUES (?)")
                .bind(source_path)
                .execute(pool)
                .await
                .map_err(|e| format!("Failed to insert source_root: {}", e))?
                .last_insert_rowid()
        }
    };

    // 2. Ensure file_location exists
    let filename = std::path::Path::new(rel_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(rel_path);

    let location_id: i64 = match sqlx::query_scalar::<_, i64>(
        "SELECT id FROM cf_file_location WHERE source_root_id = ? AND rel_path = ?"
    )
    .bind(source_root_id)
    .bind(rel_path)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to query file_location: {}", e))?
    {
        Some(id) => {
            // Update existing
            sqlx::query(
                "UPDATE cf_file_location SET last_known_mtime=?, last_known_size=?, last_seen_time=CURRENT_TIMESTAMP WHERE id=?"
            )
            .bind(mtime as f64)
            .bind(size as i64)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to update file_location: {}", e))?;
            id
        }
        None => {
            sqlx::query(
                "INSERT INTO cf_file_location (source_root_id, rel_path, filename, last_known_mtime, last_known_size) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(source_root_id)
            .bind(rel_path)
            .bind(filename)
            .bind(mtime as f64)
            .bind(size as i64)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to insert file_location: {}", e))?
            .last_insert_rowid()
        }
    };

    // 3. Ensure hash_registry entry exists (with correct column names and required size_bytes)
    sqlx::query(
        "INSERT OR IGNORE INTO cf_file_hash_registry (content_hash, first_seen, size_bytes)
         VALUES (?, CURRENT_TIMESTAMP, ?)"
    )
    .bind(content_hash)
    .bind(size as i64)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to insert hash_registry: {}", e))?;

    // 4. Create or find file_version
    let mtime_str = chrono::DateTime::from_timestamp(mtime, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "1970-01-01 00:00:00".to_string());

    let file_version_id: i64 = match sqlx::query_scalar::<_, i64>(
        "SELECT id FROM cf_file_version WHERE location_id = ? AND content_hash = ?"
    )
    .bind(location_id)
    .bind(content_hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to query file_version: {}", e))?
    {
        Some(id) => {
            // Update tags
            sqlx::query("UPDATE cf_file_version SET applied_tags = ? WHERE id = ?")
                .bind(tag)
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| format!("Failed to update file_version: {}", e))?;
            id
        }
        None => {
            sqlx::query(
                "INSERT INTO cf_file_version (location_id, content_hash, size_bytes, modified_time, applied_tags) VALUES (?, ?, ?, ?, ?)"
            )
            .bind(location_id)
            .bind(content_hash)
            .bind(size as i64)
            .bind(&mtime_str)
            .bind(tag)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to insert file_version: {}", e))?
            .last_insert_rowid()
        }
    };

    // 5. Update location's current_version_id
    sqlx::query("UPDATE cf_file_location SET current_version_id = ? WHERE id = ?")
        .bind(file_version_id)
        .bind(location_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to update current_version: {}", e))?;

    Ok(file_version_id)
}

/// Submit tagged files to Sentinel's processing queue
///
/// This is the bridge between Scout (discovery + tagging) and Sentinel (processing).
/// For each tagged file:
/// 1. Look up plugins subscribed to the tag
/// 2. Create a job in cf_processing_queue
/// 3. Update Scout's file status to 'queued' with sentinel_job_id
#[tauri::command]
async fn submit_tagged_files(
    sentinel_state: tauri::State<'_, SentinelState>,
    scout_state: tauri::State<'_, scout::ScoutState>,
    file_ids: Vec<i64>,
) -> Result<SubmitResult, String> {
    // Get Scout database
    let scout_db = scout_state.get_db().await?;

    // Get Sentinel database pool
    let pool_guard = sentinel_state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Sentinel database not connected")?;

    // Load all plugin configs to match tags to plugins
    let plugin_configs: Vec<(String, String)> = sqlx::query_as(
        "SELECT plugin_name, subscription_tags FROM cf_plugin_config WHERE enabled = 1"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load plugin configs: {}", e))?;

    let mut submitted = 0;
    let mut skipped = 0;
    let mut job_ids: Vec<(i64, i64)> = Vec::new();
    let mut no_plugin: Vec<(i64, String)> = Vec::new();

    for file_id in file_ids {
        // Get file info from Scout
        let file = scout_db
            .get_file(file_id)
            .await
            .map_err(|e| format!("Failed to get file {}: {}", file_id, e))?;

        let Some(file) = file else {
            continue; // File not found, skip
        };

        let Some(tag) = &file.tag else {
            skipped += 1;
            continue; // No tag, skip
        };

        // Get plugin to use: manual override takes precedence over tag-based matching
        let plugin_name = if let Some(ref manual) = file.manual_plugin {
            // manual_plugin should be a plugin NAME, not a path
            // If it looks like a path, that's legacy data - reject it clearly
            if manual.contains('/') || manual.contains('\\') {
                no_plugin.push((file_id, format!(
                    "Manual plugin is a file path '{}'. Please clear override and re-select from dropdown.",
                    manual
                )));
                skipped += 1;
                continue;
            }

            // Verify plugin exists in registry
            if !plugin_configs.iter().any(|(name, _)| name == manual) {
                no_plugin.push((file_id, format!(
                    "Plugin '{}' not registered. Available: {:?}",
                    manual,
                    plugin_configs.iter().map(|(n, _)| n).collect::<Vec<_>>()
                )));
                skipped += 1;
                continue;
            }
            manual.clone()
        } else {
            // Tag-based matching: exact match only
            let matching_plugins: Vec<&str> = plugin_configs
                .iter()
                .filter(|(_, tags)| tags.split(',').any(|t| t.trim() == tag))
                .map(|(name, _)| name.as_str())
                .collect();

            if matching_plugins.is_empty() {
                no_plugin.push((file_id, tag.clone()));
                skipped += 1;
                continue;
            }

            // For now, use the first matching plugin
            // Future: could create multiple jobs for multiple plugins
            matching_plugins[0].to_string()
        };

        // Get source path for bridging to Sentinel
        let source = scout_db
            .get_source(&file.source_id)
            .await
            .map_err(|e| format!("Failed to get source {}: {}", file.source_id, e))?
            .ok_or_else(|| format!("Source {} not found", file.source_id))?;

        // Bridge Scout file to Sentinel's file tracking tables
        // Generate a pseudo-hash if Scout file doesn't have a real content hash
        let content_hash = file.content_hash.clone().unwrap_or_else(|| {
            format!("scout:{}:{}:{}", file.rel_path, file.mtime, file.size)
        });
        let file_version_id = ensure_file_in_sentinel(
            pool,
            &source.path,
            &file.rel_path,
            &content_hash,
            file.size,
            file.mtime,
            tag,
        )
        .await?;

        // Insert job into Sentinel's queue
        let result = sqlx::query(
            r#"
            INSERT INTO cf_processing_queue
                (file_version_id, plugin_name, status, priority, config_overrides)
            VALUES (?, ?, 'QUEUED', 0, NULL)
            "#,
        )
        .bind(file_version_id)
        .bind(&plugin_name)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to create job for file {}: {}", file_id, e))?;

        let job_id = result.last_insert_rowid();

        // Update Scout file status to queued
        scout_db
            .mark_file_queued(file_id, job_id)
            .await
            .map_err(|e| format!("Failed to mark file {} as queued: {}", file_id, e))?;

        job_ids.push((file_id, job_id));
        submitted += 1;
    }

    info!(
        "Submitted {} files to Sentinel, {} skipped ({} no plugin)",
        submitted,
        skipped,
        no_plugin.len()
    );

    Ok(SubmitResult {
        submitted,
        skipped,
        job_ids,
        no_plugin,
    })
}

/// Spawn a worker process to execute a job
///
/// This runs `casparian process-job <job_id> --db <db_path> --output <output_path>`
/// in the background. The process runs independently and updates the job status
/// in the database when done.
#[tauri::command]
async fn process_job_async(
    state: tauri::State<'_, SentinelState>,
    job_id: i64,
) -> Result<(), String> {
    // Get database path from state
    let db_url = state.database_url.clone();

    // Extract file path from URL like "sqlite:/path?mode=rwc"
    let db_path = db_url
        .strip_prefix("sqlite:")
        .unwrap_or(&db_url)
        .split('?')
        .next()
        .unwrap_or(&db_url)
        .to_string();

    // Set up output directory (relative to db location)
    let db_parent = std::path::Path::new(&db_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    let output_dir = db_parent.join("output");

    info!(job_id, db = %db_path, output = %output_dir.display(), "Spawning job processor");

    // Find the casparian binary
    let casparian_path = find_casparian_binary()?;

    // Spawn the process
    let mut cmd = std::process::Command::new(&casparian_path);
    cmd.arg("process-job")
        .arg(job_id.to_string())
        .arg("--db")
        .arg(&db_path)
        .arg("--output")
        .arg(&output_dir);

    // Spawn without waiting
    match cmd.spawn() {
        Ok(child) => {
            info!(job_id, pid = child.id(), "Worker process spawned");
            Ok(())
        }
        Err(e) => {
            error!(job_id, error = %e, "Failed to spawn worker process");
            Err(format!("Failed to spawn worker: {}", e))
        }
    }
}

/// Find the casparian binary
fn find_casparian_binary() -> Result<std::path::PathBuf, String> {
    // First, try the current executable's directory (for bundled apps)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled = exe_dir.join("casparian");
            if bundled.exists() {
                return Ok(bundled);
            }
        }
    }

    // Then try the target directory (for development)
    for profile in ["release", "debug"] {
        let dev_path = std::path::PathBuf::from(format!("target/{}/casparian", profile));
        if dev_path.exists() {
            return Ok(dev_path);
        }
        // Also try parent directories (ui/src-tauri runs from within ui/)
        let parent_dev_path = std::path::PathBuf::from(format!("../../target/{}/casparian", profile));
        if parent_dev_path.exists() {
            return Ok(parent_dev_path);
        }
    }

    // Finally, try PATH
    if let Ok(path) = which::which("casparian") {
        return Ok(path);
    }

    Err("casparian binary not found".to_string())
}

/// Get plugins available for a tag
#[tauri::command]
async fn get_plugins_for_tag(
    state: tauri::State<'_, SentinelState>,
    tag: String,
) -> Result<Vec<String>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let plugins: Vec<(String, String)> = sqlx::query_as(
        "SELECT plugin_name, subscription_tags FROM cf_plugin_config WHERE enabled = 1"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query plugins: {}", e))?;

    // Exact match only - tags are stored without prefix
    let matching: Vec<String> = plugins
        .into_iter()
        .filter(|(_, tags)| tags.split(',').any(|t| t.trim() == tag))
        .map(|(name, _)| name)
        .collect();

    Ok(matching)
}

/// Sync job statuses from Sentinel (cf_processing_queue) back to Scout (scout_files).
///
/// This is called periodically to update Scout's file status based on job completion.
/// For each file with status IN ('queued', 'processing') and a sentinel_job_id:
/// - Query cf_processing_queue by job_id
/// - Update scout_files.status based on job status:
///   * QUEUED -> 'queued'
///   * RUNNING -> 'processing'
///   * COMPLETED -> 'processed'
///   * FAILED -> 'failed' (with error message)
///
/// Returns the count of files updated.
#[tauri::command]
async fn sync_scout_file_statuses(
    state: tauri::State<'_, SentinelState>,
) -> Result<u64, String> {
    // Get database pool (same database for Scout and Sentinel tables)
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    // Query all scout files that need status sync:
    // status IN ('queued', 'processing') AND sentinel_job_id IS NOT NULL
    let files_to_sync: Vec<(i64, i64)> = sqlx::query_as(
        r#"
        SELECT id, sentinel_job_id
        FROM scout_files
        WHERE status IN ('queued', 'processing')
          AND sentinel_job_id IS NOT NULL
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query files for sync: {}", e))?;

    if files_to_sync.is_empty() {
        return Ok(0);
    }

    let mut updated_count: u64 = 0;

    for (file_id, sentinel_job_id) in files_to_sync {
        // Query job status from Sentinel
        let job_result: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT status, error_message FROM cf_processing_queue WHERE id = ?"
        )
        .bind(sentinel_job_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to query job {}: {}", sentinel_job_id, e))?;

        let Some((job_status, error_message)) = job_result else {
            // Job not found - leave file status as-is
            continue;
        };

        // Map Sentinel job status to Scout file status
        let (new_status, new_error): (&str, Option<&str>) = match job_status.as_str() {
            "QUEUED" => ("queued", None),
            "RUNNING" => ("processing", None),
            "COMPLETED" => ("processed", None),
            "FAILED" => ("failed", error_message.as_deref()),
            _ => continue, // Unknown status, skip
        };

        // Update scout_files with new status
        if new_status == "processed" {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query("UPDATE scout_files SET status = ?, error = ?, processed_at = ? WHERE id = ?")
                .bind(new_status)
                .bind(new_error)
                .bind(now)
                .bind(file_id)
                .execute(pool)
                .await
                .map_err(|e| format!("Failed to update file {}: {}", file_id, e))?;
        } else {
            sqlx::query("UPDATE scout_files SET status = ?, error = ? WHERE id = ?")
                .bind(new_status)
                .bind(new_error)
                .bind(file_id)
                .execute(pool)
                .await
                .map_err(|e| format!("Failed to update file {}: {}", file_id, e))?;
        }

        updated_count += 1;
    }

    if updated_count > 0 {
        info!("Synced {} file statuses from Sentinel", updated_count);
    }

    Ok(updated_count)
}

// ============================================================================
// W3 - Processing Failure Capture Commands
// ============================================================================

/// Save a processing failure with detailed context for debugging
#[tauri::command]
async fn save_processing_failure(
    state: tauri::State<'_, SentinelState>,
    job_id: Option<i64>,
    parser_id: Option<String>,
    test_file_id: Option<String>,
    file_path: Option<String>,
    line_number: Option<i64>,
    column_number: Option<i64>,
    error_type: Option<String>,
    error_message: Option<String>,
    context_before: Option<String>,
    context_after: Option<String>,
    stack_trace: Option<String>,
    raw_input_sample: Option<String>,
) -> Result<i64, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let result = sqlx::query(
        r#"INSERT INTO processing_failures
           (job_id, parser_id, test_file_id, file_path, line_number, column_number,
            error_type, error_message, context_before, context_after, stack_trace, raw_input_sample)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(job_id)
    .bind(&parser_id)
    .bind(&test_file_id)
    .bind(&file_path)
    .bind(line_number)
    .bind(column_number)
    .bind(&error_type)
    .bind(&error_message)
    .bind(&context_before)
    .bind(&context_after)
    .bind(&stack_trace)
    .bind(&raw_input_sample)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to save processing failure: {}", e))?;

    let failure_id = result.last_insert_rowid();
    info!(
        failure_id,
        parser_id = ?parser_id,
        error_type = ?error_type,
        "Saved processing failure"
    );
    Ok(failure_id)
}

/// Get the most recent processing failure for a parser and test file
#[tauri::command]
async fn get_processing_failure(
    state: tauri::State<'_, SentinelState>,
    parser_id: String,
    test_file_id: String,
) -> Result<Option<ProcessingFailure>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let result: Option<(i64, Option<i64>, Option<String>, Option<String>, Option<String>,
                        Option<i64>, Option<i64>, Option<String>, Option<String>,
                        Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            r#"SELECT id, job_id, parser_id, test_file_id, file_path, line_number, column_number,
                      error_type, error_message, context_before, context_after, stack_trace,
                      raw_input_sample, created_at
               FROM processing_failures
               WHERE parser_id = ? AND test_file_id = ?
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(&parser_id)
        .bind(&test_file_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to get processing failure: {}", e))?;

    Ok(result.map(|row| ProcessingFailure {
        id: row.0,
        job_id: row.1,
        parser_id: row.2,
        test_file_id: row.3,
        file_path: row.4,
        line_number: row.5,
        column_number: row.6,
        error_type: row.7,
        error_message: row.8,
        context_before: row.9,
        context_after: row.10,
        stack_trace: row.11,
        raw_input_sample: row.12,
        created_at: row.13,
    }))
}

/// Get the most recent processing failure for a job ID
#[tauri::command]
async fn get_processing_failure_by_job(
    state: tauri::State<'_, SentinelState>,
    job_id: i64,
) -> Result<Option<ProcessingFailure>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    let result: Option<(i64, Option<i64>, Option<String>, Option<String>, Option<String>,
                        Option<i64>, Option<i64>, Option<String>, Option<String>,
                        Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            r#"SELECT id, job_id, parser_id, test_file_id, file_path, line_number, column_number,
                      error_type, error_message, context_before, context_after, stack_trace,
                      raw_input_sample, created_at
               FROM processing_failures
               WHERE job_id = ?
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(job_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to get processing failure by job: {}", e))?;

    Ok(result.map(|row| ProcessingFailure {
        id: row.0,
        job_id: row.1,
        parser_id: row.2,
        test_file_id: row.3,
        file_path: row.4,
        line_number: row.5,
        column_number: row.6,
        error_type: row.7,
        error_message: row.8,
        context_before: row.9,
        context_after: row.10,
        stack_trace: row.11,
        raw_input_sample: row.12,
        created_at: row.13,
    }))
}

// ============================================================================
// W4 - Parser Version Checking Commands
// ============================================================================

/// Parser version info for checking compatibility
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserVersionInfo {
    pub parser_id: String,
    pub parser_name: String,
    pub source_hash: Option<String>,
    pub deployed_hash: Option<String>,
    pub is_current: bool,
}

/// Get version info for a parser (comparing parser_lab vs deployed plugin)
#[tauri::command]
async fn get_parser_version_info(
    state: tauri::State<'_, SentinelState>,
    parser_id: String,
) -> Result<Option<ParserVersionInfo>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    // Get parser info including source_hash
    let parser_row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, source_hash FROM parser_lab_parsers WHERE id = ?"
    )
    .bind(&parser_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to get parser: {}", e))?;

    let Some((id, name, source_hash)) = parser_row else {
        return Ok(None);
    };

    // Get deployed plugin source_hash if exists (match by name)
    let deployed_hash: Option<String> = sqlx::query_scalar(
        "SELECT source_hash FROM cf_plugin_manifest WHERE plugin_name = ? AND status = 'ACTIVE' ORDER BY created_at DESC LIMIT 1"
    )
    .bind(&name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to get deployed plugin: {}", e))?
    .flatten();

    let is_current = match (&source_hash, &deployed_hash) {
        (Some(s), Some(d)) => s == d,
        (None, None) => true,  // Both null = consider current
        _ => false,
    };

    Ok(Some(ParserVersionInfo {
        parser_id: id,
        parser_name: name,
        source_hash,
        deployed_hash,
        is_current,
    }))
}

/// Check if a job used the current parser version
#[tauri::command]
async fn check_job_parser_version(
    state: tauri::State<'_, SentinelState>,
    job_id: i64,
) -> Result<Option<ParserVersionInfo>, String> {
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    // Get job's plugin_name and parser_source_hash
    let job_row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT plugin_name, parser_source_hash FROM cf_processing_queue WHERE id = ?"
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to get job: {}", e))?;

    let Some((plugin_name, job_hash)) = job_row else {
        return Ok(None);
    };

    // Get current deployed source_hash
    let current_hash: Option<String> = sqlx::query_scalar(
        "SELECT source_hash FROM cf_plugin_manifest WHERE plugin_name = ? AND status = 'ACTIVE' ORDER BY created_at DESC LIMIT 1"
    )
    .bind(&plugin_name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to get deployed plugin: {}", e))?
    .flatten();

    let is_current = match (&job_hash, &current_hash) {
        (Some(j), Some(c)) => j == c,
        (None, None) => true,
        _ => false,
    };

    Ok(Some(ParserVersionInfo {
        parser_id: String::new(),  // Not available from job
        parser_name: plugin_name,
        source_hash: job_hash,
        deployed_hash: current_hash,
        is_current,
    }))
}

/// Validate that a path is a safe plugin file path
/// Returns the canonicalized path if valid
fn validate_plugin_path(path: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::Path::new(path);

    // Must have .py extension
    if path.extension().and_then(|e| e.to_str()) != Some("py") {
        return Err("Only .py files are allowed".to_string());
    }

    // Canonicalize to resolve any .. or symlinks
    // For new files, canonicalize the parent
    let canonical = if path.exists() {
        std::fs::canonicalize(path)
            .map_err(|e| format!("Invalid path: {}", e))?
    } else {
        let parent = path.parent()
            .ok_or("Invalid path: no parent directory")?;
        let file_name = path.file_name()
            .ok_or("Invalid path: no file name")?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| format!("Invalid parent directory: {}", e))?;
        canonical_parent.join(file_name)
    };

    Ok(canonical)
}

/// Read a plugin source file
#[tauri::command]
async fn read_plugin_file(path: String) -> Result<String, String> {
    let canonical = validate_plugin_path(&path)?;

    tokio::fs::read_to_string(&canonical)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))
}

/// Write a plugin source file
#[tauri::command]
async fn write_plugin_file(path: String, content: String) -> Result<(), String> {
    let canonical = validate_plugin_path(&path)?;

    // Limit file size to prevent abuse (10MB max)
    if content.len() > 10 * 1024 * 1024 {
        return Err("File content too large (max 10MB)".to_string());
    }

    tokio::fs::write(&canonical, content)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))
}

/// List plugin files in a directory
#[tauri::command]
async fn list_plugins(dir: String) -> Result<Vec<String>, String> {
    // Canonicalize directory path
    let canonical_dir = std::fs::canonicalize(&dir)
        .map_err(|e| format!("Invalid directory: {}", e))?;

    if !canonical_dir.is_dir() {
        return Err("Path is not a directory".to_string());
    }

    let mut entries = tokio::fs::read_dir(&canonical_dir)
        .await
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut plugins = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "py").unwrap_or(false) {
            if let Some(name) = path.file_name() {
                plugins.push(name.to_string_lossy().to_string());
            }
        }
    }

    plugins.sort();
    Ok(plugins)
}

/// Result of plugin deployment
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployResult {
    pub success: bool,
    pub plugin_name: String,
    pub version: String,
    pub source_hash: String,
    pub validation_errors: Vec<String>,
}

/// Deploy a plugin - validates and saves to database
#[tauri::command]
async fn deploy_plugin(
    state: tauri::State<'_, SentinelState>,
    path: String,
    code: String,
) -> Result<DeployResult, String> {
    // Extract plugin name from path
    let plugin_name = std::path::Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid plugin path")?
        .to_string();

    // Calculate source hash
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    let source_hash = format!("{:x}", hasher.finalize());

    // Validate with Gatekeeper
    let gatekeeper = Gatekeeper::new();
    let validation_result = gatekeeper.validate(&code);

    if let Err(e) = validation_result {
        // Parse validation errors
        let error_str = e.to_string();
        let validation_errors: Vec<String> = error_str
            .lines()
            .filter(|l| l.starts_with("- ") || l.contains("Banned"))
            .map(|s| s.to_string())
            .collect();

        return Ok(DeployResult {
            success: false,
            plugin_name,
            version: "0.0.0".to_string(),
            source_hash,
            validation_errors: if validation_errors.is_empty() {
                vec![error_str]
            } else {
                validation_errors
            },
        });
    }

    // Get database pool
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    // Generate version (simple timestamp-based for now)
    let now = chrono::Utc::now();
    let version = now.format("%Y%m%d.%H%M%S").to_string();

    // Insert into cf_plugin_manifest table
    let result = sqlx::query(
        r#"
        INSERT INTO cf_plugin_manifest
            (plugin_name, version, source_code, source_hash, status, created_at)
        VALUES (?, ?, ?, ?, 'ACTIVE', ?)
        ON CONFLICT(plugin_name, version)
        DO UPDATE SET source_code = excluded.source_code,
                      source_hash = excluded.source_hash,
                      status = 'ACTIVE'
        "#,
    )
    .bind(&plugin_name)
    .bind(&version)
    .bind(&code)
    .bind(&source_hash)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await;

    match result {
        Ok(_) => {
            info!("Deployed plugin {} v{}", plugin_name, version);
            Ok(DeployResult {
                success: true,
                plugin_name,
                version,
                source_hash,
                validation_errors: vec![],
            })
        }
        Err(e) => Err(format!("Failed to save plugin: {}", e)),
    }
}

// ============================================================================
// Publish Wizard Commands (Real I/O, No Mocks)
// ============================================================================

/// Result of analyzing a plugin manifest
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub plugin_name: String,
    pub source_hash: String,
    pub is_valid: bool,
    pub validation_errors: Vec<String>,
    pub has_lockfile: bool,
    pub env_hash: Option<String>,
    pub handler_methods: Vec<String>,
    pub detected_topics: Vec<String>,
}

/// Analyze a plugin file on disk (Real I/O, Real AST parsing)
///
/// This reads the actual file from the filesystem and performs
/// real Gatekeeper validation using AST parsing.
#[tauri::command]
async fn analyze_plugin_manifest(path: String) -> Result<PluginManifest, String> {
    // Validate path is a .py file
    let plugin_path = Path::new(&path);
    if plugin_path.extension().and_then(|e| e.to_str()) != Some("py") {
        return Err("Only .py files are allowed".to_string());
    }

    // Real I/O: Read and analyze the plugin
    let analysis = analyze_plugin(plugin_path)
        .map_err(|e| format!("Failed to analyze plugin: {}", e))?;

    Ok(PluginManifest {
        plugin_name: analysis.plugin_name,
        source_hash: analysis.source_hash,
        is_valid: analysis.is_valid,
        validation_errors: analysis.validation_errors,
        has_lockfile: analysis.has_lockfile,
        env_hash: analysis.env_hash,
        handler_methods: analysis.handler_methods,
        detected_topics: analysis.detected_topics,
    })
}

/// Options for publishing with overrides
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishWithOverridesArgs {
    pub path: String,
    pub version: Option<String>,
    pub routing_pattern: Option<String>,
    pub routing_tag: Option<String>,
    pub routing_priority: Option<i32>,
    pub topic_uri_override: Option<String>,
}

/// Result of a successful publish with overrides
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishReceipt {
    pub success: bool,
    pub plugin_name: String,
    pub version: String,
    pub source_hash: String,
    pub env_hash: Option<String>,
    pub routing_rule_id: Option<i64>,
    pub topic_config_id: Option<i64>,
    pub message: String,
}

/// Publish a plugin with optional routing and topic overrides
///
/// This performs the complete publishing transaction:
/// 1. Validate the plugin (Real Gatekeeper AST parsing)
/// 2. Save to cf_plugin_manifest (Real SQLite)
/// 3. Create routing rule if specified (Real SQLite)
/// 4. Create/update topic config if specified (Real SQLite)
#[tauri::command]
async fn publish_with_overrides(
    state: tauri::State<'_, SentinelState>,
    args: PublishWithOverridesArgs,
) -> Result<PublishReceipt, String> {
    let plugin_path = Path::new(&args.path);

    // 1. Analyze plugin (Real I/O + Real AST)
    let analysis = analyze_plugin(plugin_path)
        .map_err(|e| format!("Failed to analyze plugin: {}", e))?;

    if !analysis.is_valid {
        return Ok(PublishReceipt {
            success: false,
            plugin_name: analysis.plugin_name,
            version: "0.0.0".to_string(),
            source_hash: analysis.source_hash,
            env_hash: analysis.env_hash,
            routing_rule_id: None,
            topic_config_id: None,
            message: format!("Validation failed: {}", analysis.validation_errors.join(", ")),
        });
    }

    // 2. Get database pool (Real SQLite connection)
    let pool_guard = state.db_pool.lock().await;
    let pool = pool_guard.as_ref().ok_or("Database not connected")?;

    // Generate version
    let now = chrono::Utc::now();
    let version = args.version.unwrap_or_else(|| now.format("%Y%m%d.%H%M%S").to_string());

    // 3. Insert into cf_plugin_manifest (Real SQL)
    sqlx::query(
        r#"
        INSERT INTO cf_plugin_manifest
            (plugin_name, version, source_code, source_hash, env_hash, status, created_at)
        VALUES (?, ?, ?, ?, ?, 'ACTIVE', ?)
        ON CONFLICT(plugin_name, version)
        DO UPDATE SET source_code = excluded.source_code,
                      source_hash = excluded.source_hash,
                      env_hash = excluded.env_hash,
                      status = 'ACTIVE'
        "#,
    )
    .bind(&analysis.plugin_name)
    .bind(&version)
    .bind(&analysis.source_code)
    .bind(&analysis.source_hash)
    .bind(&analysis.env_hash)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to save plugin: {}", e))?;

    info!("Saved plugin {} v{} to manifest", analysis.plugin_name, version);

    // 4. Create routing rule if specified (Real SQL)
    let mut routing_rule_id = None;
    if let (Some(pattern), Some(tag)) = (&args.routing_pattern, &args.routing_tag) {
        let priority = args.routing_priority.unwrap_or(50);
        let result = sqlx::query(
            r#"
            INSERT INTO cf_routing_rules (pattern, tag, priority, enabled, description)
            VALUES (?, ?, ?, 1, ?)
            "#,
        )
        .bind(pattern)
        .bind(tag)
        .bind(priority)
        .bind(format!("Auto-created for plugin {}", analysis.plugin_name))
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to create routing rule: {}", e))?;

        routing_rule_id = Some(result.last_insert_rowid());
        info!("Created routing rule {} -> {} (id: {:?})", pattern, tag, routing_rule_id);
    }

    // 5. Create/update topic config if specified (Real SQL)
    let mut topic_config_id = None;
    if let Some(uri) = &args.topic_uri_override {
        // Use first detected topic or "output"
        let topic_name = analysis.detected_topics.first()
            .map(|s| s.as_str())
            .unwrap_or("output");

        let result = sqlx::query(
            r#"
            INSERT INTO cf_topic_config (plugin_name, topic_name, uri, mode)
            VALUES (?, ?, ?, 'write')
            ON CONFLICT(plugin_name, topic_name)
            DO UPDATE SET uri = excluded.uri
            "#,
        )
        .bind(&analysis.plugin_name)
        .bind(topic_name)
        .bind(uri)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to create topic config: {}", e))?;

        topic_config_id = Some(result.last_insert_rowid());
        info!("Created/updated topic config {} -> {} (id: {:?})", topic_name, uri, topic_config_id);
    }

    Ok(PublishReceipt {
        success: true,
        plugin_name: analysis.plugin_name,
        version,
        source_hash: analysis.source_hash,
        env_hash: analysis.env_hash,
        routing_rule_id,
        topic_config_id,
        message: "Plugin published successfully".to_string(),
    })
}

/// Start the job processor loop
///
/// Polls the database every 2 seconds for QUEUED jobs and spawns worker processes.
fn start_job_processor(pool: Arc<Mutex<Option<Pool<Sqlite>>>>, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create job processor runtime");

        rt.block_on(async {
            info!("Job processor loop started");

            loop {
                // Check if we should stop
                if !running.load(Ordering::Relaxed) {
                    info!("Job processor loop stopping (running=false)");
                    break;
                }

                // Try to get the database pool
                let pool_guard = pool.lock().await;
                if let Some(db_pool) = pool_guard.as_ref() {
                    // Query for oldest QUEUED job
                    let job: Option<(i64, String)> = sqlx::query_as(
                        "SELECT id, plugin_name FROM cf_processing_queue WHERE status = 'QUEUED' ORDER BY id LIMIT 1"
                    )
                    .fetch_optional(db_pool)
                    .await
                    .ok()
                    .flatten();

                    if let Some((job_id, plugin_name)) = job {
                        info!(job_id, plugin = %plugin_name, "Found QUEUED job, marking as RUNNING");

                        // Mark as RUNNING with started_at timestamp
                        let update_result = sqlx::query(
                            "UPDATE cf_processing_queue SET status = 'RUNNING', started_at = datetime('now') WHERE id = ? AND status = 'QUEUED'"
                        )
                        .bind(job_id)
                        .execute(db_pool)
                        .await;

                        if let Err(e) = update_result {
                            error!(job_id, error = %e, "Failed to mark job as RUNNING");
                        } else {
                            // Get database path for the worker
                            // Extract path from the pool's connect options
                            // We need to get the database path from environment or default location
                            let db_path = std::env::var("CASPARIAN_DATABASE")
                                .map(|url| {
                                    url.strip_prefix("sqlite:")
                                        .unwrap_or(&url)
                                        .split('?')
                                        .next()
                                        .unwrap_or(&url)
                                        .to_string()
                                })
                                .unwrap_or_else(|_| {
                                    dirs::home_dir()
                                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                                        .join(".casparian_flow")
                                        .join("casparian_flow.sqlite3")
                                        .to_string_lossy()
                                        .to_string()
                                });

                            // Set up output directory
                            let db_parent = std::path::Path::new(&db_path)
                                .parent()
                                .unwrap_or(std::path::Path::new("."));
                            let output_dir = db_parent.join("output");

                            // Find and spawn the casparian binary
                            match find_casparian_binary() {
                                Ok(casparian_path) => {
                                    let mut cmd = std::process::Command::new(&casparian_path);
                                    cmd.arg("process-job")
                                        .arg(job_id.to_string())
                                        .arg("--db")
                                        .arg(&db_path)
                                        .arg("--output")
                                        .arg(&output_dir);

                                    match cmd.spawn() {
                                        Ok(child) => {
                                            info!(job_id, pid = child.id(), "Worker process spawned by job loop");
                                        }
                                        Err(e) => {
                                            error!(job_id, error = %e, "Failed to spawn worker process");
                                            // Mark job as FAILED since we couldn't spawn worker
                                            let _ = sqlx::query(
                                                "UPDATE cf_processing_queue SET status = 'FAILED', error_message = ?, end_time = datetime('now') WHERE id = ?"
                                            )
                                            .bind(format!("Failed to spawn worker: {}", e))
                                            .bind(job_id)
                                            .execute(db_pool)
                                            .await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(job_id, error = %e, "Failed to find casparian binary");
                                    // Mark job as FAILED
                                    let _ = sqlx::query(
                                        "UPDATE cf_processing_queue SET status = 'FAILED', error_message = ?, end_time = datetime('now') WHERE id = ?"
                                    )
                                    .bind(format!("Casparian binary not found: {}", e))
                                    .bind(job_id)
                                    .execute(db_pool)
                                    .await;
                                }
                            }
                        }
                    }
                }
                drop(pool_guard);

                // Sleep for 2 seconds before next poll
                tokio::time::sleep(Duration::from_secs(2)).await;
            }

            info!("Job processor loop stopped");
        });
    });
}

/// Start the pulse emitter task
fn start_pulse_emitter(app: AppHandle, running: Arc<AtomicBool>) {
    // Spawn a task that emits system pulse events every 500ms
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("Failed to create pulse runtime");

        rt.block_on(async {
            let mut interval = tokio::time::interval(Duration::from_millis(500));

            while running.load(Ordering::Relaxed) {
                interval.tick().await;

                let pulse = SystemPulse::from_metrics();

                // Emit to all windows
                if let Err(e) = app.emit("system-pulse", &pulse) {
                    error!("Failed to emit system-pulse: {}", e);
                }
            }

            info!("Pulse emitter stopped");
        });
    });
}

/// Start the Sentinel on a background thread
fn start_sentinel(
    running: Arc<AtomicBool>,
    bind_addr: String,
    database_url: String,
) -> oneshot::Sender<()> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    std::thread::spawn(move || {
        // Create a dedicated Tokio runtime for the Sentinel
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("sentinel")
            .build()
            .expect("Failed to create Sentinel runtime");

        rt.block_on(async {
            running.store(true, Ordering::Relaxed);

            let config = SentinelConfig {
                bind_addr: bind_addr.clone(),
                database_url: database_url.clone(),
            };

            match Sentinel::bind(config).await {
                Ok(mut sentinel) => {
                    info!("Sentinel started on {}", bind_addr);

                    // Run sentinel until shutdown signal
                    tokio::select! {
                        result = sentinel.run() => {
                            match result {
                                Ok(_) => info!("Sentinel stopped normally"),
                                Err(e) => error!("Sentinel error: {}", e),
                            }
                        }
                        _ = shutdown_rx => {
                            info!("Shutdown signal received, stopping Sentinel");
                            sentinel.stop();
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to start Sentinel: {}", e);
                }
            }

            running.store(false, Ordering::Relaxed);
            info!("Sentinel runtime stopped");
        });
    });

    shutdown_tx
}

/// Create Sentinel database tables if they don't exist
async fn create_sentinel_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Plugin manifest (source of truth for plugins)
    // Columns used: plugin_name, version, source_code, source_hash, env_hash, status, created_at
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_plugin_manifest (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            plugin_name TEXT NOT NULL,
            version TEXT NOT NULL,
            source_code TEXT NOT NULL,
            source_hash TEXT NOT NULL,
            env_hash TEXT,
            status TEXT DEFAULT 'ACTIVE',
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            deployed_at TEXT,
            UNIQUE(plugin_name, version)
        )"#,
    )
    .execute(pool)
    .await?;

    // Plugin config (subscription tags)
    // Columns used: plugin_name, subscription_tags, default_parameters, enabled
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_plugin_config (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            plugin_name TEXT UNIQUE NOT NULL,
            subscription_tags TEXT NOT NULL,
            default_parameters TEXT,
            enabled INTEGER DEFAULT 1
        )"#,
    )
    .execute(pool)
    .await?;

    // Plugin subscriptions (used by topology view)
    // Columns used: plugin_name, topic_name, is_active
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_plugin_subscriptions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            plugin_name TEXT NOT NULL,
            topic_name TEXT NOT NULL,
            is_active INTEGER DEFAULT 1,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(plugin_name, topic_name)
        )"#,
    )
    .execute(pool)
    .await?;

    // Topic config (output routing)
    // Columns used: plugin_name, topic_name, uri, mode, sink_type, schema_json
    // NOTE: schema_json is required by casparian_sentinel TopicConfig model
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_topic_config (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            plugin_name TEXT NOT NULL,
            topic_name TEXT NOT NULL,
            uri TEXT NOT NULL,
            mode TEXT DEFAULT 'write',
            sink_type TEXT DEFAULT 'parquet',
            schema_json TEXT,
            enabled INTEGER DEFAULT 1,
            UNIQUE(plugin_name, topic_name)
        )"#,
    )
    .execute(pool)
    .await?;

    // Migration: Add sink_type column if missing (for existing databases)
    let _ = sqlx::query("ALTER TABLE cf_topic_config ADD COLUMN sink_type TEXT DEFAULT 'parquet'")
        .execute(pool)
        .await;

    // Migration: Add schema_json column if missing (required by Sentinel)
    let _ = sqlx::query("ALTER TABLE cf_topic_config ADD COLUMN schema_json TEXT")
        .execute(pool)
        .await;

    // Routing rules (tag matching)
    // Columns used: pattern, tag, priority, enabled, description
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_routing_rules (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern TEXT NOT NULL,
            tag TEXT NOT NULL,
            priority INTEGER DEFAULT 0,
            enabled INTEGER DEFAULT 1,
            description TEXT
        )"#,
    )
    .execute(pool)
    .await?;

    // Processing queue
    // Columns used: file_version_id, plugin_name, input_file, status, priority,
    //               config_overrides, claim_time, end_time, result_summary, error_message, retry_count
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_processing_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_version_id INTEGER,
            plugin_name TEXT NOT NULL,
            input_file TEXT,
            status TEXT DEFAULT 'QUEUED',
            priority INTEGER DEFAULT 0,
            config_overrides TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            started_at TEXT,
            completed_at TEXT,
            claim_time TEXT,
            end_time TEXT,
            result_summary TEXT,
            error_message TEXT,
            retry_count INTEGER DEFAULT 0,
            logs TEXT,
            FOREIGN KEY (file_version_id) REFERENCES cf_file_version(id)
        )"#,
    )
    .execute(pool)
    .await?;

    // Job logs (cold storage for job execution logs)
    // Columns used: job_id, log_text
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_job_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id INTEGER NOT NULL,
            log_text TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (job_id) REFERENCES cf_processing_queue(id)
        )"#,
    )
    .execute(pool)
    .await?;

    // File tracking tables (for submit_tagged_files)
    // cf_source_root: path
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_source_root (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE
        )"#,
    )
    .execute(pool)
    .await?;

    // cf_file_location: source_root_id, rel_path, filename, last_known_mtime, last_known_size,
    //                   current_version_id, last_seen_time
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_file_location (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_root_id INTEGER NOT NULL,
            rel_path TEXT NOT NULL,
            filename TEXT NOT NULL,
            last_known_mtime REAL,
            last_known_size INTEGER,
            current_version_id INTEGER,
            last_seen_time TEXT DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (source_root_id) REFERENCES cf_source_root(id)
        )"#,
    )
    .execute(pool)
    .await?;

    // cf_file_hash_registry: content_hash, first_seen, size_bytes
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_file_hash_registry (
            content_hash TEXT PRIMARY KEY,
            first_seen TEXT DEFAULT CURRENT_TIMESTAMP,
            size_bytes INTEGER NOT NULL
        )"#,
    )
    .execute(pool)
    .await?;

    // cf_file_version: location_id, content_hash, size_bytes, modified_time, applied_tags
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS cf_file_version (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            location_id INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            modified_time TEXT,
            applied_tags TEXT DEFAULT '',
            FOREIGN KEY (location_id) REFERENCES cf_file_location(id),
            FOREIGN KEY (content_hash) REFERENCES cf_file_hash_registry(content_hash)
        )"#,
    )
    .execute(pool)
    .await?;

    // =========================================================================
    // W3 - Processing Failures (detailed error context for debugging)
    // =========================================================================
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS processing_failures (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id INTEGER,
            parser_id TEXT,
            test_file_id TEXT,
            file_path TEXT,
            line_number INTEGER,
            column_number INTEGER,
            error_type TEXT,
            error_message TEXT,
            context_before TEXT,
            context_after TEXT,
            stack_trace TEXT,
            raw_input_sample TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (job_id) REFERENCES cf_processing_queue(id)
        )"#,
    )
    .execute(pool)
    .await?;

    // Index for quick lookup by parser_id and test_file_id
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_processing_failures_parser ON processing_failures(parser_id, test_file_id)"
    )
    .execute(pool)
    .await;

    // Index for job lookup
    let _ = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_processing_failures_job ON processing_failures(job_id)"
    )
    .execute(pool)
    .await;

    // =========================================================================
    // W4 - Parser Versioning (source_hash tracking)
    // =========================================================================
    // Add source_hash column to parser_lab_parsers if missing
    let _ = sqlx::query("ALTER TABLE parser_lab_parsers ADD COLUMN source_hash TEXT")
        .execute(pool)
        .await;

    // Add parser_source_hash to cf_processing_queue to track which version was used
    let _ = sqlx::query("ALTER TABLE cf_processing_queue ADD COLUMN parser_source_hash TEXT")
        .execute(pool)
        .await;

    info!("Sentinel database tables created/verified");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("casparian=info".parse().unwrap())
                .add_directive("casparian_sentinel=info".parse().unwrap()),
        )
        .init();

    info!("Starting Casparian Deck");

    // Configuration (could be loaded from file/env in production)
    // Use Unix domain socket for local desktop app (faster, more secure)
    let bind_addr = std::env::var("CASPARIAN_BIND").unwrap_or_else(|_| {
        let socket_path = dirs::runtime_dir()
            .or_else(|| dirs::cache_dir())
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("casparian.sock");
        format!("ipc://{}", socket_path.display())
    });

    // Default to the database in ~/.casparian_flow/
    let database_url = std::env::var("CASPARIAN_DATABASE").unwrap_or_else(|_| {
        // Use standard location - always absolute path
        let cf_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".casparian_flow");
        // Create directory if it doesn't exist
        let _ = std::fs::create_dir_all(&cf_dir);
        let db_path = cf_dir.join("casparian_flow.sqlite3");
        // Use mode=rwc to auto-create
        format!("sqlite:{}?mode=rwc", db_path.display())
    });

    // Shared running flag
    let running = Arc::new(AtomicBool::new(false));

    // Database pool (initialized lazily in setup)
    let db_pool = Arc::new(Mutex::new(None));

    // Start the Sentinel (database will be created with ?mode=rwc)
    let shutdown_tx = start_sentinel(running.clone(), bind_addr.clone(), database_url.clone());

    // Create state
    let state = SentinelState {
        shutdown_tx: Some(shutdown_tx),
        running: running.clone(),
        bind_addr,
        db_pool: db_pool.clone(),
        database_url: database_url.clone(),
    };

    // Create Scout state
    let scout_state = scout::ScoutState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .manage(scout_state)
        .invoke_handler(tauri::generate_handler![
            get_system_pulse,
            get_prometheus_metrics,
            is_sentinel_running,
            get_bind_address,
            get_topology,
            get_job_outputs,
            get_job_details,
            list_deployed_plugins,
            read_plugin_file,
            write_plugin_file,
            list_plugins,
            deploy_plugin,
            // Routing rules CRUD
            get_routing_rules,
            create_routing_rule,
            update_routing_rule,
            delete_routing_rule,
            // Topic configuration
            get_topic_configs,
            update_topic_uri,
            // Scout-Sentinel Bridge
            submit_tagged_files,
            get_plugins_for_tag,
            process_job_async,
            sync_scout_file_statuses,
            // W3 - Processing Failure Capture
            save_processing_failure,
            get_processing_failure,
            get_processing_failure_by_job,
            // W4 - Parser Version Checking
            get_parser_version_info,
            check_job_parser_version,
            // Job Management
            cancel_job,
            // Publish Wizard (Real I/O)
            analyze_plugin_manifest,
            publish_with_overrides,
            // Scout commands (File Discovery + Tagging)
            scout::scout_init_db,
            scout::scout_list_sources,
            scout::scout_add_source,
            scout::scout_remove_source,
            scout::scout_scan_source,
            scout::scout_status,
            // Scout file commands
            scout::scout_list_files,
            scout::scout_list_files_by_tag,
            scout::scout_list_untagged_files,
            scout::scout_list_failed_files,
            // Scout tagging rule commands
            scout::scout_list_tagging_rules,
            scout::scout_list_tagging_rules_for_source,
            scout::scout_add_tagging_rule,
            scout::scout_remove_tagging_rule,
            // Scout tagging commands
            scout::scout_tag_files,
            scout::scout_auto_tag,
            scout::scout_tag_stats,
            // Scout analysis commands
            scout::scout_preview_pattern,
            scout::scout_analyze_coverage,
            scout::scout_retry_failed,
            // Scout manual override commands
            scout::scout_set_manual_plugin,
            scout::scout_clear_manual_overrides,
            scout::scout_list_manual_files,
            scout::scout_get_file,
            // Parser publishing
            scout::publish_parser,
            scout::validate_subscription_tag,
            // Utility commands
            scout::get_parsers_dir,
            scout::preview_shard,
            scout::ensure_parser_env,
            scout::query_parquet,
            // Parser Lab (v6 - parser-centric, no project layer)
            scout::parser_lab_create_parser,
            scout::parser_lab_get_parser,
            scout::parser_lab_update_parser,
            scout::parser_lab_delete_parser,
            scout::parser_lab_list_parsers,
            scout::parser_lab_add_test_file,
            scout::parser_lab_remove_test_file,
            scout::parser_lab_list_test_files,
            scout::parser_lab_validate_parser,
            scout::parser_lab_import_plugin,
            scout::parser_lab_load_sample,
            scout::parser_lab_chat,
            // Plugin registry commands
            scout::list_registered_plugins,
        ])
        .setup(move |app| {
            // Initialize database pool (blocking to ensure ready before commands)
            let db_pool_clone = db_pool.clone();
            let db_pool_for_jobs = db_pool.clone();
            let running_for_jobs = running.clone();
            let database_url_clone = database_url.clone();

            tauri::async_runtime::block_on(async move {
                // database_url already has ?mode=rwc, use it directly
                match SqlitePool::connect(&database_url_clone).await {
                    Ok(pool) => {
                        // Run migrations to create tables
                        if let Err(e) = create_sentinel_tables(&pool).await {
                            error!("Failed to create Sentinel tables: {}", e);
                        }
                        let mut guard = db_pool_clone.lock().await;
                        *guard = Some(pool);
                        info!("Sentinel database pool initialized: {}", database_url_clone);
                    }
                    Err(e) => {
                        error!("Failed to initialize Sentinel database pool: {} - {}", database_url_clone, e);
                    }
                }
            });

            // Start the pulse emitter after app is ready
            let app_handle = app.handle().clone();
            start_pulse_emitter(app_handle, running);

            // Start the job processor loop (polls for QUEUED jobs every 2 seconds)
            start_job_processor(db_pool_for_jobs, running_for_jobs);

            info!("Casparian Deck setup complete");
            Ok(())
        })
        .on_window_event(|window, event| {
            // Handle window close - graceful shutdown
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                info!("Window close requested, initiating graceful shutdown");

                // Get state and signal sentinel to stop
                let state = window.state::<SentinelState>();
                state.running.store(false, Ordering::Relaxed);

                // Let the window close naturally - no need to prevent and re-trigger
                // The sentinel will stop on its next loop iteration
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // Path Validation Tests
    // =========================================================================

    #[test]
    fn test_validate_plugin_path_rejects_non_py_files() {
        // Should reject non-.py files
        let result = validate_plugin_path("/tmp/test.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only .py files"));

        let result = validate_plugin_path("/tmp/test.rs");
        assert!(result.is_err());

        let result = validate_plugin_path("/tmp/test");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_plugin_path_accepts_py_files() {
        // Create a temp directory with a .py file
        let temp_dir = TempDir::new().unwrap();
        let py_file = temp_dir.path().join("test_plugin.py");
        std::fs::write(&py_file, "# test plugin").unwrap();

        let result = validate_plugin_path(py_file.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_plugin_path_resolves_relative_paths() {
        // Create a temp directory with a .py file
        let temp_dir = TempDir::new().unwrap();
        let py_file = temp_dir.path().join("plugin.py");
        std::fs::write(&py_file, "# test").unwrap();

        // Create a path with ..
        let evil_path = temp_dir.path().join("subdir/../plugin.py");
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let result = validate_plugin_path(evil_path.to_str().unwrap());
        // Should resolve to the canonical path
        assert!(result.is_ok());
        let canonical = result.unwrap();
        assert!(!canonical.to_string_lossy().contains(".."));
    }

    #[test]
    fn test_validate_plugin_path_new_file_in_valid_dir() {
        // For new files, should validate parent directory exists
        let temp_dir = TempDir::new().unwrap();
        let new_file = temp_dir.path().join("new_plugin.py");

        let result = validate_plugin_path(new_file.to_str().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_plugin_path_new_file_in_nonexistent_dir() {
        // Should fail if parent directory doesn't exist
        let result = validate_plugin_path("/nonexistent/dir/plugin.py");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid parent directory"));
    }

    // =========================================================================
    // SystemPulse Tests
    // =========================================================================

    #[test]
    fn test_system_pulse_from_metrics_zero_state() {
        // When no metrics have been recorded, pulse should show zeros
        // Note: This tests the initial state, actual metrics would need
        // the sentinel to be running
        let pulse = SystemPulse::from_metrics();

        // Should not panic and should have a valid timestamp
        assert!(pulse.timestamp > 0);
        // In-flight should never be negative
        assert!(pulse.jobs_in_flight <= pulse.jobs_dispatched);
    }

    #[test]
    fn test_system_pulse_success_rate_calculation() {
        // Test the success rate edge case when no jobs completed
        let pulse = SystemPulse {
            connected_workers: 0,
            jobs_completed: 0,
            jobs_failed: 0,
            jobs_dispatched: 0,
            jobs_in_flight: 0,
            avg_dispatch_ms: 0.0,
            avg_conclude_ms: 0.0,
            messages_sent: 0,
            messages_received: 0,
            timestamp: 0,
        };

        // When both completed and failed are 0, we should handle divide by zero
        let total = pulse.jobs_completed + pulse.jobs_failed;
        if total == 0 {
            // This is the expected case - no division needed
            assert_eq!(total, 0);
        } else {
            let rate = pulse.jobs_completed as f64 / total as f64;
            assert!(rate >= 0.0 && rate <= 1.0);
        }
    }

    // =========================================================================
    // Topology Tests
    // =========================================================================

    #[test]
    fn test_topology_node_serialization() {
        let node = TopologyNode {
            id: "plugin:test".to_string(),
            label: "test".to_string(),
            node_type: "plugin".to_string(),
            status: Some("active".to_string()),
            metadata: HashMap::new(),
            x: 100.0,
            y: 50.0,
        };

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"nodeType\":\"plugin\"")); // camelCase
        assert!(json.contains("\"id\":\"plugin:test\""));
        assert!(json.contains("\"x\":100.0"));
        assert!(json.contains("\"y\":50.0"));
    }

    #[test]
    fn test_topology_edge_serialization() {
        let edge = TopologyEdge {
            id: "e1".to_string(),
            source: "plugin:a".to_string(),
            target: "topic:b:c".to_string(),
            label: Some("publishes".to_string()),
            animated: true,
        };

        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("\"animated\":true"));
        assert!(json.contains("\"source\":\"plugin:a\""));
    }

    #[test]
    fn test_pipeline_topology_empty() {
        let topology = PipelineTopology {
            nodes: vec![],
            edges: vec![],
        };

        let json = serde_json::to_string(&topology).unwrap();
        assert!(json.contains("\"nodes\":[]"));
        assert!(json.contains("\"edges\":[]"));
    }

    // =========================================================================
    // File Bridging Tests (Scout -> Sentinel)
    // =========================================================================

    #[tokio::test]
    async fn test_ensure_file_in_sentinel_creates_fk_chain() {
        // Create in-memory SQLite database
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory database");

        // Create required tables (minimal schema for this test)
        sqlx::query(
            r#"
            CREATE TABLE cf_source_root (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE
            );
            CREATE TABLE cf_file_location (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_root_id INTEGER NOT NULL,
                rel_path TEXT NOT NULL,
                filename TEXT NOT NULL,
                last_known_mtime REAL,
                last_known_size INTEGER,
                current_version_id INTEGER,
                FOREIGN KEY (source_root_id) REFERENCES cf_source_root(id)
            );
            CREATE TABLE cf_file_hash_registry (
                content_hash TEXT PRIMARY KEY,
                first_seen TEXT DEFAULT CURRENT_TIMESTAMP,
                size_bytes INTEGER NOT NULL
            );
            CREATE TABLE cf_file_version (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                location_id INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                modified_time TEXT NOT NULL,
                applied_tags TEXT DEFAULT '',
                FOREIGN KEY (location_id) REFERENCES cf_file_location(id),
                FOREIGN KEY (content_hash) REFERENCES cf_file_hash_registry(content_hash)
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create schema");

        // Enable foreign keys
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .expect("Failed to enable foreign keys");

        // Call ensure_file_in_sentinel
        let result = ensure_file_in_sentinel(
            &pool,
            "/test/source",
            "data/test.csv",
            "scout:data/test.csv:1704067200:1024",
            1024,
            1704067200,
            "test-tag",
        )
        .await;

        assert!(result.is_ok(), "ensure_file_in_sentinel failed: {:?}", result);
        let file_version_id = result.unwrap();
        assert!(file_version_id > 0, "file_version_id should be positive");

        // Verify the FK chain was created correctly
        let source_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cf_source_root")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(source_count, 1, "Should have 1 source_root");

        let location_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cf_file_location")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(location_count, 1, "Should have 1 file_location");

        let hash_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cf_file_hash_registry")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(hash_count, 1, "Should have 1 hash_registry entry");

        let version_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cf_file_version")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(version_count, 1, "Should have 1 file_version");

        // Verify cf_processing_queue can reference this file_version_id
        sqlx::query(
            r#"
            CREATE TABLE cf_processing_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_version_id INTEGER NOT NULL,
                plugin_name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'QUEUED',
                FOREIGN KEY (file_version_id) REFERENCES cf_file_version(id)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create processing_queue table");

        // This should NOT fail with FK constraint error
        let insert_result = sqlx::query(
            "INSERT INTO cf_processing_queue (file_version_id, plugin_name, status) VALUES (?, ?, 'QUEUED')",
        )
        .bind(file_version_id)
        .bind("test-plugin")
        .execute(&pool)
        .await;

        assert!(
            insert_result.is_ok(),
            "Should be able to insert into processing_queue with file_version_id: {:?}",
            insert_result
        );
    }
}
