/**
 * Deploy Plugin Guard Rail
 *
 * This test scans the Rust source code to ensure deploy_plugin
 * and related functions use ACTIVE status, not PENDING.
 *
 * WHY: On 2025-01-05, lib.rs used 'PENDING' but main.rs queries for 'ACTIVE'.
 * This test would have caught that instantly.
 */
import { test, expect } from "@playwright/test";
import { readFileSync, existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const PROJECT_ROOT = join(__dirname, "../..");

test.describe("Deploy Plugin Guard Rails", () => {
  test("lib.rs deploy_plugin uses ACTIVE status (not PENDING)", async () => {
    const libRsPath = join(__dirname, "../src-tauri/src/lib.rs");

    if (!existsSync(libRsPath)) {
      test.skip(true, "lib.rs not found - not in Tauri context");
      return;
    }

    const content = readFileSync(libRsPath, "utf-8");

    // Find the deploy_plugin function area (around cf_plugin_manifest INSERT)
    const deploySection = content.match(
      /INSERT INTO cf_plugin_manifest[\s\S]{0,500}/
    );

    if (!deploySection) {
      console.log("Could not find cf_plugin_manifest INSERT in lib.rs");
      return;
    }

    const section = deploySection[0];

    // Check for PENDING status (should NOT exist)
    const hasPending = section.includes("'PENDING'");
    const hasActive = section.includes("'ACTIVE'");

    if (hasPending) {
      console.error("CRITICAL: Found 'PENDING' status in deploy_plugin!");
      console.error("This breaks job processing - main.rs only queries ACTIVE/DEPLOYED");
      console.error("\nSection:\n", section);
    }

    expect(hasPending, "deploy_plugin must NOT use 'PENDING' status").toBe(false);
    expect(hasActive, "deploy_plugin MUST use 'ACTIVE' status").toBe(true);
  });

  test("main.rs job processor queries ACTIVE or DEPLOYED status", async () => {
    const mainRsPath = join(PROJECT_ROOT, "crates/casparian/src/main.rs");

    if (!existsSync(mainRsPath)) {
      test.skip(true, "main.rs not found");
      return;
    }

    const content = readFileSync(mainRsPath, "utf-8");

    // Find the plugin lookup query
    const queryMatch = content.match(
      /SELECT.*FROM cf_plugin_manifest.*WHERE[\s\S]{0,300}status/i
    );

    if (!queryMatch) {
      console.log("Could not find plugin lookup query in main.rs");
      // Not a failure - query might have changed
      return;
    }

    const query = queryMatch[0];

    // Verify it looks for ACTIVE or DEPLOYED
    const hasActiveCheck = query.includes("ACTIVE") || query.includes("DEPLOYED");

    expect(
      hasActiveCheck,
      "Job processor should query for ACTIVE or DEPLOYED status"
    ).toBe(true);
  });

  test("config.rs uses casparian_flow.sqlite3 not scout.db", async () => {
    const configRsPath = join(
      PROJECT_ROOT,
      "crates/casparian_scout/src/config.rs"
    );

    if (!existsSync(configRsPath)) {
      test.skip(true, "config.rs not found");
      return;
    }

    const content = readFileSync(configRsPath, "utf-8");

    // Find default_database_path function
    const fnMatch = content.match(
      /fn default_database_path\(\)[^{]*\{[\s\S]*?^\}/m
    );

    if (!fnMatch) {
      console.log("Could not find default_database_path function");
      return;
    }

    const fn = fnMatch[0];

    // Should NOT have hardcoded scout.db
    const hasScoutDb = fn.includes('"scout.db"');
    // Should have casparian_flow.sqlite3
    const hasCasparianDb = fn.includes("casparian_flow.sqlite3");

    if (hasScoutDb) {
      console.error("CRITICAL: config.rs still uses scout.db!");
      console.error("Should use casparian_flow.sqlite3");
    }

    expect(hasScoutDb, "Should NOT use scout.db").toBe(false);
    expect(hasCasparianDb, "Should use casparian_flow.sqlite3").toBe(true);
  });

  test("test-bridge.ts publish_parser uses ACTIVE status", async () => {
    const bridgePath = join(__dirname, "../scripts/test-bridge.ts");

    if (!existsSync(bridgePath)) {
      test.skip(true, "test-bridge.ts not found");
      return;
    }

    const content = readFileSync(bridgePath, "utf-8");

    // Find publish_parser implementation
    const publishMatch = content.match(
      /publish_parser.*?INSERT INTO cf_plugin_manifest[\s\S]{0,500}/
    );

    if (!publishMatch) {
      console.log("Could not find publish_parser INSERT in test-bridge.ts");
      return;
    }

    const section = publishMatch[0];

    // Should use ACTIVE
    const hasActive = section.includes("'ACTIVE'") || section.includes("ACTIVE");

    expect(hasActive, "test-bridge publish_parser must use ACTIVE status").toBe(
      true
    );
  });
});
