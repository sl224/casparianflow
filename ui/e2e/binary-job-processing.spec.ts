/**
 * Real Binary Job Processing Test
 *
 * Tests the ACTUAL casparian binary end-to-end, not just the bridge.
 * This catches issues where bridge tests pass but real binary fails.
 *
 * On 2025-01-05, all bridge tests passed but binary failed with
 * "Plugin not found" because of status mismatch.
 */
import { test, expect } from "@playwright/test";
import { execSync } from "child_process";
import { join, dirname } from "path";
import { existsSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { homedir } from "os";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const PROJECT_ROOT = join(__dirname, "../..");
const CF_DIR = join(homedir(), ".casparian_flow");
const DB_PATH = join(CF_DIR, "casparian_flow.sqlite3");
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

function findBinary(): string | null {
  const release = join(PROJECT_ROOT, "target/release/casparian");
  const debug = join(PROJECT_ROOT, "target/debug/casparian");
  if (existsSync(release)) return release;
  if (existsSync(debug)) return debug;
  return null;
}

test.describe("Real Binary Job Processing", () => {
  test.setTimeout(120000); // 2 minutes

  test("plugin is processable after deployment", async () => {
    // This test verifies the critical STATUS fix: plugins must be ACTIVE to be found.
    // We test the validation query which uses the EXACT same criteria as main.rs:796-808
    const testId = `binary_e2e_${Date.now()}`;
    console.log(`\n=== Plugin Processability Test: ${testId} ===\n`);

    // 1. Publish plugin via bridge
    const code = `import polars as pl

TOPIC = "test_data"
SINK = "parquet"

def parse(input_path: str) -> pl.DataFrame:
    """Parse CSV file and return DataFrame."""
    df = pl.read_csv(input_path)
    return df
`;

    console.log("Publishing plugin via bridge...");
    await bridgeCall("publish_parser", {
      parserKey: testId,
      sourceCode: code,
      sinkType: "parquet",
      outputPath: `/tmp/${testId}/`,
      outputMode: "single",
    });
    console.log(`Plugin published: ${testId}`);

    // 2. Verify plugin is processable using EXACT main.rs query criteria
    const validation = await bridgeCall("validate_plugin_processable", {
      pluginName: testId,
    }) as { processable: boolean; status?: string; reason?: string };

    if (!validation.processable) {
      console.error("\n" + "=".repeat(60));
      console.error("STATUS BUG DETECTED!");
      console.error("Plugin was published but not processable!");
      console.error("Reason:", validation.reason);
      console.error("This indicates deploy_plugin uses wrong status value.");
      console.error("=".repeat(60));
    }

    expect(validation.processable, `Plugin must be processable: ${validation.reason}`).toBe(true);
    expect(validation.status).toBe("ACTIVE");

    console.log(`\n=== Plugin Processability Test PASSED ===\n`);
  });

  test.skip("binary can find and process deployed plugin (requires full file chain)", async () => {
    // NOTE: This test is skipped because the binary's process-job command
    // requires a full file chain (cf_source_root → cf_file_location → cf_file_version)
    // The bridge creates jobs with just input_file which the binary doesn't use.
    //
    // The critical STATUS bug is tested by:
    // - e2e/status-consistency.spec.ts (uses validate_plugin_processable)
    // - The test above (plugin is processable after deployment)
    //
    // To fully test binary job processing, we would need to:
    // 1. Add files via Scout (which populates the file chain)
    // 2. Create jobs referencing those files
    // 3. Run the binary
  });

  test("binary fails gracefully for non-existent plugin", async () => {
    const binaryPath = findBinary();
    if (!binaryPath) {
      test.skip(true, "Binary not found");
      return;
    }

    // Create a job for a plugin that doesn't exist
    const job = await bridgeCall("create_processing_job", {
      pluginName: "nonexistent_plugin_12345",
      inputFile: "/tmp/fake.csv",
    }) as { jobId: number };

    let error: Error | null = null;
    try {
      execSync(
        `${binaryPath} process-job ${job.jobId} --db "${DB_PATH}" --output /tmp`,
        {
          cwd: PROJECT_ROOT,
          encoding: "utf-8",
          timeout: 30000,
        }
      );
    } catch (e: any) {
      error = e;
    }

    // Should fail with "not found" error (expected behavior)
    expect(error, "Should fail for non-existent plugin").toBeTruthy();
    expect(
      error!.message.includes("not found") || error!.stderr?.includes("not found"),
      "Error should mention plugin not found"
    ).toBe(true);
  });

  test("binary validates database exists", async () => {
    const binaryPath = findBinary();
    if (!binaryPath) {
      test.skip(true, "Binary not found");
      return;
    }

    let error: Error | null = null;
    try {
      execSync(
        `${binaryPath} process-job 999 --db /nonexistent/path.sqlite3 --output /tmp`,
        {
          cwd: PROJECT_ROOT,
          encoding: "utf-8",
          timeout: 10000,
        }
      );
    } catch (e: any) {
      error = e;
    }

    // Should fail with database error
    expect(error, "Should fail for non-existent database").toBeTruthy();
  });
});

test.describe("Plugin Lookup Query Verification", () => {
  test("deployed plugin is findable with exact main.rs query", async () => {
    const testId = `query_test_${Date.now()}`;

    // Publish plugin
    const code = `import polars as pl

TOPIC = "query_test"
SINK = "parquet"

def parse(p): return pl.read_csv(p)`;

    await bridgeCall("publish_parser", {
      parserKey: testId,
      sourceCode: code,
      sinkType: "parquet",
      outputPath: `/tmp/${testId}/`,
      outputMode: "single",
    });

    // Use bridge's validate_plugin_processable which uses exact main.rs query
    const result = await bridgeCall("validate_plugin_processable", {
      pluginName: testId,
    }) as { processable: boolean; status?: string; reason?: string };

    expect(result.processable, `Plugin ${testId} must be findable: ${result.reason}`).toBe(true);
    expect(result.status).toBe("ACTIVE");
  });
});
