/**
 * Test Bridge Server for Playwright E2E Tests
 *
 * This server exposes Tauri commands via HTTP so Playwright can test
 * the full application flow without needing Tauri's IPC.
 *
 * Usage:
 *   bun run scripts/test-bridge.ts
 *
 * Then run Playwright tests:
 *   bun run test:e2e
 *
 * The bridge connects to the same SQLite database as the Tauri app
 * and implements the key commands needed for E2E testing.
 */

import { Database } from 'bun:sqlite';
import { existsSync, mkdirSync, readFileSync, writeFileSync, readdirSync, statSync } from 'fs';
import { join, dirname, basename } from 'path';
import { homedir } from 'os';
import { spawn } from 'child_process';
import { createHash } from 'crypto';

// Configuration
const PORT = 9999;
const HOME = homedir();
const CF_DIR = join(HOME, '.casparian_flow');
// SINGLE DATABASE - all tables in one file
const DB_PATH = join(CF_DIR, 'casparian_flow.sqlite3');
const SAMPLES_DIR = join(CF_DIR, 'samples');
const PARSERS_DIR = join(CF_DIR, 'parsers');

// Ensure directories exist
mkdirSync(CF_DIR, { recursive: true });
mkdirSync(SAMPLES_DIR, { recursive: true });
mkdirSync(PARSERS_DIR, { recursive: true });

// Single database for everything
let db: Database;

function initDb() {
  // SINGLE DATABASE - all tables (Parser Lab, Scout, Sentinel)
  db = new Database(DB_PATH, { create: true });

  // ========================================================================
  // PARSER LAB TABLES (parser_lab_*)
  // ========================================================================

  // Create tables if not exist (matching scout.rs schema)
  db.run(`
    CREATE TABLE IF NOT EXISTS parser_lab_parsers (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      file_pattern TEXT NOT NULL DEFAULT '',
      pattern_type TEXT DEFAULT 'all',
      source_code TEXT,
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
    )
  `);

  db.run(`
    CREATE TABLE IF NOT EXISTS parser_lab_test_files (
      id TEXT PRIMARY KEY,
      parser_id TEXT NOT NULL,
      file_path TEXT NOT NULL,
      file_name TEXT NOT NULL,
      file_size INTEGER,
      created_at INTEGER NOT NULL,
      UNIQUE(parser_id, file_path)
    )
  `);

  // ========================================================================
  // SENTINEL TABLES (cf_*)
  // Must match lib.rs create_tables() exactly!
  // ========================================================================

  // Plugin manifest
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_plugin_manifest (
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
    )
  `);

  // Plugin config (subscription tags)
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_plugin_config (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      plugin_name TEXT UNIQUE NOT NULL,
      subscription_tags TEXT NOT NULL,
      default_parameters TEXT,
      enabled INTEGER DEFAULT 1
    )
  `);

  // Topic config (output routing)
  // NOTE: schema_json is required by casparian_sentinel TopicConfig model
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_topic_config (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      plugin_name TEXT NOT NULL,
      topic_name TEXT NOT NULL,
      uri TEXT NOT NULL,
      mode TEXT DEFAULT 'write',
      sink_type TEXT DEFAULT 'parquet',
      schema_json TEXT,
      enabled INTEGER DEFAULT 1,
      UNIQUE(plugin_name, topic_name)
    )
  `);

  // Processing queue (for job submission)
  // NOTE: worker_host is required by casparian_sentinel QueueRow model
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_processing_queue (
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
      worker_host TEXT,
      worker_pid INTEGER,
      logs TEXT
    )
  `);

  // Job logs
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_job_logs (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      job_id INTEGER NOT NULL,
      log_text TEXT,
      created_at TEXT DEFAULT CURRENT_TIMESTAMP
    )
  `);

  // Source root (for file tracking)
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_source_root (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      path TEXT NOT NULL UNIQUE
    )
  `);

  // File location (for file tracking)
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_file_location (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      source_root_id INTEGER NOT NULL,
      rel_path TEXT NOT NULL,
      filename TEXT NOT NULL,
      last_known_mtime REAL,
      last_known_size INTEGER,
      current_version_id INTEGER,
      last_seen_time TEXT DEFAULT CURRENT_TIMESTAMP,
      FOREIGN KEY (source_root_id) REFERENCES cf_source_root(id)
    )
  `);

  // File hash registry - MUST match lib.rs lines 1727-1731
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_file_hash_registry (
      content_hash TEXT PRIMARY KEY,
      first_seen TEXT DEFAULT CURRENT_TIMESTAMP,
      size_bytes INTEGER NOT NULL
    )
  `);

  // File version (for job processing)
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_file_version (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      location_id INTEGER NOT NULL,
      content_hash TEXT NOT NULL,
      size_bytes INTEGER NOT NULL,
      modified_time TEXT,
      applied_tags TEXT DEFAULT '',
      FOREIGN KEY (location_id) REFERENCES cf_file_location(id),
      FOREIGN KEY (content_hash) REFERENCES cf_file_hash_registry(content_hash)
    )
  `);

  // Plugin subscriptions - MUST match lib.rs lines 1599-1606
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_plugin_subscriptions (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      plugin_name TEXT NOT NULL,
      topic_name TEXT NOT NULL,
      is_active INTEGER DEFAULT 1,
      created_at TEXT DEFAULT CURRENT_TIMESTAMP,
      UNIQUE(plugin_name, topic_name)
    )
  `);

  // Routing rules - MUST match lib.rs lines 1643-1650
  db.run(`
    CREATE TABLE IF NOT EXISTS cf_routing_rules (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      pattern TEXT NOT NULL,
      tag TEXT NOT NULL,
      priority INTEGER DEFAULT 0,
      enabled INTEGER DEFAULT 1,
      description TEXT
    )
  `);

  // Add sink_type column if missing (migration)
  try {
    db.run(`ALTER TABLE cf_topic_config ADD COLUMN sink_type TEXT DEFAULT 'parquet'`);
  } catch {
    // Column already exists
  }

  // ========================================================================
  // Scout tables - MUST match crates/casparian_scout/src/db.rs exactly!
  // ========================================================================

  // Sources: filesystem locations to watch
  db.run(`
    CREATE TABLE IF NOT EXISTS scout_sources (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL UNIQUE,
      source_type TEXT NOT NULL,
      path TEXT NOT NULL,
      poll_interval_secs INTEGER NOT NULL DEFAULT 30,
      enabled INTEGER NOT NULL DEFAULT 1,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    )
  `);

  // Tagging Rules: pattern → tag mappings
  db.run(`
    CREATE TABLE IF NOT EXISTS scout_tagging_rules (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL UNIQUE,
      source_id TEXT NOT NULL REFERENCES scout_sources(id),
      pattern TEXT NOT NULL,
      tag TEXT NOT NULL,
      priority INTEGER NOT NULL DEFAULT 0,
      enabled INTEGER NOT NULL DEFAULT 1,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    )
  `);

  // Settings: key-value store
  db.run(`
    CREATE TABLE IF NOT EXISTS scout_settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    )
  `);

  // Schema migrations tracking
  db.run(`
    CREATE TABLE IF NOT EXISTS schema_migrations (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL UNIQUE,
      applied_at INTEGER NOT NULL
    )
  `);

  // Files: discovered files and their status
  // Matches Rust schema exactly (including all migration columns)
  db.run(`
    CREATE TABLE IF NOT EXISTS scout_files (
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
    )
  `);

  // Splitter tables (matches Rust MIGRATION_ADD_SPLITTER_TABLES)
  db.run(`
    CREATE TABLE IF NOT EXISTS splitter_sessions (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      source_file_path TEXT NOT NULL,
      output_dir TEXT,
      col_index INTEGER,
      delimiter TEXT DEFAULT ',',
      has_header INTEGER DEFAULT 1,
      analysis_messages_json TEXT,
      analysis_result_json TEXT,
      full_analysis_json TEXT,
      shred_result_json TEXT,
      status TEXT NOT NULL DEFAULT 'new',
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    )
  `);

  db.run(`
    CREATE TABLE IF NOT EXISTS splitter_parser_drafts (
      id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL REFERENCES splitter_sessions(id) ON DELETE CASCADE,
      shard_key TEXT NOT NULL,
      shard_path TEXT NOT NULL,
      current_code TEXT,
      validation_status TEXT DEFAULT 'pending',
      validation_error TEXT,
      validation_output TEXT,
      messages_json TEXT,
      schema_json TEXT,
      sink_type TEXT DEFAULT 'parquet',
      output_path TEXT,
      phase TEXT DEFAULT 'refining',
      published_plugin_id INTEGER,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL,
      UNIQUE(session_id, shard_key)
    )
  `);

  // Create indexes (matches Rust SCHEMA_INDEXES)
  db.run(`CREATE INDEX IF NOT EXISTS idx_files_source ON scout_files(source_id)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_files_status ON scout_files(status)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_files_tag ON scout_files(tag)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_files_mtime ON scout_files(mtime)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_files_path ON scout_files(path)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_files_last_seen ON scout_files(last_seen_at)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_files_tag_source ON scout_files(tag_source)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_files_manual_plugin ON scout_files(manual_plugin)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_tagging_rules_source ON scout_tagging_rules(source_id)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_tagging_rules_priority ON scout_tagging_rules(priority DESC)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_splitter_sessions_status ON splitter_sessions(status)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_splitter_sessions_updated ON splitter_sessions(updated_at DESC)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_splitter_parser_drafts_session ON splitter_parser_drafts(session_id)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_splitter_parser_drafts_status ON splitter_parser_drafts(validation_status)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_updated ON parser_lab_parsers(updated_at DESC)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_status ON parser_lab_parsers(validation_status)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_parser_lab_parsers_pattern ON parser_lab_parsers(file_pattern)`);
  db.run(`CREATE INDEX IF NOT EXISTS idx_parser_lab_test_files_parser ON parser_lab_test_files(parser_id)`);

  console.log('[Bridge] Database initialized at', DB_PATH);
}

// UUID generator
function uuid(): string {
  return crypto.randomUUID();
}

// Command implementations
const commands: Record<string, (args: any) => any> = {
  // Schema inspection (for compatibility tests)
  get_table_columns: ({ tableName }: { tableName: string }) => {
    const rows = db.query(`PRAGMA table_info(${tableName})`).all();
    return rows;
  },

  get_indexes: () => {
    // Get all indexes from the database
    const indexes = db.query(`SELECT name FROM sqlite_master WHERE type='index'`).all() as { name: string }[];
    return indexes.map(r => r.name);
  },

  // Create a job in the processing queue (for integration testing)
  create_processing_job: ({ pluginName, inputFile }: { pluginName: string; inputFile: string }) => {
    const result = db.run(`
      INSERT INTO cf_processing_queue (plugin_name, input_file, status)
      VALUES (?, ?, 'QUEUED')
    `, [pluginName, inputFile]);
    return { jobId: result.lastInsertRowid };
  },

  // Get job status from processing queue
  get_job_status: ({ jobId }: { jobId: number }) => {
    const row = db.query(`
      SELECT id, plugin_name, input_file, status, error_message, result_summary
      FROM cf_processing_queue WHERE id = ?
    `).get(jobId) as any;
    return row || null;
  },

  // Update job status (for testing job completion)
  update_job_status: ({ jobId, status, errorMessage, resultSummary }: {
    jobId: number;
    status: string;
    errorMessage?: string;
    resultSummary?: string
  }) => {
    db.run(`
      UPDATE cf_processing_queue
      SET status = ?, error_message = ?, result_summary = ?, end_time = datetime('now')
      WHERE id = ?
    `, [status, errorMessage || null, resultSummary || null, jobId]);
    return true;
  },

  // Cancel a job (matches Tauri cancel_job command)
  cancel_job: ({ jobId }: { jobId: number }) => {
    const result = db.run(`
      UPDATE cf_processing_queue
      SET status = 'CANCELLED',
          error_message = 'Cancelled by user',
          end_time = datetime('now')
      WHERE id = ? AND status IN ('RUNNING', 'QUEUED')
    `, [jobId]);

    if (result.changes > 0) {
      return `Job ${jobId} cancelled`;
    } else {
      throw new Error(`Job ${jobId} not found or not in cancellable state`);
    }
  },

  // Simulate stale worker cleanup (marks jobs as FAILED when worker goes stale)
  simulate_stale_worker: ({ jobId }: { jobId: number }) => {
    const result = db.run(`
      UPDATE cf_processing_queue
      SET status = 'FAILED',
          error_message = 'Worker became unresponsive (stale heartbeat)',
          end_time = datetime('now')
      WHERE id = ? AND status = 'RUNNING'
    `, [jobId]);

    if (result.changes > 0) {
      return `Job ${jobId} marked as failed due to stale worker`;
    } else {
      throw new Error(`Job ${jobId} not found or not running`);
    }
  },

  // Clean up test jobs (for E2E test cleanup)
  cleanup_test_jobs: () => {
    const result = db.run(`
      DELETE FROM cf_processing_queue WHERE plugin_name LIKE 'test_%'
    `);
    console.log(`[Bridge] Cleaned up ${result.changes} test jobs`);
    return { deleted: result.changes };
  },

  // Validate plugin is processable (simulates main.rs:801 query)
  // This is the EXACT query the job processor uses
  validate_plugin_processable: ({ pluginName }: { pluginName: string }) => {
    const row = db.query(`
      SELECT plugin_name, status, source_code
      FROM cf_plugin_manifest
      WHERE plugin_name = ? AND status IN ('ACTIVE', 'DEPLOYED')
      ORDER BY deployed_at DESC LIMIT 1
    `).get(pluginName) as { plugin_name: string; status: string; source_code: string } | null;

    if (!row) {
      // Check if plugin exists with wrong status
      const anyRow = db.query(
        'SELECT plugin_name, status FROM cf_plugin_manifest WHERE plugin_name = ?'
      ).get(pluginName) as { plugin_name: string; status: string } | null;

      if (anyRow) {
        return {
          processable: false,
          reason: `Plugin exists with status '${anyRow.status}' but job processor needs 'ACTIVE' or 'DEPLOYED'`,
          actualStatus: anyRow.status,
        };
      }
      return { processable: false, reason: "Plugin not found in database" };
    }
    return { processable: true, status: row.status };
  },

  // Get deployed plugin details
  get_deployed_plugin: ({ name }: { name: string }) => {
    return db.query(`
      SELECT plugin_name, version, status, deployed_at, source_hash
      FROM cf_plugin_manifest WHERE plugin_name = ?
      ORDER BY deployed_at DESC LIMIT 1
    `).get(name);
  },

  // List all plugins with their status
  list_all_plugins: () => {
    return db.query(`
      SELECT plugin_name, version, status, deployed_at
      FROM cf_plugin_manifest
      ORDER BY deployed_at DESC
    `).all();
  },

  // Scout init
  scout_init_db: () => {
    initDb();
    return true;
  },

  // Parser Lab commands
  parser_lab_load_sample: () => {
    // Delete any existing samples with broken code and create fresh one
    // This ensures we always have a working sample
    const existingSamples = db.query('SELECT id FROM parser_lab_parsers WHERE is_sample = 1').all() as { id: string }[];
    for (const sample of existingSamples) {
      db.run('DELETE FROM parser_lab_test_files WHERE parser_id = ?', [sample.id]);
      db.run('DELETE FROM parser_lab_parsers WHERE id = ?', [sample.id]);
    }

    const now = Date.now();
    const id = uuid();

    // Create sample CSV (matches Rust version in scout.rs)
    const samplePath = join(SAMPLES_DIR, 'transactions.csv');
    if (!existsSync(samplePath)) {
      const csv = `id,type,date,amount,category,description
1,SALE,2024-01-15,150.00,electronics,Wireless headphones
2,REFUND,2024-01-16,-25.00,electronics,Defective charger return
3,SALE,2024-01-16,89.99,books,Programming textbook
4,SALE,2024-01-17,34.50,office,Notebook set
5,SALE,2024-01-18,299.00,electronics,Bluetooth speaker
6,REFUND,2024-01-19,-89.99,books,Wrong edition
7,SALE,2024-01-20,45.00,clothing,Cotton t-shirt
8,SALE,2024-01-21,120.00,electronics,USB hub
9,SALE,2024-01-22,15.99,office,Pen pack
10,SALE,2024-01-23,599.00,electronics,Mechanical keyboard`;
      writeFileSync(samplePath, csv);
    }

    // Sample parser code (matches Rust version in scout.rs)
    const sampleCode = `import polars as pl

TOPIC = "transactions"
SINK = "parquet"

def parse(input_path: str) -> pl.DataFrame:
    """
    Parse transaction records into a clean DataFrame.

    This sample parser demonstrates:
    - Reading CSV files with polars
    - Type conversions (date, float)
    - Basic data cleaning
    """
    df = pl.read_csv(input_path)

    # Convert types
    df = df.with_columns([
        pl.col("date").str.strptime(pl.Date, "%Y-%m-%d"),
        pl.col("amount").cast(pl.Float64),
    ])

    return df`;

    // Insert parser
    db.run(`
      INSERT INTO parser_lab_parsers
        (id, name, file_pattern, pattern_type, source_code, is_sample, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, 1, ?, ?)
    `, [id, 'Sample Parser', '', 'all', sampleCode, now, now]);

    // Add sample file as test file
    const testFileId = uuid();
    const stats = statSync(samplePath);
    db.run(`
      INSERT INTO parser_lab_test_files
        (id, parser_id, file_path, file_name, file_size, created_at)
      VALUES (?, ?, ?, ?, ?, ?)
    `, [testFileId, id, samplePath, 'transactions.csv', stats.size, now]);

    return {
      id,
      name: 'Sample Parser',
      filePattern: '',
      patternType: 'all',
      sourceCode: sampleCode,
      validationStatus: 'pending',
      validationError: null,
      validationOutput: null,
      lastValidatedAt: null,
      messagesJson: null,
      schemaJson: null,
      sinkType: 'parquet',
      sinkConfigJson: null,
      publishedAt: null,
      publishedPluginId: null,
      isSample: true,
      outputMode: 'single',
      detectedTopicsJson: null,
      createdAt: now,
      updatedAt: now,
    };
  },

  parser_lab_get_parser: ({ parserId }: { parserId: string }) => {
    const row = db.query(`SELECT * FROM parser_lab_parsers WHERE id = ?`).get(parserId) as any;
    if (!row) return null;

    return {
      id: row.id,
      name: row.name,
      filePattern: row.file_pattern,
      patternType: row.pattern_type,
      sourceCode: row.source_code,
      validationStatus: row.validation_status,
      validationError: row.validation_error,
      validationOutput: row.validation_output,
      lastValidatedAt: row.last_validated_at,
      messagesJson: row.messages_json,
      schemaJson: row.schema_json,
      sinkType: row.sink_type,
      sinkConfigJson: row.sink_config_json,
      publishedAt: row.published_at,
      publishedPluginId: row.published_plugin_id,
      isSample: Boolean(row.is_sample),
      outputMode: row.output_mode,
      detectedTopicsJson: row.detected_topics_json,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    };
  },

  parser_lab_list_parsers: ({ limit = 50 }: { limit?: number }) => {
    const rows = db.query(`
      SELECT id, name, file_pattern, validation_status, is_sample, output_mode, updated_at
      FROM parser_lab_parsers
      ORDER BY updated_at DESC
      LIMIT ?
    `).all(limit) as any[];

    return rows.map(row => ({
      id: row.id,
      name: row.name,
      filePattern: row.file_pattern,
      validationStatus: row.validation_status,
      isSample: Boolean(row.is_sample),
      outputMode: row.output_mode,
      updatedAt: row.updated_at,
    }));
  },

  parser_lab_list_test_files: ({ parserId }: { parserId: string }) => {
    const rows = db.query(`
      SELECT * FROM parser_lab_test_files WHERE parser_id = ?
    `).all(parserId) as any[];

    return rows.map(row => ({
      id: row.id,
      parserId: row.parser_id,
      filePath: row.file_path,
      fileName: row.file_name,
      fileSize: row.file_size,
      createdAt: row.created_at,
    }));
  },

  parser_lab_add_test_file: ({ parserId, filePath }: { parserId: string; filePath: string }) => {
    const id = uuid();
    const now = Date.now();
    const fileName = basename(filePath);
    let fileSize = null;

    if (existsSync(filePath)) {
      fileSize = statSync(filePath).size;
    }

    db.run(`
      INSERT OR REPLACE INTO parser_lab_test_files
        (id, parser_id, file_path, file_name, file_size, created_at)
      VALUES (?, ?, ?, ?, ?, ?)
    `, [id, parserId, filePath, fileName, fileSize, now]);

    return {
      id,
      parserId,
      filePath,
      fileName,
      fileSize,
      createdAt: now,
    };
  },

  parser_lab_update_parser: ({ parser }: { parser: any }) => {
    const now = Date.now();
    db.run(`
      UPDATE parser_lab_parsers SET
        name = ?,
        file_pattern = ?,
        pattern_type = ?,
        source_code = ?,
        sink_type = ?,
        sink_config_json = ?,
        output_mode = ?,
        updated_at = ?
      WHERE id = ?
    `, [
      parser.name,
      parser.filePattern,
      parser.patternType,
      parser.sourceCode,
      parser.sinkType,
      parser.sinkConfigJson,
      parser.outputMode || 'single',
      now,
      parser.id,
    ]);
    return true;
  },

  parser_lab_validate_parser: async ({ parserId, testFileId }: { parserId: string; testFileId: string }) => {
    // Get parser and test file
    const parser = commands.parser_lab_get_parser({ parserId });
    if (!parser) throw new Error('Parser not found');

    const testFiles = commands.parser_lab_list_test_files({ parserId });
    const testFile = testFiles.find((f: any) => f.id === testFileId);
    if (!testFile) throw new Error('Test file not found');

    // Run Python validation
    const result = await runPythonValidation(parser.sourceCode, testFile.filePath);

    const now = Date.now();
    db.run(`
      UPDATE parser_lab_parsers SET
        validation_status = ?,
        validation_error = ?,
        validation_output = ?,
        output_mode = ?,
        detected_topics_json = ?,
        last_validated_at = ?,
        updated_at = ?
      WHERE id = ?
    `, [
      result.status,
      result.error,
      result.output,
      result.outputMode,
      result.detectedTopics ? JSON.stringify(result.detectedTopics) : null,
      now,
      now,
      parserId,
    ]);

    return commands.parser_lab_get_parser({ parserId });
  },

  preview_shard: ({ path: filePath, numLines = 30 }: { path: string; numLines?: number }) => {
    if (!existsSync(filePath)) {
      return [`File not found: ${filePath}`];
    }

    const content = readFileSync(filePath, 'utf-8');
    const lines = content.split('\n').slice(0, numLines);
    return lines;
  },

  // AI Chat (stub - returns helpful message)
  parser_lab_chat: async ({ filePreview, currentCode, userMessage }: { filePreview: string; currentCode: string; userMessage: string }) => {
    // In real implementation, this would call an LLM
    // For testing, detect data format and return appropriate parser

    // Detect MCDATA format (has ACT_, RFC_, PFC_, CONFIG_ record types)
    if (filePreview && (
      filePreview.includes('ACT_') ||
      filePreview.includes('RFC_') ||
      filePreview.includes('PFC_') ||
      filePreview.includes('CONFIG_')
    )) {
      return `I analyzed your MCDATA file. This is a multi-record format with different record types. Here's a DEMUX parser:

\`\`\`python
import polars as pl
from casparian_types import Output

TOPIC = "mcdata"

def parse(input_path: str) -> list[Output]:
    """Parse MCDATA file with multiple record types."""
    tables: dict[str, list] = {}

    with open(input_path, 'r') as f:
        for line in f:
            parts = line.strip().split(',')
            if len(parts) < 2:
                continue

            # Extract record type from column 2 (format: "ACT_FCNS1_SW:")
            record_type = parts[1].rstrip(':').strip() if len(parts) > 1 else 'unknown'
            if not record_type:
                continue

            # Simplify record type for table name
            if record_type.startswith('ACT_'):
                table_name = 'act_records'
            elif record_type.startswith('RFC_'):
                table_name = 'rfc_faults'
            elif record_type.startswith('PFC_'):
                table_name = 'pfc_faults'
            elif record_type.startswith('CONFIG_'):
                table_name = 'config_data'
            else:
                table_name = 'other_records'

            if table_name not in tables:
                tables[table_name] = []
            tables[table_name].append({
                'record_type': record_type,
                'raw_data': line.strip()
            })

    # Convert to Outputs with appropriate sink types
    outputs = []
    for name, rows in tables.items():
        if rows:
            df = pl.DataFrame(rows)
            # Faults go to SQLite for querying, others to parquet
            sink = "sqlite" if name.endswith('_faults') else "parquet"
            outputs.append(Output(name, df, sink))

    return outputs
\`\`\`

This DEMUX parser groups records by type into separate outputs. RFC and PFC faults go to SQLite, other records to Parquet.`;
    }

    // Default CSV parser
    return `I analyzed your data. Here's a suggested parser:

\`\`\`python
import polars as pl

TOPIC = "data"
SINK = "parquet"

def parse(input_path: str) -> pl.DataFrame:
    """Parse the data file."""
    df = pl.read_csv(input_path)
    return df
\`\`\`

This is a basic parser. Adjust as needed for your specific data format.`;
  },

  // Validate subscription tag for uniqueness
  validate_subscription_tag: ({ tag, currentParserId }: { tag: string; currentParserId?: string }) => {
    // Basic format validation - alphanumeric, underscore, hyphen, dot only
    const validFormat = /^[a-zA-Z0-9_\-\.]+$/.test(tag);
    if (!validFormat) {
      return { valid: false, exists: false, existingPluginName: null };
    }

    // Check database for existing plugins with this tag (exact match, no prefix)
    const existing = db.query(`
      SELECT plugin_name FROM cf_plugin_config WHERE subscription_tags = ? LIMIT 1
    `).get(tag) as { plugin_name: string } | null;

    if (existing) {
      return {
        valid: true,
        exists: true,
        existingPluginName: existing.plugin_name
      };
    }

    return { valid: true, exists: false, existingPluginName: null };
  },

  // Publish parser
  publish_parser: ({ parserKey, sourceCode, sinkType, outputPath, outputMode, topicUrisJson, version }: any) => {
    // Save parser to file
    const filename = `${parserKey.replace(/[^a-zA-Z0-9_]/g, '_')}.py`;
    const parserPath = join(PARSERS_DIR, filename);
    writeFileSync(parserPath, sourceCode);

    // Calculate hash
    const hash = createHash('sha256').update(sourceCode).digest('hex');
    const pluginName = parserKey;
    // Tags are stored as-is, no prefix
    const subscriptionTag = parserKey;
    // Use provided version or default to 1.0.0
    const pluginVersion = version || '1.0.0';

    // Insert into cf_plugin_manifest (Sentinel DB)
    db.run(`
      INSERT OR REPLACE INTO cf_plugin_manifest
        (plugin_name, version, source_code, source_hash, status, deployed_at)
      VALUES (?, ?, ?, ?, 'ACTIVE', datetime('now'))
    `, [pluginName, pluginVersion, sourceCode, hash]);

    // Insert into cf_plugin_config with subscription_tags (Sentinel DB)
    db.run(`
      INSERT OR REPLACE INTO cf_plugin_config
        (plugin_name, subscription_tags, enabled)
      VALUES (?, ?, 1)
    `, [pluginName, subscriptionTag]);

    // Insert into cf_topic_config for output routing
    console.log(`[Bridge] publish_parser outputMode=${outputMode}, topicUrisJson=${topicUrisJson ? 'present' : 'null'}`);
    if (outputMode === 'multi' && topicUrisJson) {
      try {
        const topicUris = JSON.parse(topicUrisJson);
        console.log('[Bridge] topicUris:', JSON.stringify(topicUris, null, 2));
        for (const [topicName, value] of Object.entries(topicUris)) {
          // Handle both old format (string URI) and new format (TopicSink object)
          let uri: string;
          let topicSinkType: string;
          if (typeof value === 'string') {
            uri = value;
            // Extract sink type from URI prefix
            topicSinkType = uri.startsWith('sqlite://') ? 'sqlite' :
                            uri.startsWith('csv://') ? 'csv' : 'parquet';
          } else if (typeof value === 'object' && value !== null) {
            // New format: { type: 'parquet', uri: '...', config: {} }
            const sink = value as { type?: string; uri?: string; path?: string };
            topicSinkType = sink.type || 'parquet';
            // Use uri if provided, otherwise construct from type + path
            uri = sink.uri || `${topicSinkType}://${sink.path || `~/.casparian_flow/output/${topicName}/`}`;
            console.log(`[Bridge] Topic ${topicName}: sink.type=${sink.type}, topicSinkType=${topicSinkType}, uri=${uri}`);
          } else {
            uri = `parquet://~/.casparian_flow/output/${topicName}/`;
            topicSinkType = 'parquet';
          }
          try {
            db.run(`
              INSERT OR REPLACE INTO cf_topic_config
                (plugin_name, topic_name, uri, enabled, sink_type)
              VALUES (?, ?, ?, 1, ?)
            `, [pluginName, topicName, uri, topicSinkType]);
            console.log(`[Bridge] Inserted topic ${topicName}`);
          } catch (err) {
            console.error(`[Bridge] Failed to insert topic ${topicName}:`, err);
          }
        }
      } catch (e) {
        console.error('Failed to parse topicUrisJson:', e);
      }
    } else {
      // Single output mode
      const uri = `${sinkType}://${outputPath || `~/.casparian_flow/output/${parserKey}/`}`;
      db.run(`
        INSERT OR REPLACE INTO cf_topic_config
          (plugin_name, topic_name, uri, enabled, sink_type)
        VALUES (?, ?, ?, 1, ?)
      `, [pluginName, 'default', uri, sinkType]);
    }

    return {
      success: true,
      pluginName: parserKey,
      message: `Deployed as ${filename}`,
      hash,
    };
  },

  // Add test file to the most recently updated parser
  parser_lab_add_test_file_to_latest: ({ filePath }: { filePath: string }) => {
    // Get the most recent parser
    const latestParser = db.query(`
      SELECT id FROM parser_lab_parsers ORDER BY updated_at DESC LIMIT 1
    `).get() as { id: string } | null;

    if (!latestParser) {
      throw new Error('No parsers found');
    }

    return commands.parser_lab_add_test_file({
      parserId: latestParser.id,
      filePath
    });
  },

  // Get parser by name
  parser_lab_get_parser_by_name: ({ name }: { name: string }) => {
    const parser = db.query(`
      SELECT id, name FROM parser_lab_parsers WHERE name = ?
    `).get(name) as { id: string; name: string } | null;

    if (!parser) {
      throw new Error(`Parser not found with name: ${name}`);
    }

    return parser;
  },

  // Import plugin from file
  parser_lab_import_plugin: ({ pluginPath }: { pluginPath: string }) => {
    if (!existsSync(pluginPath)) {
      throw new Error(`Plugin file not found: ${pluginPath}`);
    }

    const sourceCode = readFileSync(pluginPath, 'utf-8');
    const fileName = basename(pluginPath, '.py');
    const now = Date.now();
    const id = uuid();

    db.run(`
      INSERT INTO parser_lab_parsers
        (id, name, file_pattern, pattern_type, source_code, is_sample, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, 0, ?, ?)
    `, [id, fileName, fileName, 'key_column', sourceCode, now, now]);

    return {
      id,
      name: fileName,
      filePattern: fileName,
      patternType: 'key_column',
      sourceCode,
      validationStatus: 'pending',
      validationError: null,
      validationOutput: null,
      isSample: false,
      outputMode: 'single',
      createdAt: now,
      updatedAt: now,
    };
  },

  // Create parser with given properties
  parser_lab_create_parser: ({ name, filePattern }: { name: string; filePattern?: string }) => {
    const now = Date.now();
    const id = uuid();

    db.run(`
      INSERT INTO parser_lab_parsers
        (id, name, file_pattern, pattern_type, source_code, is_sample, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, 0, ?, ?)
    `, [id, name, filePattern || '', 'all', '', now, now]);

    return {
      id,
      name,
      filePattern: filePattern || '',
      patternType: 'all',
      sourceCode: '',
      validationStatus: 'pending',
      isSample: false,
      createdAt: now,
      updatedAt: now,
    };
  },

  // Submit tagged files for processing
  submit_tagged_files: ({ fileIds }: { fileIds: number[] }) => {
    const submitted = [];
    const skipped = [];
    const noPlugin: [number, string][] = [];
    const jobIds: [number, number][] = [];

    for (const fileId of fileIds) {
      // Get file info
      const file = db.query(`
        SELECT id, tag, manual_plugin FROM scout_files WHERE id = ?
      `).get(fileId) as { id: number; tag: string | null; manual_plugin: string | null } | null;

      if (!file) {
        skipped.push(fileId);
        continue;
      }

      // Check for plugin: manual override first, then tag-based lookup
      let pluginName = file.manual_plugin;

      if (!pluginName && file.tag) {
        // Look up plugin by tag
        const plugins = commands.get_plugins_for_tag({ tag: file.tag });
        if (plugins.length > 0) {
          pluginName = plugins[0];
        }
      }

      if (!pluginName) {
        noPlugin.push([fileId, file.tag || 'untagged']);
        continue;
      }

      // Create a job
      const jobId = db.run(`
        INSERT INTO scout_jobs (file_id, plugin_name, status, created_at)
        VALUES (?, ?, 'queued', datetime('now'))
      `, [fileId, pluginName]).lastInsertRowid;

      submitted.push(fileId);
      jobIds.push([fileId, Number(jobId)]);
    }

    return {
      submitted: submitted.length,
      skipped: skipped.length,
      jobIds,
      noPlugin,
    };
  },

  // Scout file management
  scout_set_manual_plugin: ({ fileId, pluginName }: { fileId: number; pluginName: string }) => {
    db.run(`
      UPDATE scout_files SET manual_plugin = ? WHERE id = ?
    `, [pluginName, fileId]);
    return true;
  },

  scout_clear_manual_overrides: ({ fileId }: { fileId: number }) => {
    db.run(`
      UPDATE scout_files SET tag = NULL, tag_source = NULL, rule_id = NULL, manual_plugin = NULL, status = 'pending' WHERE id = ?
    `, [fileId]);
    return true;
  },

  // Get plugins for a tag (used by FileList to find matching plugins)
  get_plugins_for_tag: ({ tag }: { tag: string }) => {
    const rows = db.query(`
      SELECT plugin_name, subscription_tags
      FROM cf_plugin_config
      WHERE enabled = 1
    `).all() as { plugin_name: string; subscription_tags: string }[];

    // Exact match only - tags are stored without prefix
    const matchingPlugins = rows
      .filter(row => row.subscription_tags.split(',').some(t => t.trim() === tag))
      .map(row => row.plugin_name);

    return matchingPlugins;
  },

  // List registered plugins from database (source of truth)
  list_registered_plugins: () => {
    const rows = db.query(`
      SELECT plugin_name FROM cf_plugin_manifest WHERE status = 'ACTIVE' ORDER BY plugin_name
    `).all() as { plugin_name: string }[];

    return rows.map(row => row.plugin_name);
  },

  // List deployed plugins with full details (matches Tauri list_deployed_plugins)
  // NOTE: Transform snake_case DB columns to camelCase to match Rust serde output
  list_deployed_plugins: () => {
    const rows = db.query(`
      SELECT plugin_name, version, status, deployed_at
      FROM cf_plugin_manifest
      ORDER BY deployed_at DESC, plugin_name
    `).all() as { plugin_name: string; version: string; status: string; deployed_at: string | null }[];

    return rows.map(r => ({
      pluginName: r.plugin_name,
      version: r.version,
      status: r.status,
      deployedAt: r.deployed_at
    }));
  },

  // Ensure plugin cache file exists (regenerate from DB if missing)
  ensure_plugin_cached: ({ pluginName }: { pluginName: string }) => {
    const parserPath = join(PARSERS_DIR, `${pluginName}.py`);

    // Check if cache file already exists
    if (existsSync(parserPath)) {
      return parserPath;
    }

    // Cache miss - regenerate from database
    const row = db.query(`
      SELECT source_code FROM cf_plugin_manifest WHERE plugin_name = ? AND status = 'ACTIVE'
    `).get(pluginName) as { source_code: string } | null;

    if (!row) {
      throw new Error(`Plugin '${pluginName}' not found in registry`);
    }

    // Write cache file
    writeFileSync(parserPath, row.source_code);
    console.log(`[Bridge] Regenerated cache for plugin '${pluginName}'`);

    return parserPath;
  },

  // System commands
  get_bind_address: () => 'bridge://localhost:9999',
  is_sentinel_running: () => false,
  get_system_pulse: () => ({
    timestamp: Date.now(),
    cpu_usage: 0,
    memory_usage: 0,
    disk_usage: 0,
    active_jobs: 0,
    pending_jobs: 0,
    completed_jobs: 0,
    failed_jobs: 0,
  }),

  get_job_outputs: () => [],

  // Get deployed plugin by name
  get_deployed_plugin: ({ name }: { name: string }) => {
    const row = db.query(`
      SELECT * FROM cf_plugin_manifest WHERE plugin_name = ?
    `).get(name) as any;
    return row || null;
  },

  // Get topic config for a plugin
  get_topic_config: ({ pluginName }: { pluginName: string }) => {
    const rows = db.query(`
      SELECT * FROM cf_topic_config WHERE plugin_name = ?
    `).all(pluginName) as any[];
    return rows || [];
  },

  // Run a parser job and write to sinks (for E2E testing)
  run_parser_job: async ({ pluginName, inputFilePath }: { pluginName: string; inputFilePath: string }) => {
    // Get plugin source code
    const plugin = db.query(`
      SELECT source_code FROM cf_plugin_manifest WHERE plugin_name = ? AND status = 'ACTIVE'
    `).get(pluginName) as { source_code: string } | null;

    if (!plugin) {
      throw new Error(`Plugin '${pluginName}' not found or not active`);
    }

    // Get topic configs
    const topicConfigs = db.query(`
      SELECT topic_name, uri, sink_type FROM cf_topic_config WHERE plugin_name = ?
    `).all(pluginName) as { topic_name: string; uri: string; sink_type: string }[];

    // Run parser and write to sinks
    const result = await runParserJob(plugin.source_code, inputFilePath, topicConfigs);
    return result;
  },

  // Check output files exist and have data
  verify_output_files: ({ pluginName }: { pluginName: string }) => {
    const topicConfigs = db.query(`
      SELECT topic_name, uri, sink_type FROM cf_topic_config WHERE plugin_name = ?
    `).all(pluginName) as { topic_name: string; uri: string; sink_type: string }[];

    const results: { topic: string; sinkType: string; path: string; exists: boolean; hasData: boolean; rowCount?: number }[] = [];

    for (const config of topicConfigs) {
      // Extract path from URI (e.g., "parquet://~/.casparian_flow/output/act_hw/" -> "~/.casparian_flow/output/act_hw/")
      const uriPath = config.uri.replace(/^(parquet|sqlite|csv):\/\//, '').replace(/^~/, HOME);
      let exists = false;
      let hasData = false;
      let rowCount: number | undefined;

      if (config.sink_type === 'sqlite') {
        // Check SQLite file
        const dbPath = uriPath.endsWith('.db') ? uriPath : join(uriPath, `${config.topic_name}.db`);
        exists = existsSync(dbPath);
        if (exists) {
          try {
            const sqliteDb = new Database(dbPath, { readonly: true });
            const countResult = sqliteDb.query(`SELECT COUNT(*) as cnt FROM ${config.topic_name}`).get() as { cnt: number };
            rowCount = countResult?.cnt || 0;
            hasData = rowCount > 0;
            sqliteDb.close();
          } catch {
            hasData = false;
          }
        }
      } else if (config.sink_type === 'parquet') {
        // Check parquet directory for .parquet files
        if (existsSync(uriPath)) {
          try {
            const files = readdirSync(uriPath).filter(f => f.endsWith('.parquet'));
            exists = files.length > 0;
            hasData = files.some(f => statSync(join(uriPath, f)).size > 0);
          } catch {
            exists = false;
          }
        }
      } else if (config.sink_type === 'csv') {
        const csvPath = uriPath.endsWith('.csv') ? uriPath : join(uriPath, `${config.topic_name}.csv`);
        exists = existsSync(csvPath);
        if (exists) {
          const content = readFileSync(csvPath, 'utf-8');
          const lines = content.trim().split('\n');
          rowCount = lines.length - 1; // Minus header
          hasData = rowCount > 0;
        }
      }

      results.push({
        topic: config.topic_name,
        sinkType: config.sink_type,
        path: uriPath,
        exists,
        hasData,
        rowCount,
      });
    }

    return results;
  },
};

// Python validation runner
async function runPythonValidation(code: string, testFilePath: string): Promise<{
  status: string;
  error: string | null;
  output: string | null;
  outputMode: string;
  detectedTopics: string[] | null;
}> {
  return new Promise((resolve) => {
    // Create temp file with validation wrapper
    const tempDir = join(CF_DIR, 'temp');
    mkdirSync(tempDir, { recursive: true });

    const wrapperCode = `
import sys
import json
from typing import NamedTuple, Union
import polars as pl
import pandas as pd
import types

# Define Output for parsers that import it
class Output(NamedTuple):
    name: str
    data: Union[pl.DataFrame, pd.DataFrame]
    sink: str
    table: str = None
    compression: str = "snappy"

# Mock casparian_types module so parser imports work
casparian_types = types.ModuleType('casparian_types')
casparian_types.Output = Output
sys.modules['casparian_types'] = casparian_types

# User code
${code}

# Run validation
try:
    result = parse("${testFilePath.replace(/\\/g, '\\\\')}")

    # Check for list[Output] (new multi-output contract)
    if isinstance(result, list) and len(result) > 0 and isinstance(result[0], tuple) and hasattr(result[0], 'name'):
        # Multi-output using Output NamedTuple
        topics = [out.name for out in result]
        output = {}
        for out in result:
            df = out.data
            rows = len(df) if hasattr(df, '__len__') else 0
            preview = str(df.head(5)) if hasattr(df, 'head') else str(df)
            output[out.name] = {
                "rows": rows,
                "preview": preview
            }
        print(json.dumps({
            "status": "valid",
            "mode": "multi",
            "topics": topics,
            "output": output
        }))
    elif isinstance(result, dict):
        # Legacy multi-output (dict[str, DataFrame])
        topics = list(result.keys())
        output = {}
        for topic, df in result.items():
            output[topic] = {
                "rows": len(df),
                "preview": str(df.head(5))
            }
        print(json.dumps({
            "status": "valid",
            "mode": "multi",
            "topics": topics,
            "output": output
        }))
    else:
        # Single output (DataFrame)
        print(json.dumps({
            "status": "valid",
            "mode": "single",
            "rows": len(result),
            "preview": str(result.head(10))
        }))
except Exception as e:
    import traceback
    print(json.dumps({
        "status": "invalid",
        "error": str(e) + "\\n" + traceback.format_exc()
    }))
`;

    const wrapperPath = join(tempDir, 'validate.py');
    writeFileSync(wrapperPath, wrapperCode);

    // Find Python with polars
    const pythonEnv = join(CF_DIR, 'shredder_env', 'bin', 'python');
    const pythonCmd = existsSync(pythonEnv) ? pythonEnv : 'python3';

    const proc = spawn(pythonCmd, [wrapperPath], {
      timeout: 60000,
    });

    let stdout = '';
    let stderr = '';

    proc.stdout.on('data', (data) => { stdout += data; });
    proc.stderr.on('data', (data) => { stderr += data; });

    proc.on('close', (code) => {
      try {
        const result = JSON.parse(stdout);
        if (result.status === 'valid') {
          if (result.mode === 'multi') {
            const outputStr = Object.entries(result.output)
              .map(([topic, data]: [string, any]) => `=== ${topic} (${data.rows} rows) ===\n${data.preview}`)
              .join('\n\n');
            resolve({
              status: 'valid',
              error: null,
              output: outputStr,
              outputMode: 'multi',
              detectedTopics: result.topics,
            });
          } else {
            resolve({
              status: 'valid',
              error: null,
              output: result.preview,
              outputMode: 'single',
              detectedTopics: null,
            });
          }
        } else {
          resolve({
            status: 'invalid',
            error: result.error,
            output: null,
            outputMode: 'single',
            detectedTopics: null,
          });
        }
      } catch {
        resolve({
          status: 'invalid',
          error: stderr || stdout || 'Unknown error',
          output: null,
          outputMode: 'single',
          detectedTopics: null,
        });
      }
    });

    proc.on('error', (err) => {
      resolve({
        status: 'invalid',
        error: `Failed to run Python: ${err.message}`,
        output: null,
        outputMode: 'single',
        detectedTopics: null,
      });
    });
  });
}

// Run parser and write outputs to configured sinks
async function runParserJob(
  code: string,
  inputFilePath: string,
  topicConfigs: { topic_name: string; uri: string; sink_type: string }[]
): Promise<{ success: boolean; message: string; outputs: { topic: string; rows: number }[] }> {
  return new Promise((resolve) => {
    const tempDir = join(CF_DIR, 'temp');
    mkdirSync(tempDir, { recursive: true });

    // Build sink config for Python
    const sinkConfig: Record<string, { type: string; path: string }> = {};
    for (const config of topicConfigs) {
      const uriPath = config.uri.replace(/^(parquet|sqlite|csv):\/\//, '').replace(/^~/, HOME);
      sinkConfig[config.topic_name] = {
        type: config.sink_type,
        path: uriPath,
      };
    }

    const wrapperCode = `
import sys
import json
import os
from typing import NamedTuple, Union
import polars as pl
import pandas as pd
import types

# Define Output for parsers that import it
class Output(NamedTuple):
    name: str
    data: Union[pl.DataFrame, pd.DataFrame]
    sink: str
    table: str = None
    compression: str = "snappy"

# Mock casparian_types module so parser imports work
casparian_types = types.ModuleType('casparian_types')
casparian_types.Output = Output
sys.modules['casparian_types'] = casparian_types

# User code
${code}

# Sink configuration
sink_config = ${JSON.stringify(sinkConfig)}

def write_output(topic: str, df, cfg: dict):
    """Write a dataframe to the configured sink."""
    sink_type = cfg['type']
    sink_path = cfg['path']
    os.makedirs(sink_path, exist_ok=True)

    if sink_type == 'parquet':
        out_file = os.path.join(sink_path, f"{topic}.parquet")
        if hasattr(df, 'write_parquet'):
            df.write_parquet(out_file)
        else:
            import pyarrow.parquet as pq
            pq.write_table(df.to_arrow() if hasattr(df, 'to_arrow') else pa.Table.from_pandas(df), out_file)
    elif sink_type == 'sqlite':
        import sqlite3
        db_file = os.path.join(sink_path, f"{topic}.db")
        conn = sqlite3.connect(db_file)
        pdf = df.to_pandas() if hasattr(df, 'to_pandas') else df
        pdf.to_sql(topic, conn, if_exists='replace', index=False)
        conn.close()
    elif sink_type == 'csv':
        out_file = os.path.join(sink_path, f"{topic}.csv")
        if hasattr(df, 'write_csv'):
            df.write_csv(out_file)
        else:
            df.to_pandas().to_csv(out_file, index=False)

# Run parser
try:
    result = parse("${inputFilePath.replace(/\\/g, '\\\\')}")

    outputs = []

    # Check for list[Output] (new multi-output contract)
    if isinstance(result, list) and len(result) > 0 and isinstance(result[0], tuple) and hasattr(result[0], 'name'):
        for out in result:
            topic = out.name
            df = out.data
            if topic in sink_config:
                write_output(topic, df, sink_config[topic])
                outputs.append({"topic": topic, "rows": len(df)})
    elif isinstance(result, dict):
        # Legacy multi-output (dict[str, DataFrame])
        for topic, df in result.items():
            if topic in sink_config:
                write_output(topic, df, sink_config[topic])
                outputs.append({"topic": topic, "rows": len(df)})
    else:
        # Single output - use first topic config
        if sink_config:
            first_topic = list(sink_config.keys())[0]
            write_output(first_topic, result, sink_config[first_topic])
            outputs.append({"topic": first_topic, "rows": len(result)})

    print(json.dumps({
        "success": True,
        "message": f"Processed {len(outputs)} topics",
        "outputs": outputs
    }))
except Exception as e:
    import traceback
    print(json.dumps({
        "success": False,
        "message": str(e) + "\\n" + traceback.format_exc(),
        "outputs": []
    }))
`;

    const wrapperPath = join(tempDir, 'run_job.py');
    writeFileSync(wrapperPath, wrapperCode);

    const pythonEnv = join(CF_DIR, 'shredder_env', 'bin', 'python');
    const pythonCmd = existsSync(pythonEnv) ? pythonEnv : 'python3';

    const proc = spawn(pythonCmd, [wrapperPath], { timeout: 120000 });

    let stdout = '';
    let stderr = '';

    proc.stdout.on('data', (data) => { stdout += data; });
    proc.stderr.on('data', (data) => { stderr += data; });

    proc.on('close', () => {
      try {
        const result = JSON.parse(stdout);
        resolve(result);
      } catch {
        resolve({
          success: false,
          message: stderr || stdout || 'Unknown error',
          outputs: [],
        });
      }
    });

    proc.on('error', (err) => {
      resolve({
        success: false,
        message: `Failed to run Python: ${err.message}`,
        outputs: [],
      });
    });
  });
}

// Initialize
initDb();

// Start HTTP server
const server = Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);

    // CORS headers
    const corsHeaders = {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type',
    };

    // Handle CORS preflight
    if (req.method === 'OPTIONS') {
      return new Response(null, { headers: corsHeaders });
    }

    // RPC endpoint
    if (url.pathname === '/api/rpc' && req.method === 'POST') {
      try {
        const body = await req.json() as { command: string; args?: any };
        const { command, args = {} } = body;

        console.log(`[Bridge] ${command}`, args);

        const handler = commands[command];
        if (!handler) {
          return Response.json(
            { error: `Unknown command: ${command}` },
            { status: 404, headers: corsHeaders }
          );
        }

        const result = await handler(args);
        return Response.json({ result }, { headers: corsHeaders });
      } catch (err: any) {
        console.error('[Bridge] Error:', err);
        return Response.json(
          { error: err.message },
          { status: 500, headers: corsHeaders }
        );
      }
    }

    // Health check
    if (url.pathname === '/health') {
      return Response.json({ status: 'ok', port: PORT }, { headers: corsHeaders });
    }

    return new Response('Not Found', { status: 404, headers: corsHeaders });
  },
});

console.log(`[Bridge] Test bridge server running on http://localhost:${PORT}`);
console.log('[Bridge] Database:', DB_PATH);
console.log('[Bridge] Ready for Playwright tests');
