/**
 * Rust Binary Startup Test
 *
 * This test starts the ACTUAL Rust binaries and verifies they don't crash on startup.
 * It catches schema mismatches that bridge-only tests miss.
 *
 * WHY THIS EXISTS:
 * On 2025-01-05, the bridge tests passed but the real app crashed because:
 * - casparian_sentinel expected `schema_json` column in cf_topic_config
 * - lib.rs didn't create that column
 * - Bridge tests only check bridge schema, not what Sentinel expects
 *
 * This test would have caught that immediately.
 *
 * NOTE: We test the CLI binary (casparian) not the GUI (casparian-deck)
 * because CLI can run headlessly and uses the same Sentinel code.
 */

import { test, expect } from "@playwright/test";
import { spawn } from "child_process";
import { join, dirname } from "path";
import { existsSync } from "fs";
import { homedir } from "os";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const PROJECT_ROOT = join(__dirname, "../..");
const CF_DIR = join(homedir(), ".casparian_flow");

// Patterns that indicate startup failure
const ERROR_PATTERNS = [
  /ERROR.*Failed to start Sentinel/i,
  /ERROR.*Sentinel.*terminated/i,        // "Sentinel thread terminated unexpectedly"
  /Sentinel failed to start/i,           // Worker shutdown message
  /ERROR.*no column found/i,
  /ERROR.*Database error/i,
  /ERROR.*Failed to initialize/i,
  /Failed to open.*database/i,
  /Failed to connect to database/i,      // Sentinel DB connection error
  /panic/i,
  /FATAL/i,
];

// Patterns that indicate successful startup
const SUCCESS_PATTERNS = [
  /Sentinel started/i,
  /Worker started/i,
  /Starting unified/i,
  /database.*initialized/i,
];

test.describe("Rust Binary Startup", () => {
  test.setTimeout(30000); // 30 seconds

  test("casparian binary starts Sentinel without database errors", async () => {
    // Use the CLI binary which starts Sentinel
    const binaryPath = join(PROJECT_ROOT, "target/release/casparian");
    const debugBinaryPath = join(PROJECT_ROOT, "target/debug/casparian");

    const actualBinaryPath = existsSync(binaryPath)
      ? binaryPath
      : existsSync(debugBinaryPath)
        ? debugBinaryPath
        : null;

    if (!actualBinaryPath) {
      test.skip(true, "Casparian binary not found. Run 'cargo build --release -p casparian' first.");
      return;
    }

    console.log(`Testing binary: ${actualBinaryPath}`);

    // First, ensure bridge creates the databases with correct schema
    console.log("Initializing databases via bridge...");
    const bridgeInit = spawn("bun", ["run", "scripts/test-bridge.ts"], {
      cwd: join(__dirname, ".."),
      stdio: ["ignore", "pipe", "pipe"],
    });

    // Wait for bridge to initialize DBs
    await new Promise((resolve) => setTimeout(resolve, 3000));
    bridgeInit.kill("SIGTERM");

    // Verify databases were created
    const sentinelDbPath = join(CF_DIR, "casparian_flow.sqlite3");
    expect(existsSync(sentinelDbPath), "Sentinel DB should exist after bridge init").toBe(true);
    console.log(`Sentinel DB created: ${sentinelDbPath}`);

    // Now start the casparian binary with 'start' command
    // It will try to connect to the database and start Sentinel
    console.log("\nStarting casparian binary...");

    const logs: string[] = [];
    let hasError = false;
    let hasSuccess = false;
    let exitCode: number | null = null;

    const app = spawn(actualBinaryPath, ["start"], {
      cwd: PROJECT_ROOT,
      stdio: ["ignore", "pipe", "pipe"],
      env: {
        ...process.env,
        RUST_LOG: "info,casparian=debug,casparian_sentinel=debug",
      },
    });

    const collectLogs = (data: Buffer) => {
      const text = data.toString();
      logs.push(text);
      process.stdout.write(text); // Real-time output

      // Check for errors
      for (const pattern of ERROR_PATTERNS) {
        if (pattern.test(text)) {
          hasError = true;
          console.error(`\n!!! ERROR DETECTED: ${pattern} !!!`);
        }
      }

      // Check for success
      for (const pattern of SUCCESS_PATTERNS) {
        if (pattern.test(text)) {
          hasSuccess = true;
        }
      }
    };

    app.stdout.on("data", collectLogs);
    app.stderr.on("data", collectLogs);

    app.on("close", (code) => {
      exitCode = code;
    });

    // Wait for startup or error (up to 10 seconds)
    const startTime = Date.now();
    while (Date.now() - startTime < 10000) {
      if (hasError || hasSuccess || exitCode !== null) break;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }

    // Kill the app gracefully
    if (exitCode === null) {
      console.log("\nSending SIGTERM...");
      app.kill("SIGTERM");
      await new Promise((resolve) => setTimeout(resolve, 2000));
      if (!app.killed) {
        app.kill("SIGKILL");
      }
    }

    // Analyze results
    const fullLog = logs.join("");

    if (hasError) {
      console.error("\n" + "=".repeat(60));
      console.error("STARTUP FAILED - DATABASE SCHEMA MISMATCH DETECTED");
      console.error("=".repeat(60));

      // Extract specific error
      for (const pattern of ERROR_PATTERNS) {
        const match = fullLog.match(pattern);
        if (match) {
          console.error(`\nError: ${match[0]}`);
        }
      }

      console.error("\nThis usually means:");
      console.error("1. Bridge schema doesn't match what Sentinel expects");
      console.error("2. lib.rs create_tables() is missing columns");
      console.error("3. A migration is needed for new columns");
      console.error("\nCheck crates/casparian_sentinel/src/db/models.rs for expected columns");

      expect(hasError, "Casparian should start without database errors").toBe(false);
    }

    if (exitCode !== null && exitCode !== 0 && !hasSuccess) {
      console.error(`\nProcess exited with code ${exitCode}`);
      console.error("Full logs:", fullLog);
      expect(exitCode, "Casparian should not crash on startup").toBe(0);
    }

    console.log("\n" + "=".repeat(60));
    console.log("STARTUP TEST PASSED");
    console.log("=".repeat(60));
  });

  test("single database exists at correct path", async () => {
    // SINGLE DATABASE: everything uses casparian_flow.sqlite3
    const dbPath = join(CF_DIR, "casparian_flow.sqlite3");
    const oldScoutDbPath = join(CF_DIR, "scout.db");

    expect(existsSync(CF_DIR), "~/.casparian_flow directory should exist").toBe(true);

    console.log("Database check:");
    console.log(`  Database: ${dbPath} (exists: ${existsSync(dbPath)})`);
    console.log(`  Old scout.db: ${oldScoutDbPath} (exists: ${existsSync(oldScoutDbPath)})`);

    // Single database must exist
    expect(
      existsSync(dbPath),
      "Database should exist at ~/.casparian_flow/casparian_flow.sqlite3"
    ).toBe(true);

    // Old scout.db should NOT exist (we use single DB now)
    expect(
      existsSync(oldScoutDbPath),
      "scout.db should NOT exist - we use single database now"
    ).toBe(false);
  });
});
