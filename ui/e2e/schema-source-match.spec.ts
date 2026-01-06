/**
 * Schema Source Match Test (CI Guard)
 *
 * This test compares the database schemas defined in:
 * - ui/src-tauri/src/lib.rs (the REAL Tauri app schema)
 * - ui/scripts/test-bridge.ts (the test bridge schema)
 *
 * WHY THIS EXISTS:
 * On 2025-01-05, tests passed but manual testing failed because the bridge
 * had a different schema (first_seen_at vs first_seen, missing size_bytes).
 * This test prevents future schema drift.
 */
import { test, expect } from "@playwright/test";
import { readFileSync, existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const LIB_RS_PATH = join(__dirname, "../src-tauri/src/lib.rs");
const BRIDGE_PATH = join(__dirname, "../scripts/test-bridge.ts");

// Parse CREATE TABLE statements from source code
function extractTables(content: string): Map<string, Set<string>> {
  const tables = new Map<string, Set<string>>();

  // Match CREATE TABLE statements for cf_* tables
  // Handles both "CREATE TABLE IF NOT EXISTS cf_foo" and "CREATE TABLE cf_foo"
  const tableRegex = /CREATE TABLE(?:\s+IF NOT EXISTS)?\s+(cf_\w+)\s*\(([\s\S]*?)\)/g;
  let match;

  while ((match = tableRegex.exec(content)) !== null) {
    const tableName = match[1];
    const columnsStr = match[2];

    // Extract column names (first word of each line, excluding constraints)
    const columns = new Set<string>();
    const lines = columnsStr.split(/,\s*\n|\n/);
    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed && !trimmed.startsWith("FOREIGN") && !trimmed.startsWith("UNIQUE")) {
        const colName = trimmed.split(/\s+/)[0];
        if (colName && colName !== ")") {
          columns.add(colName.toLowerCase());
        }
      }
    }
    if (columns.size > 0) {
      tables.set(tableName, columns);
    }
  }
  return tables;
}

test.describe("Schema Source Match (CI Guard)", () => {
  test.beforeAll(() => {
    // Verify source files exist
    expect(existsSync(LIB_RS_PATH), `lib.rs should exist at ${LIB_RS_PATH}`).toBe(true);
    expect(existsSync(BRIDGE_PATH), `test-bridge.ts should exist at ${BRIDGE_PATH}`).toBe(true);
  });

  test("bridge has all cf_* tables from lib.rs", () => {
    const libRs = readFileSync(LIB_RS_PATH, "utf-8");
    const bridge = readFileSync(BRIDGE_PATH, "utf-8");

    const libTables = extractTables(libRs);
    const bridgeTables = extractTables(bridge);

    console.log("Tables found in lib.rs:", [...libTables.keys()]);
    console.log("Tables found in bridge:", [...bridgeTables.keys()]);

    const missingTables: string[] = [];
    for (const [tableName] of libTables) {
      if (!bridgeTables.has(tableName)) {
        missingTables.push(tableName);
      }
    }

    if (missingTables.length > 0) {
      console.error("Missing tables in bridge:", missingTables);
    }

    expect(
      missingTables.length,
      `Bridge is missing tables: ${missingTables.join(", ")}`
    ).toBe(0);
  });

  test("cf_file_hash_registry has correct columns", () => {
    const libRs = readFileSync(LIB_RS_PATH, "utf-8");
    const bridge = readFileSync(BRIDGE_PATH, "utf-8");

    const libTables = extractTables(libRs);
    const bridgeTables = extractTables(bridge);

    const libCols = libTables.get("cf_file_hash_registry");
    const bridgeCols = bridgeTables.get("cf_file_hash_registry");

    expect(bridgeCols, "Bridge missing cf_file_hash_registry table").toBeDefined();
    expect(libCols, "lib.rs missing cf_file_hash_registry table").toBeDefined();

    console.log("lib.rs cf_file_hash_registry columns:", [...libCols!]);
    console.log("bridge cf_file_hash_registry columns:", [...bridgeCols!]);

    // Verify required columns exist
    expect(bridgeCols!.has("content_hash"), "Missing content_hash column").toBe(true);
    expect(bridgeCols!.has("first_seen"), "Missing first_seen column").toBe(true);
    expect(bridgeCols!.has("size_bytes"), "Missing size_bytes column").toBe(true);

    // Check for WRONG column names (regressions)
    expect(
      bridgeCols!.has("first_seen_at"),
      "Bridge has wrong column 'first_seen_at', should be 'first_seen'"
    ).toBe(false);
  });

  test("all cf_* tables have matching columns", () => {
    const libRs = readFileSync(LIB_RS_PATH, "utf-8");
    const bridge = readFileSync(BRIDGE_PATH, "utf-8");

    const libTables = extractTables(libRs);
    const bridgeTables = extractTables(bridge);

    const errors: string[] = [];

    for (const [tableName, libCols] of libTables) {
      const bridgeCols = bridgeTables.get(tableName);
      if (!bridgeCols) {
        errors.push(`Missing table: ${tableName}`);
        continue;
      }

      for (const col of libCols) {
        if (!bridgeCols.has(col)) {
          errors.push(`${tableName}: missing column '${col}'`);
        }
      }
    }

    if (errors.length > 0) {
      console.error("Schema mismatches found:");
      errors.forEach((e) => console.error(`  - ${e}`));
    }

    expect(errors.length, `Schema mismatches: ${errors.join(", ")}`).toBe(0);
  });

  test("no deprecated column names exist in bridge cf_file_hash_registry", () => {
    const bridge = readFileSync(BRIDGE_PATH, "utf-8");

    // Extract just the cf_file_hash_registry table definition
    const tableMatch = bridge.match(/CREATE TABLE IF NOT EXISTS cf_file_hash_registry\s*\([^)]+\)/);
    expect(tableMatch, "Bridge should have cf_file_hash_registry table").toBeTruthy();

    const tableDefinition = tableMatch![0];
    console.log("cf_file_hash_registry table definition:", tableDefinition);

    // Check for known wrong column names that have caused bugs
    // Only check within the specific table definition, not the entire file
    expect(
      tableDefinition.includes("first_seen_at"),
      "cf_file_hash_registry has wrong column 'first_seen_at', should be 'first_seen'"
    ).toBe(false);

    // Verify correct column exists
    expect(
      tableDefinition.includes("first_seen"),
      "cf_file_hash_registry should have 'first_seen' column"
    ).toBe(true);
  });
});
