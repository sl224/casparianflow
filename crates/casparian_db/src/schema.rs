//! Database schema creation for all Casparian Flow tables.
//!
//! All CREATE TABLE statements live here - single source of truth.

use crate::error::Result;
use crate::CasparianDb;
use tracing::info;

impl CasparianDb {
    /// Ensure all tables exist.
    pub(crate) async fn ensure_schema(&self) -> Result<()> {
        // Enable WAL mode for better concurrent access
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&self.pool)
            .await?;
        sqlx::query("PRAGMA synchronous=NORMAL")
            .execute(&self.pool)
            .await?;
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&self.pool)
            .await?;

        self.create_scout_tables().await?;
        self.create_parser_lab_tables().await?;
        self.create_sentinel_tables().await?;

        info!("Database schema verified");
        Ok(())
    }

    /// Create Scout tables (file discovery & tagging)
    async fn create_scout_tables(&self) -> Result<()> {
        // Sources: filesystem locations to watch
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS scout_sources (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                source_type TEXT NOT NULL,
                path TEXT NOT NULL,
                poll_interval_secs INTEGER NOT NULL DEFAULT 30,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // Tagging Rules: pattern → tag mappings
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS scout_tagging_rules (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                source_id TEXT NOT NULL REFERENCES scout_sources(id),
                pattern TEXT NOT NULL,
                tag TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // Settings: key-value store
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS scout_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // Files: discovered files and their status
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS scout_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id TEXT NOT NULL REFERENCES scout_sources(id),
                path TEXT NOT NULL,
                rel_path TEXT NOT NULL,
                size INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                content_hash TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                tag TEXT,
                tag_source TEXT,
                rule_id TEXT,
                manual_plugin TEXT,
                error TEXT,
                first_seen_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL,
                processed_at INTEGER,
                sentinel_job_id INTEGER,
                UNIQUE(source_id, path)
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // Scout indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_files_source ON scout_files(source_id)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_files_status ON scout_files(status)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_files_tag ON scout_files(tag)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_files_mtime ON scout_files(mtime)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_files_path ON scout_files(path)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tagging_rules_source ON scout_tagging_rules(source_id)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Create Parser Lab tables
    async fn create_parser_lab_tables(&self) -> Result<()> {
        // Parsers
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS parser_lab_parsers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                file_pattern TEXT NOT NULL DEFAULT '',
                pattern_type TEXT DEFAULT 'all',
                source_code TEXT,
                source_hash TEXT,
                validation_status TEXT DEFAULT 'pending',
                validation_error TEXT,
                validation_output TEXT,
                last_validated_at INTEGER,
                messages_json TEXT,
                schema_json TEXT,
                sink_type TEXT DEFAULT 'parquet',
                sink_config_json TEXT,
                published_at INTEGER,
                published_plugin_id INTEGER,
                is_sample INTEGER DEFAULT 0,
                output_mode TEXT DEFAULT 'single',
                detected_topics_json TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // Test files
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS parser_lab_test_files (
                id TEXT PRIMARY KEY,
                parser_id TEXT NOT NULL REFERENCES parser_lab_parsers(id) ON DELETE CASCADE,
                file_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_size INTEGER,
                created_at INTEGER NOT NULL,
                UNIQUE(parser_id, file_path)
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // Parser Lab indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_updated ON parser_lab_parsers(updated_at DESC)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_parser_lab_test_files_parser ON parser_lab_test_files(parser_id)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Create Sentinel tables (job processing)
    async fn create_sentinel_tables(&self) -> Result<()> {
        // Plugin manifest (deployed plugins)
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
        .execute(&self.pool)
        .await?;

        // Plugin config (subscription tags)
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS cf_plugin_config (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                plugin_name TEXT UNIQUE NOT NULL,
                subscription_tags TEXT NOT NULL,
                default_parameters TEXT,
                enabled INTEGER DEFAULT 1
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // Plugin subscriptions
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
        .execute(&self.pool)
        .await?;

        // Topic config (output routing)
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
        .execute(&self.pool)
        .await?;

        // Routing rules
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
        .execute(&self.pool)
        .await?;

        // Processing queue
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
                logs TEXT
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // Job logs
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS cf_job_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id INTEGER NOT NULL,
                log_text TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (job_id) REFERENCES cf_processing_queue(id)
            )"#,
        )
        .execute(&self.pool)
        .await?;

        // File tracking tables
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS cf_source_root (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE
            )"#,
        )
        .execute(&self.pool)
        .await?;

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
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS cf_file_hash_registry (
                content_hash TEXT PRIMARY KEY,
                first_seen TEXT DEFAULT CURRENT_TIMESTAMP,
                size_bytes INTEGER NOT NULL
            )"#,
        )
        .execute(&self.pool)
        .await?;

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
        .execute(&self.pool)
        .await?;

        // Sentinel indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_queue_status ON cf_processing_queue(status)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_queue_plugin ON cf_processing_queue(plugin_name)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_plugin_manifest_name ON cf_plugin_manifest(plugin_name)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
