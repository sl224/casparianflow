//! Database schema initialization for Sentinel tables
//!
//! All CREATE TABLE statements live here - pure DDL, no business logic.

use sqlx::SqlitePool;
use tracing::info;

/// Create Sentinel database tables if they don't exist
pub async fn create_sentinel_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
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

    info!("Sentinel database tables created/verified");
    Ok(())
}
