/**
 * Schema Compatibility Test
 *
 * This test ensures the bridge database schema matches what Rust expects.
 * If this test fails, the bridge schema in test-bridge.ts has diverged
 * from the Rust schema in crates/casparian_scout/src/db.rs
 *
 * WHY THIS EXISTS:
 * On 2025-01-05, we shipped a bug where the bridge created tables with
 * different column names than Rust. Tests passed but the real app crashed.
 * This test prevents that from happening again.
 */

import { test, expect } from "@playwright/test";

const BRIDGE_URL = "http://localhost:9999";

async function bridgeCall(command: string, args: Record<string, unknown> = {}) {
  const response = await fetch(`${BRIDGE_URL}/api/rpc`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ command, args }),
  });
  const data = await response.json();
  if (data.error) throw new Error(data.error);
  return data.result;
}

test.describe("Schema Compatibility", () => {
  test("scout_files table has all required columns", async () => {
    // These are the columns the Rust code expects (from db.rs)
    const requiredColumns = [
      "id",
      "source_id",
      "path",        // NOT "file_path"
      "rel_path",
      "size",        // NOT "file_size"
      "mtime",       // CRITICAL - this was missing before
      "content_hash",
      "status",
      "tag",
      "tag_source",
      "rule_id",
      "manual_plugin",
      "error",
      "first_seen_at",
      "last_seen_at",
      "processed_at",
      "sentinel_job_id",
    ];

    const tableInfo = await bridgeCall("get_table_columns", {
      tableName: "scout_files",
    });

    const columns = tableInfo.map((col: { name: string }) => col.name);

    for (const required of requiredColumns) {
      expect(columns, `Missing column: ${required}`).toContain(required);
    }
  });

  test("scout_sources table exists with correct columns", async () => {
    const requiredColumns = [
      "id",
      "name",
      "source_type",
      "path",
      "poll_interval_secs",
      "enabled",
      "created_at",
      "updated_at",
    ];

    const tableInfo = await bridgeCall("get_table_columns", {
      tableName: "scout_sources",
    });

    const columns = tableInfo.map((col: { name: string }) => col.name);

    for (const required of requiredColumns) {
      expect(columns, `Missing column: ${required}`).toContain(required);
    }
  });

  test("parser_lab_parsers table has multi-output columns", async () => {
    // These columns were added in v7 migration
    const requiredColumns = [
      "id",
      "name",
      "file_pattern",
      "pattern_type",
      "source_code",
      "validation_status",
      "output_mode",         // v7 migration
      "detected_topics_json", // v7 migration
    ];

    const tableInfo = await bridgeCall("get_table_columns", {
      tableName: "parser_lab_parsers",
    });

    const columns = tableInfo.map((col: { name: string }) => col.name);

    for (const required of requiredColumns) {
      expect(columns, `Missing column: ${required}`).toContain(required);
    }
  });

  test("required indexes exist", async () => {
    // Critical indexes that Rust creates
    const requiredIndexes = [
      "idx_files_mtime",
      "idx_files_path",
      "idx_files_tag",
      "idx_files_status",
    ];

    const indexes = await bridgeCall("get_indexes", {});

    for (const required of requiredIndexes) {
      expect(indexes, `Missing index: ${required}`).toContain(required);
    }
  });

  // ========================================================================
  // Sentinel tables (cf_* tables) - must match lib.rs create_tables()
  // ========================================================================

  test("cf_topic_config has all required columns", async () => {
    // This table is used by both lib.rs AND casparian_sentinel
    // The Sentinel's TopicConfig model (db/models.rs) requires schema_json
    // Bug discovered 2025-01-05: sink_type was missing
    // Bug discovered 2025-01-05: schema_json was missing (Sentinel startup failed)
    const requiredColumns = [
      "id",
      "plugin_name",
      "topic_name",
      "uri",
      "mode",
      "sink_type",    // Required by lib.rs publish flow
      "schema_json",  // CRITICAL - required by casparian_sentinel TopicConfig model
      "enabled",
    ];

    const tableInfo = await bridgeCall("get_table_columns", {
      tableName: "cf_topic_config",
    });

    const columns = tableInfo.map((col: { name: string }) => col.name);

    for (const required of requiredColumns) {
      expect(columns, `cf_topic_config missing column: ${required}`).toContain(required);
    }
  });

  test("cf_plugin_manifest has required columns", async () => {
    const requiredColumns = [
      "id",
      "plugin_name",
      "version",
      "source_code",
      "source_hash",
      "status",
      "deployed_at",
    ];

    const tableInfo = await bridgeCall("get_table_columns", {
      tableName: "cf_plugin_manifest",
    });

    const columns = tableInfo.map((col: { name: string }) => col.name);

    for (const required of requiredColumns) {
      expect(columns, `cf_plugin_manifest missing column: ${required}`).toContain(required);
    }
  });

  test("cf_plugin_config has required columns", async () => {
    const requiredColumns = [
      "id",
      "plugin_name",
      "subscription_tags",
      "enabled",
    ];

    const tableInfo = await bridgeCall("get_table_columns", {
      tableName: "cf_plugin_config",
    });

    const columns = tableInfo.map((col: { name: string }) => col.name);

    for (const required of requiredColumns) {
      expect(columns, `cf_plugin_config missing column: ${required}`).toContain(required);
    }
  });
});
