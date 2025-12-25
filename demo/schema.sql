-- Demo Database Schema (SQLite)
-- For E2E UI Demo with slow_processor plugin

CREATE TABLE IF NOT EXISTS cf_source_root (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    type TEXT DEFAULT 'local',
    active INTEGER DEFAULT 1
);

CREATE TABLE IF NOT EXISTS cf_file_location (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_root_id INTEGER NOT NULL,
    rel_path TEXT NOT NULL,
    filename TEXT NOT NULL,
    last_known_mtime REAL,
    last_known_size INTEGER,
    current_version_id INTEGER,
    discovered_time TEXT DEFAULT CURRENT_TIMESTAMP,
    last_seen_time TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (source_root_id) REFERENCES cf_source_root(id)
);

CREATE TABLE IF NOT EXISTS cf_file_hash_registry (
    content_hash TEXT PRIMARY KEY,
    first_seen TEXT DEFAULT CURRENT_TIMESTAMP,
    size_bytes INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cf_file_version (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    location_id INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    modified_time TEXT NOT NULL,
    detected_at TEXT DEFAULT CURRENT_TIMESTAMP,
    applied_tags TEXT DEFAULT '',
    FOREIGN KEY (location_id) REFERENCES cf_file_location(id),
    FOREIGN KEY (content_hash) REFERENCES cf_file_hash_registry(content_hash)
);

CREATE TABLE IF NOT EXISTS cf_plugin_manifest (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    version TEXT NOT NULL,
    source_code TEXT NOT NULL,
    source_hash TEXT NOT NULL UNIQUE,
    status TEXT DEFAULT 'PENDING',
    signature TEXT,
    validation_error TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    deployed_at TEXT,
    env_hash TEXT,
    artifact_hash TEXT,
    publisher_id INTEGER,
    system_requirements TEXT
);

CREATE TABLE IF NOT EXISTS cf_processing_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_version_id INTEGER NOT NULL,
    plugin_name TEXT NOT NULL,
    config_overrides TEXT,
    status TEXT NOT NULL DEFAULT 'PENDING',
    priority INTEGER DEFAULT 0,
    worker_host TEXT,
    worker_pid INTEGER,
    claim_time TEXT,
    end_time TEXT,
    result_summary TEXT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    FOREIGN KEY (file_version_id) REFERENCES cf_file_version(id)
);

CREATE INDEX IF NOT EXISTS ix_queue_pop ON cf_processing_queue(status, priority, id);

-- Job logs table (COLD storage - separate from hot processing_queue)
-- Only loaded on-demand when user requests job details
-- Uses BLOB to allow future compression (gzip)
CREATE TABLE IF NOT EXISTS cf_job_logs (
    job_id INTEGER PRIMARY KEY,
    log_text BLOB,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(job_id) REFERENCES cf_processing_queue(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cf_topic_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    topic_name TEXT NOT NULL,
    uri TEXT NOT NULL,
    mode TEXT DEFAULT 'append',
    schema_json TEXT
);

CREATE INDEX IF NOT EXISTS ix_topic_lookup ON cf_topic_config(plugin_name, topic_name);

-- Plugin configuration (for topology view)
CREATE TABLE IF NOT EXISTS cf_plugin_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL UNIQUE,
    subscription_tags TEXT DEFAULT '',
    default_parameters TEXT,
    enabled INTEGER DEFAULT 1
);

-- Plugin subscriptions (for topology edges)
CREATE TABLE IF NOT EXISTS cf_plugin_subscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    topic_name TEXT NOT NULL,
    is_active INTEGER DEFAULT 1,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(plugin_name, topic_name)
);

-- Routing Rules: Maps file patterns to tags
-- Example: pattern='data/sales/*.csv' -> tag='finance'
CREATE TABLE IF NOT EXISTS cf_routing_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern TEXT NOT NULL,
    tag TEXT NOT NULL,
    priority INTEGER DEFAULT 0,
    enabled INTEGER DEFAULT 1,
    description TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS ix_routing_priority ON cf_routing_rules(priority DESC, id);

-- ============================================================================
-- Demo Data - Slow Processor Plugin
-- ============================================================================

-- Source root (demo data directory)
INSERT INTO cf_source_root (id, path, type, active)
VALUES (1, 'DEMO_DIR/data', 'local', 1);

-- File hash registry
INSERT INTO cf_file_hash_registry (content_hash, size_bytes)
VALUES ('demo_sample_data_hash', 750);

-- File location
INSERT INTO cf_file_location (id, source_root_id, rel_path, filename)
VALUES (1, 1, 'sample_data.csv', 'sample_data.csv');

-- File version
INSERT INTO cf_file_version (id, location_id, content_hash, size_bytes, modified_time)
VALUES (1, 1, 'demo_sample_data_hash', 750, datetime('now'));

-- Plugin manifest (ACTIVE status) - The slow processor
INSERT INTO cf_plugin_manifest (plugin_name, version, source_code, source_hash, status, env_hash)
VALUES (
    'slow_processor',
    '1.0.0',
    '"""Slow Processor - Demo plugin that processes files with delays."""
import time
import pandas as pd

class Handler:
    def configure(self, context, config):
        self.context = context
        self.handle = context.register_topic("processed_output")

    def execute(self, file_path: str):
        batch_size = 5
        delay_seconds = 1.5
        df = pd.read_csv(file_path)
        total_rows = len(df)
        batch_number = 0

        for i in range(0, total_rows, batch_size):
            batch_number += 1
            batch_df = df.iloc[i:i + batch_size].copy()
            time.sleep(delay_seconds)
            batch_df["_batch"] = batch_number
            batch_df["_processed_at"] = pd.Timestamp.now().isoformat()
            self.context.publish(self.handle, batch_df)
            print(f"[slow_processor] Batch {batch_number} done, {len(batch_df)} rows")

        print(f"[slow_processor] Complete: {total_rows} rows in {batch_number} batches")
        return None
',
    'slow_processor_demo_hash_v1',
    'ACTIVE',
    'demo_env_hash'
);

-- Topic configurations (multiple for demo visibility)
INSERT INTO cf_topic_config (plugin_name, topic_name, uri, mode)
VALUES
    ('slow_processor', 'processed_output', 'parquet://DEMO_DIR/output/processed_output.parquet', 'write'),
    ('data_validator', 'validated_data', 'parquet://DEMO_DIR/output/validated.parquet', 'write'),
    ('data_validator', 'errors', 'parquet://DEMO_DIR/output/errors.parquet', 'write');

-- Plugin configurations (for topology view)
INSERT INTO cf_plugin_config (plugin_name, subscription_tags, default_parameters)
VALUES
    ('slow_processor', 'demo,csv', '{"batch_size": 5, "delay": 1.5}'),
    ('data_validator', 'demo', '{"strict": true}');

-- Plugin subscriptions (creates topic -> plugin edges in topology)
INSERT INTO cf_plugin_subscriptions (plugin_name, topic_name, is_active)
VALUES
    ('data_validator', 'slow_processor:processed_output', 1);

-- Pre-completed jobs with output files (for immediate Data tab testing)
-- These get IDs 1-4 automatically
INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority, end_time, result_summary)
VALUES
    (1, 'slow_processor', 'COMPLETED', 0, datetime('now', '-5 minutes'), 'DEMO_DIR/output/processed_output.parquet'),
    (1, 'data_validator', 'COMPLETED', 0, datetime('now', '-3 minutes'), 'DEMO_DIR/output/validated.parquet'),
    (1, 'data_validator', 'COMPLETED', 0, datetime('now', '-2 minutes'), 'DEMO_DIR/output/errors.parquet'),
    (1, 'simple_transform', 'COMPLETED', 0, datetime('now', '-1 minutes'), 'DEMO_DIR/output/mixed_types.parquet');

-- Pre-failed job for testing error display (ID 5)
INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority, end_time, error_message)
VALUES
    (1, 'broken_plugin', 'FAILED', 0, datetime('now', '-30 seconds'), 'ModuleNotFoundError: No module named ''pandas''');

-- Queue jobs for processing (IDs 6, 7, 8 - will be processed by the worker)
INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority)
VALUES
    (1, 'slow_processor', 'QUEUED', 10),
    (1, 'slow_processor', 'QUEUED', 5),
    (1, 'slow_processor', 'QUEUED', 1);

-- Sample logs for demo jobs (Glass Box feature)
-- Job IDs: 1=slow_processor, 2=data_validator, 5=broken_plugin
INSERT INTO cf_job_logs (job_id, log_text) VALUES
(1, '[INFO] Plugin execution started
[STDOUT] Processing file: sample_data.csv
[STDOUT] Reading 25 rows...
[DEBUG] Batch 1: 5 rows processed
[DEBUG] Batch 2: 5 rows processed
[DEBUG] Batch 3: 5 rows processed
[DEBUG] Batch 4: 5 rows processed
[DEBUG] Batch 5: 5 rows processed
[INFO] Total rows processed: 25
[INFO] Plugin execution completed: {''rows_published'': 25, ''status'': ''SUCCESS''}'),
(2, '[INFO] Plugin execution started
[STDOUT] Validating data from processed_output.parquet
[INFO] Schema validation passed
[INFO] Data quality checks passed
[INFO] Plugin execution completed: {''rows_published'': 25, ''status'': ''SUCCESS''}'),
(5, '[INFO] Plugin execution started
[STDERR] WARNING: Missing optional dependency
[ERROR] Plugin execution failed: ModuleNotFoundError: No module named ''pandas''
Traceback (most recent call last):
  File "bridge_shim.py", line 194, in execute_plugin
    exec(source_code, plugin_namespace)
  File "<bridge>", line 1, in <module>
    import pandas as pd
ModuleNotFoundError: No module named ''pandas''');

-- Demo Routing Rules (file pattern -> tag mapping)
INSERT INTO cf_routing_rules (pattern, tag, priority, enabled, description) VALUES
    ('data/sales/*.csv', 'finance', 100, 1, 'Sales CSV files go to finance pipeline'),
    ('data/sales/*.xlsx', 'finance', 100, 1, 'Sales Excel files go to finance pipeline'),
    ('data/marketing/**/*.json', 'marketing', 90, 1, 'Marketing JSON data'),
    ('data/logs/*.log', 'logs', 50, 1, 'Application logs for analysis'),
    ('data/raw/**/*', 'raw', 10, 1, 'Catch-all for raw data ingestion'),
    ('**/*.parquet', 'processed', 80, 1, 'Already processed parquet files'),
    ('data/test/*', 'test', 200, 0, 'Test data (disabled)');
