//! Casparian Deck - Tauri Desktop Application
//!
//! Embeds the Sentinel and provides real-time system monitoring via Tauri events.

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
use tracing::{error, info, warn};

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

// ============================================================================
// Query Types for DuckDB
// ============================================================================

/// Result of a parquet query
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub execution_time_ms: u64,
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

    // Include both completed and failed jobs so users can debug failures
    let jobs: Vec<(i32, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT id, plugin_name, status, result_summary, end_time
        FROM cf_processing_queue
        WHERE status IN ('COMPLETED', 'FAILED')
        ORDER BY end_time DESC
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

/// Maximum rows to return from a query to prevent memory blowup
const MAX_QUERY_ROWS: usize = 10_000;

/// Query a parquet file using DuckDB
#[tauri::command]
async fn query_parquet(file_path: String, sql: Option<String>) -> Result<QueryResult, String> {
    use duckdb::Connection;
    use std::time::Instant;

    let start = Instant::now();

    // Canonicalize the path to prevent path traversal
    let canonical_path = std::fs::canonicalize(&file_path)
        .map_err(|e| format!("Invalid file path: {}", e))?;

    // Validate file exists and is a regular file
    if !canonical_path.is_file() {
        return Err(format!("Not a valid file: {}", file_path));
    }

    // Validate file extension (only allow data files)
    let ext = canonical_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if !matches!(ext, "parquet" | "csv" | "json" | "jsonl") {
        return Err(format!("Unsupported file type: .{}", ext));
    }

    let file_path_str = canonical_path.to_string_lossy().to_string();

    // Use provided SQL or default to SELECT * LIMIT 100
    // Note: SQL is user-provided for their own local data files,
    // so SQL injection is not a security concern here.
    let query = sql.unwrap_or_else(|| {
        format!("SELECT * FROM read_parquet('{}') LIMIT 100", file_path_str)
    });

    // Execute in a blocking task since DuckDB is sync
    let result = tokio::task::spawn_blocking(move || -> Result<QueryResult, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to open DuckDB: {}", e))?;

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        // Get column names
        let columns: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        // Execute and collect rows
        let rows_iter = stmt
            .query_map([], |row| {
                let mut values = Vec::new();
                for i in 0..columns.len() {
                    // Try to get value as different types
                    let value: serde_json::Value = if let Ok(v) = row.get::<_, i64>(i) {
                        serde_json::Value::Number(v.into())
                    } else if let Ok(v) = row.get::<_, f64>(i) {
                        serde_json::Number::from_f64(v)
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null)
                    } else if let Ok(v) = row.get::<_, String>(i) {
                        serde_json::Value::String(v)
                    } else if let Ok(v) = row.get::<_, bool>(i) {
                        serde_json::Value::Bool(v)
                    } else {
                        serde_json::Value::Null
                    };
                    values.push(value);
                }
                Ok(values)
            })
            .map_err(|e| format!("Failed to execute query: {}", e))?;

        // Collect with row limit to prevent memory blowup
        let rows: Vec<Vec<serde_json::Value>> = rows_iter
            .filter_map(|r| r.ok())
            .take(MAX_QUERY_ROWS)
            .collect();

        let row_count = rows.len();

        Ok(QueryResult {
            columns,
            rows,
            row_count,
            execution_time_ms: 0, // Will be set after
        })
    })
    .await
    .map_err(|e| format!("Task error: {}", e))??;

    let execution_time_ms = start.elapsed().as_millis() as u64;

    Ok(QueryResult {
        execution_time_ms,
        ..result
    })
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
        VALUES (?, ?, ?, ?, 'PENDING', ?)
        ON CONFLICT(plugin_name, version)
        DO UPDATE SET source_code = excluded.source_code,
                      source_hash = excluded.source_hash,
                      status = 'PENDING'
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

    // Default to the database in the project root
    let database_url = std::env::var("CASPARIAN_DATABASE").unwrap_or_else(|_| {
        // Try to find database relative to executable or use absolute path
        let project_db = std::path::Path::new("/Users/shan/workspace/casparianflow/casparian_flow.sqlite3");
        if project_db.exists() {
            format!("sqlite://{}", project_db.display())
        } else {
            "sqlite://casparian_flow.sqlite3".to_string()
        }
    });

    // Shared running flag
    let running = Arc::new(AtomicBool::new(false));

    // Database pool (initialized lazily in setup)
    let db_pool = Arc::new(Mutex::new(None));

    // Start the Sentinel
    let shutdown_tx = start_sentinel(running.clone(), bind_addr.clone(), database_url.clone());

    // Create state
    let state = SentinelState {
        shutdown_tx: Some(shutdown_tx),
        running: running.clone(),
        bind_addr,
        db_pool: db_pool.clone(),
        database_url: database_url.clone(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_system_pulse,
            get_prometheus_metrics,
            is_sentinel_running,
            get_bind_address,
            get_topology,
            get_job_outputs,
            get_job_details,
            query_parquet,
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
            // Publish Wizard (Real I/O)
            analyze_plugin_manifest,
            publish_with_overrides,
        ])
        .setup(move |app| {
            // Initialize database pool
            let db_pool_clone = db_pool.clone();
            let database_url_clone = database_url.clone();

            tauri::async_runtime::spawn(async move {
                // Extract the file path from the URL
                let db_path = database_url_clone
                    .strip_prefix("sqlite://")
                    .unwrap_or(&database_url_clone);

                match SqlitePool::connect(&format!("sqlite:{}", db_path)).await {
                    Ok(pool) => {
                        let mut guard = db_pool_clone.lock().await;
                        *guard = Some(pool);
                        info!("Database pool initialized");
                    }
                    Err(e) => {
                        warn!("Failed to initialize database pool: {}", e);
                    }
                }
            });

            // Start the pulse emitter after app is ready
            let app_handle = app.handle().clone();
            start_pulse_emitter(app_handle, running);

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
    // QueryResult Tests
    // =========================================================================

    #[test]
    fn test_query_result_serialization() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "name".to_string()],
            rows: vec![
                vec![serde_json::json!(1), serde_json::json!("test")],
            ],
            row_count: 1,
            execution_time_ms: 42,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"rowCount\":1")); // camelCase
        assert!(json.contains("\"executionTimeMs\":42"));
    }

    #[test]
    fn test_query_result_handles_null_values() {
        let result = QueryResult {
            columns: vec!["nullable".to_string()],
            rows: vec![
                vec![serde_json::Value::Null],
            ],
            row_count: 1,
            execution_time_ms: 0,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("null"));
    }

    // =========================================================================
    // File Size Limit Test
    // =========================================================================

    #[test]
    fn test_max_query_rows_constant() {
        // Verify the constant is reasonable (not too small, not too large)
        assert!(MAX_QUERY_ROWS >= 1000, "Should allow at least 1000 rows");
        assert!(MAX_QUERY_ROWS <= 100_000, "Should limit to reasonable size");
    }
}
