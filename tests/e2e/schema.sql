-- End-to-End Test Database Schema (SQLite)
-- Minimal schema for testing Worker + Sentinel integration

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
-- Test Data
-- ============================================================================

-- Source root (test data directory)
INSERT INTO cf_source_root (id, path, type, active)
VALUES (1, '/tmp/casparian_test_data', 'local', 1);

-- File hash registry
INSERT INTO cf_file_hash_registry (content_hash, size_bytes)
VALUES ('test_hash_abc123', 100);

-- File location
INSERT INTO cf_file_location (id, source_root_id, rel_path, filename)
VALUES (1, 1, 'test_input.csv', 'test_input.csv');

-- File version
INSERT INTO cf_file_version (id, location_id, content_hash, size_bytes, modified_time)
VALUES (1, 1, 'test_hash_abc123', 100, datetime('now'));

-- Plugin manifest (ACTIVE status)
INSERT INTO cf_plugin_manifest (plugin_name, version, source_code, source_hash, status, env_hash)
VALUES (
    'test_plugin',
    '1.0.0',
    '# Test plugin - bridge mode compatible
import pyarrow as pa

class Handler:
    def execute(self, file_path):
        """Execute plugin and yield Arrow batches."""
        # Create test data
        schema = pa.schema([("id", pa.int64()), ("value", pa.float64())])
        data = {"id": [1, 2, 3], "value": [10.5, 20.3, 30.1]}
        table = pa.Table.from_pydict(data, schema=schema)

        # Yield batch (generator pattern expected by bridge)
        yield table.to_batches()[0]
',
    'test_source_hash_xyz',
    'ACTIVE',
    'test_env_hash_123'
);

-- Topic configuration
INSERT INTO cf_topic_config (plugin_name, topic_name, uri, mode)
VALUES ('test_plugin', 'output', 'parquet://test_output.parquet', 'append');

-- Test job (QUEUED status)
INSERT INTO cf_processing_queue (file_version_id, plugin_name, status, priority)
VALUES (1, 'test_plugin', 'QUEUED', 10);
