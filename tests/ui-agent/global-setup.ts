/**
 * Global Setup - Ephemeral Backend Initialization
 *
 * This runs BEFORE any tests:
 * 1. Creates a fresh SQLite database
 * 2. Applies schema
 * 3. Seeds test data
 * 4. Spawns the bridge server
 */

import { spawn, execSync, ChildProcess } from 'child_process';
import Database from 'better-sqlite3';
import * as fs from 'fs';
import * as path from 'path';

const TEST_DB_PATH = '/tmp/casparian_test.db';
const BRIDGE_PORT = 9999;

// Store bridge process globally so teardown can kill it
let bridgeProcess: ChildProcess | null = null;

/**
 * Kill any process using the bridge port
 */
function killExistingBridge() {
  try {
    // Find and kill any process on port 9999
    const pids = execSync(`lsof -ti:${BRIDGE_PORT} 2>/dev/null || true`).toString().trim();
    if (pids) {
      execSync(`kill -9 ${pids.split('\n').join(' ')} 2>/dev/null || true`);
      console.log(`[Setup] Killed existing process(es) on port ${BRIDGE_PORT}`);
    }
  } catch {
    // No existing process, that's fine
  }
}

/**
 * Schema for test database (matches Rust Sentinel schema)
 */
const SCHEMA = `
-- Routing rules
CREATE TABLE IF NOT EXISTS cf_routing_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern TEXT NOT NULL,
    tag TEXT NOT NULL,
    priority INTEGER DEFAULT 0,
    enabled INTEGER DEFAULT 1,
    description TEXT
);

-- Plugin configuration
CREATE TABLE IF NOT EXISTS cf_plugin_config (
    plugin_name TEXT PRIMARY KEY,
    subscription_tags TEXT,
    default_parameters TEXT
);

-- Topic configuration
CREATE TABLE IF NOT EXISTS cf_topic_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    topic_name TEXT NOT NULL,
    uri TEXT NOT NULL,
    mode TEXT DEFAULT 'write',
    UNIQUE(plugin_name, topic_name)
);

-- Plugin subscriptions
CREATE TABLE IF NOT EXISTS cf_plugin_subscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    topic_name TEXT NOT NULL,
    is_active INTEGER DEFAULT 1
);

-- Processing queue
CREATE TABLE IF NOT EXISTS cf_processing_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    status TEXT DEFAULT 'PENDING',
    result_summary TEXT,
    error_message TEXT,
    claim_time TEXT,
    end_time TEXT,
    retry_count INTEGER DEFAULT 0
);

-- Job logs (cold storage)
CREATE TABLE IF NOT EXISTS cf_job_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL,
    log_text TEXT
);

-- Plugin manifests (matches production schema)
CREATE TABLE IF NOT EXISTS cf_plugin_manifest (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    plugin_name TEXT NOT NULL,
    version TEXT NOT NULL,
    source_code TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    status TEXT DEFAULT 'PENDING',
    signature TEXT,
    validation_error TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    deployed_at TEXT,
    env_hash TEXT,
    artifact_hash TEXT,
    publisher_id INTEGER,
    system_requirements TEXT,
    UNIQUE(plugin_name, version)
);
`;

/**
 * Seed data for testing
 */
const SEED_DATA = `
-- Sample routing rules
INSERT INTO cf_routing_rules (pattern, tag, priority, enabled, description) VALUES
    ('data/sales/*.csv', 'finance', 100, 1, 'Sales CSV files'),
    ('data/marketing/**/*.json', 'marketing', 90, 1, 'Marketing JSON data'),
    ('data/logs/*.log', 'logs', 50, 1, 'Application logs'),
    ('**/*.parquet', 'processed', 80, 0, 'Processed parquet files');

-- Sample plugins
INSERT INTO cf_plugin_config (plugin_name, subscription_tags, default_parameters) VALUES
    ('slow_processor', 'raw_data', '{}'),
    ('data_validator', 'processed', '{}');

-- Sample topic configs
INSERT INTO cf_topic_config (plugin_name, topic_name, uri, mode) VALUES
    ('slow_processor', 'processed_output', 'parquet://output/processed.parquet', 'write'),
    ('data_validator', 'validated_data', 'parquet://output/validated.parquet', 'write');

-- Sample completed jobs
INSERT INTO cf_processing_queue (plugin_name, status, result_summary, end_time, retry_count) VALUES
    ('slow_processor', 'COMPLETED', '/tmp/output.parquet', datetime('now'), 0),
    ('data_validator', 'COMPLETED', '/tmp/validated.parquet', datetime('now'), 0),
    ('broken_plugin', 'FAILED', NULL, datetime('now'), 2);

INSERT INTO cf_job_logs (job_id, log_text) VALUES
    (1, '[INFO] Processing started
[DEBUG] Reading file...
[INFO] Complete'),
    (3, '[ERROR] Plugin crashed: division by zero');
`;

async function setup() {
  console.log('[Setup] Starting ephemeral backend setup...');

  // 0. Kill any existing bridge process
  killExistingBridge();

  // Brief pause to ensure port is released
  await new Promise((r) => setTimeout(r, 100));

  // 1. Remove old test database
  if (fs.existsSync(TEST_DB_PATH)) {
    fs.unlinkSync(TEST_DB_PATH);
    console.log('[Setup] Removed old test database');
  }

  // 2. Create fresh database with schema
  const db = new Database(TEST_DB_PATH);
  db.pragma('journal_mode = WAL');
  db.exec(SCHEMA);
  console.log('[Setup] Created database schema');

  // 3. Seed test data
  db.exec(SEED_DATA);
  console.log('[Setup] Seeded test data');

  // Verify data
  const ruleCount = db.prepare('SELECT COUNT(*) as count FROM cf_routing_rules').get() as { count: number };
  console.log(`[Setup] Verified: ${ruleCount.count} routing rules`);

  db.close();

  // 4. Spawn bridge server using Bun
  const bridgePath = path.join(__dirname, 'bridge', 'server.ts');

  bridgeProcess = spawn('bun', ['run', bridgePath], {
    env: {
      ...process.env,
      CASPARIAN_TEST_DB: TEST_DB_PATH,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  bridgeProcess.stdout?.on('data', (data) => {
    console.log(`[Bridge] ${data.toString().trim()}`);
  });

  bridgeProcess.stderr?.on('data', (data) => {
    console.error(`[Bridge] ${data.toString().trim()}`);
  });

  console.log(`[Setup] Spawned bridge server (PID: ${bridgeProcess.pid})`);

  // 5. Wait for bridge to be ready
  let retries = 20;
  while (retries > 0) {
    try {
      const res = await fetch(`http://localhost:${BRIDGE_PORT}/api/pulse`);
      if (res.ok) {
        console.log('[Setup] Bridge server is ready');
        break;
      }
    } catch {
      // Not ready yet
    }
    await new Promise((r) => setTimeout(r, 250));
    retries--;
  }

  if (retries === 0) {
    throw new Error('Bridge server failed to start');
  }

  console.log('[Setup] Ephemeral backend ready!');
}

async function teardown() {
  console.log('[Teardown] Cleaning up...');

  if (bridgeProcess) {
    bridgeProcess.kill();
    console.log('[Teardown] Killed bridge server');
  }
}

// Export for Playwright
export default async function globalSetup() {
  await setup();

  // Return teardown function
  return async () => {
    await teardown();
  };
}
