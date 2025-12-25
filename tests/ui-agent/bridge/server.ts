/**
 * Bridge Server - HTTP to SQLite Proxy
 *
 * This server mimics Tauri's invoke() behavior for Playwright tests.
 * It receives HTTP requests and executes the same SQLite operations
 * that the Tauri commands would.
 *
 * Usage: bun run tests/ui-agent/bridge/server.ts
 */

import { Database } from "bun:sqlite";

// Get database path from environment or use default
const DB_PATH = process.env.CASPARIAN_TEST_DB || "/tmp/casparian_test.db";

// Initialize database connection
let db: Database;

function initDatabase() {
  db = new Database(DB_PATH);
  db.exec("PRAGMA journal_mode = WAL");
  console.log(`[Bridge] Connected to ${DB_PATH}`);
}

// ============================================================================
// Command Handlers (mirror Tauri commands from lib.rs)
// ============================================================================

type CommandHandler = (args: Record<string, unknown>) => unknown;

const commands: Record<string, CommandHandler> = {
  // System
  get_system_pulse: () => ({
    connectedWorkers: 0,
    jobsCompleted: 0,
    jobsFailed: 0,
    jobsDispatched: 0,
    jobsInFlight: 0,
    avgDispatchMs: 0,
    avgConcludeMs: 0,
    messagesSent: 0,
    messagesReceived: 0,
    timestamp: Math.floor(Date.now() / 1000),
  }),

  is_sentinel_running: () => true,

  get_bind_address: () => "bridge://localhost:9999",

  // Routing Rules CRUD
  get_routing_rules: () => {
    const rows = db
      .query(
        `SELECT id, pattern, tag, priority, enabled, description
         FROM cf_routing_rules
         ORDER BY priority DESC, id`
      )
      .all();

    return rows.map((row: any) => ({
      id: row.id,
      pattern: row.pattern,
      tag: row.tag,
      priority: row.priority,
      enabled: row.enabled !== 0,
      description: row.description,
    }));
  },

  create_routing_rule: (args) => {
    const { pattern, tag, priority, description } = args as {
      pattern: string;
      tag: string;
      priority: number;
      description?: string;
    };

    const result = db
      .query(
        `INSERT INTO cf_routing_rules (pattern, tag, priority, enabled, description)
         VALUES (?, ?, ?, 1, ?)`
      )
      .run(pattern, tag, priority, description || null);

    return result.lastInsertRowid;
  },

  update_routing_rule: (args) => {
    const { rule } = args as {
      rule: {
        id: number;
        pattern: string;
        tag: string;
        priority: number;
        enabled: boolean;
        description?: string;
      };
    };

    db.query(
      `UPDATE cf_routing_rules
       SET pattern = ?, tag = ?, priority = ?, enabled = ?, description = ?
       WHERE id = ?`
    ).run(
      rule.pattern,
      rule.tag,
      rule.priority,
      rule.enabled ? 1 : 0,
      rule.description || null,
      rule.id
    );

    return null;
  },

  delete_routing_rule: (args) => {
    const { id } = args as { id: number };
    db.query("DELETE FROM cf_routing_rules WHERE id = ?").run(id);
    return null;
  },

  // Topic Configuration
  get_topic_configs: () => {
    const rows = db
      .query(
        `SELECT id, plugin_name, topic_name, uri, mode
         FROM cf_topic_config
         ORDER BY plugin_name, topic_name`
      )
      .all();

    return rows.map((row: any) => ({
      id: row.id,
      pluginName: row.plugin_name,
      topicName: row.topic_name,
      uri: row.uri,
      mode: row.mode,
    }));
  },

  update_topic_uri: (args) => {
    const { id, uri } = args as { id: number; uri: string };
    db.query("UPDATE cf_topic_config SET uri = ? WHERE id = ?").run(uri, id);
    return null;
  },

  // Pipeline Topology
  get_topology: () => {
    const plugins = db
      .query(
        `SELECT plugin_name, subscription_tags, default_parameters
         FROM cf_plugin_config`
      )
      .all();

    const topics = db
      .query(
        `SELECT id, plugin_name, topic_name, uri, mode
         FROM cf_topic_config`
      )
      .all();

    // Build topology (simplified version)
    const nodes: any[] = [];
    const edges: any[] = [];

    // Plugin nodes
    plugins.forEach((p: any, idx: number) => {
      nodes.push({
        id: `plugin:${p.plugin_name}`,
        label: p.plugin_name,
        nodeType: "plugin",
        status: "active",
        metadata: { tags: p.subscription_tags || "" },
        x: 100,
        y: 50 + idx * 120,
      });
    });

    // Topic nodes
    topics.forEach((t: any, idx: number) => {
      nodes.push({
        id: `topic:${t.plugin_name}:${t.topic_name}`,
        label: t.topic_name,
        nodeType: "topic",
        status: null,
        metadata: { uri: t.uri, mode: t.mode },
        x: 500,
        y: 50 + idx * 120,
      });

      // Edge from plugin to topic
      edges.push({
        id: `e${idx}`,
        source: `plugin:${t.plugin_name}`,
        target: `topic:${t.plugin_name}:${t.topic_name}`,
        label: "publishes",
        animated: true,
      });
    });

    return { nodes, edges };
  },

  // Job outputs
  get_job_outputs: (args) => {
    const limit = (args?.limit as number) || 50;

    const rows = db
      .query(
        `SELECT id, plugin_name, status, result_summary, end_time
         FROM cf_processing_queue
         WHERE status IN ('COMPLETED', 'FAILED')
         ORDER BY end_time DESC
         LIMIT ?`
      )
      .all(limit);

    return rows.map((row: any) => ({
      jobId: row.id,
      pluginName: row.plugin_name,
      status: row.status,
      outputPath: row.result_summary,
      completedAt: row.end_time,
    }));
  },

  get_job_details: (args) => {
    const { jobId } = args as { jobId: number };

    const row: any = db
      .query(
        `SELECT id, plugin_name, status, result_summary, error_message,
                claim_time, end_time, retry_count
         FROM cf_processing_queue
         WHERE id = ?`
      )
      .get(jobId);

    if (!row) {
      throw new Error("Job not found");
    }

    // Get logs if available
    const logRow: any = db
      .query("SELECT log_text FROM cf_job_logs WHERE job_id = ?")
      .get(jobId);

    return {
      jobId: row.id,
      pluginName: row.plugin_name,
      status: row.status,
      outputPath: row.result_summary,
      errorMessage: row.error_message,
      resultSummary: row.result_summary,
      claimTime: row.claim_time,
      endTime: row.end_time,
      retryCount: row.retry_count,
      logs: logRow?.log_text || null,
    };
  },
};

// ============================================================================
// HTTP Server
// ============================================================================

const server = Bun.serve({
  port: 9999,

  async fetch(req) {
    const url = new URL(req.url);

    // CORS headers
    const corsHeaders = {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type",
    };

    // Handle preflight
    if (req.method === "OPTIONS") {
      return new Response(null, { headers: corsHeaders });
    }

    // Health check
    if (url.pathname === "/api/pulse") {
      return Response.json(
        { ok: true, db: DB_PATH },
        { headers: corsHeaders }
      );
    }

    // RPC endpoint
    if (url.pathname === "/api/rpc" && req.method === "POST") {
      try {
        const body = await req.json();
        const { command, args } = body as {
          command: string;
          args?: Record<string, unknown>;
        };

        console.log(`[Bridge] ${command}`, args || "");

        const handler = commands[command];
        if (!handler) {
          return Response.json(
            { error: `Unknown command: ${command}` },
            { status: 400, headers: corsHeaders }
          );
        }

        const result = handler(args || {});
        return Response.json(
          { result },
          { headers: corsHeaders }
        );
      } catch (e: any) {
        console.error(`[Bridge] Error:`, e);
        return Response.json(
          { error: e.message },
          { status: 500, headers: corsHeaders }
        );
      }
    }

    return Response.json(
      { error: "Not found" },
      { status: 404, headers: corsHeaders }
    );
  },
});

// Initialize and start
initDatabase();
console.log(`[Bridge] Server running on http://localhost:${server.port}`);
