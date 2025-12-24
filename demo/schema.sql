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

CREATE TABLE IF NOT EXISTS cf_topic_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    topic_name TEXT NOT NULL,
    uri TEXT NOT NULL,
    mode TEXT DEFAULT 'append',
    schema_json TEXT
);

CREATE INDEX IF NOT EXISTS ix_topic_lookup ON cf_topic_config(plugin_name, topic_name);

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
    '# See demo/plugins/slow_processor.py for full source
# This is a placeholder - actual code loaded from file system
import time
import pandas as pd
import pyarrow as pa
from casparian_flow.sdk import BasePlugin, PluginMetadata

MANIFEST = PluginMetadata(
    pattern="demo/data/*.csv",
    topic="processed_output",
)

class Handler(BasePlugin):
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
            yield pa.Table.from_pandas(batch_df)
',
    'slow_processor_demo_hash_v1',
    'ACTIVE',
    'demo_env_hash'
);

-- Topic configuration
INSERT INTO cf_topic_config (plugin_name, topic_name, uri, mode)
VALUES ('slow_processor', 'processed_output', 'parquet://demo_output.parquet', 'append');

-- Queue multiple jobs for demo visibility (3 jobs = ~18 seconds total processing)
INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority)
VALUES
    (1, 'slow_processor', 'QUEUED', 10),
    (1, 'slow_processor', 'QUEUED', 5),
    (1, 'slow_processor', 'QUEUED', 1);
