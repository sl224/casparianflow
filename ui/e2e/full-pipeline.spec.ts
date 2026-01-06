/**
 * Full Pipeline Integration Test
 *
 * This test verifies the complete pipeline from Parser Lab to job execution.
 * It catches database path and schema mismatches between components.
 *
 * CRITICAL: This test uses:
 * - Bridge for Parser Lab operations (writes to ~/.casparian_flow/scout.db)
 * - Bridge for plugin deployment (writes to ~/.casparian_flow/casparian_flow.sqlite3)
 * - Bridge's run_parser_job which executes actual Python code
 * - Output verification that checks real files on disk
 *
 * If the bridge schema doesn't match lib.rs schema, this test WILL catch it.
 */

import { test, expect } from "@playwright/test";
import * as path from "path";
import { fileURLToPath } from "url";
import { existsSync, readFileSync } from "fs";
import { homedir } from "os";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const BRIDGE_URL = "http://localhost:9999";
const HOME = homedir();
const CF_DIR = path.join(HOME, ".casparian_flow");

// Test fixture: MCDATA file
const MCDATA_FILE = path.resolve(
  __dirname,
  "../test-fixtures/scout/sample_data/invoice_batch.mcdata"
);

// DEMUX parser for invoice MCDATA format
const MCDATA_PARSER_CODE = `import polars as pl
from casparian_types import Output

TOPIC = "mcdata_invoice"

def parse(input_path: str) -> list[Output]:
    """Parse MCDATA invoice file into separate outputs."""
    headers = []
    line_items = []
    totals = []

    with open(input_path, 'r') as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith('#'):
                continue

            parts = line.split('|')
            record_type = parts[0]

            if record_type == 'H':
                headers.append({
                    'invoice_id': parts[1],
                    'date': parts[2],
                    'customer': parts[3],
                    'terms': parts[4]
                })
            elif record_type == 'L':
                line_items.append({
                    'invoice_id': parts[1],
                    'line_num': parts[2],
                    'description': parts[3],
                    'qty': int(parts[4]),
                    'unit_price': float(parts[5]),
                    'total': float(parts[6])
                })
            elif record_type == 'T':
                totals.append({
                    'invoice_id': parts[1],
                    'subtotal': float(parts[2]),
                    'tax': float(parts[3]),
                    'grand_total': float(parts[4])
                })

    return [
        Output('headers', pl.DataFrame(headers), 'parquet'),
        Output('line_items', pl.DataFrame(line_items), 'parquet'),
        Output('totals', pl.DataFrame(totals), 'parquet'),
    ]
`;

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

test.describe("Full Pipeline Integration", () => {
  test.setTimeout(180000); // 3 minutes

  test.beforeEach(async ({ page }) => {
    // Enable bridge mode
    await page.addInitScript(() => {
      (window as any).__CASPARIAN_BRIDGE__ = true;
    });
  });

  test("complete pipeline: create → publish → process → verify output", async ({
    page,
  }) => {
    const testId = `pipeline_test_${Date.now()}`;
    console.log(`\n=== Full Pipeline Test: ${testId} ===\n`);

    // ========================================================================
    // Step 1: Create parser in Parser Lab (writes to scout.db)
    // ========================================================================
    console.log("Step 1: Creating MCDATA parser in Parser Lab...");
    const parser = await bridgeCall("parser_lab_create_parser", {
      name: testId,
      filePattern: "*.mcdata",
    });
    expect(parser.id).toBeTruthy();

    // Update with parser code
    await bridgeCall("parser_lab_update_parser", {
      parser: {
        ...parser,
        sourceCode: MCDATA_PARSER_CODE,
        outputMode: "multi",
      },
    });

    // Add test file
    const testFile = await bridgeCall("parser_lab_add_test_file", {
      parserId: parser.id,
      filePath: MCDATA_FILE,
    });
    console.log(`  - Parser created: ${parser.id}`);
    console.log(`  - Test file added: ${testFile.fileName}`);

    // ========================================================================
    // Step 2: Validate parser (runs actual Python)
    // ========================================================================
    console.log("\nStep 2: Validating parser...");
    const validated = await bridgeCall("parser_lab_validate_parser", {
      parserId: parser.id,
      testFileId: testFile.id,
    });

    expect(validated.validationStatus).toBe("valid");
    expect(validated.outputMode).toBe("multi");
    expect(validated.detectedTopicsJson).toBeTruthy();

    const topics = JSON.parse(validated.detectedTopicsJson);
    console.log(`  - Validation: ${validated.validationStatus}`);
    console.log(`  - Detected topics: ${topics.join(", ")}`);
    expect(topics).toContain("headers");
    expect(topics).toContain("line_items");
    expect(topics).toContain("totals");

    // ========================================================================
    // Step 3: Publish parser as plugin (writes to casparian_flow.sqlite3)
    // ========================================================================
    console.log("\nStep 3: Publishing parser as plugin...");
    const outputBase = `~/.casparian_flow/output/${testId}`;

    const deployResult = await bridgeCall("publish_parser", {
      parserKey: testId,
      sourceCode: MCDATA_PARSER_CODE,
      schema: [],
      sinkType: "parquet",
      outputPath: outputBase,
      outputMode: "multi",
      topicUrisJson: JSON.stringify({
        headers: {
          type: "parquet",
          path: `${outputBase}/headers/`,
        },
        line_items: {
          type: "sqlite",
          path: `${outputBase}/line_items/`,
        },
        totals: {
          type: "parquet",
          path: `${outputBase}/totals/`,
        },
      }),
    });

    expect(deployResult.success).toBe(true);
    console.log(`  - Deployed as: ${deployResult.pluginName}`);

    // ========================================================================
    // Step 4: Verify plugin exists in Sentinel DB
    // ========================================================================
    console.log("\nStep 4: Verifying plugin in Sentinel DB...");
    const plugin = await bridgeCall("get_deployed_plugin", {
      name: testId,
    });
    expect(plugin).toBeTruthy();
    expect(plugin.status).toBe("ACTIVE");
    console.log(`  - Plugin status: ${plugin.status}`);

    // Verify topic config
    const topicConfig = await bridgeCall("get_topic_config", {
      pluginName: testId,
    });
    expect(topicConfig.length).toBe(3);

    const sinkTypes = topicConfig.map((t: any) => t.sink_type);
    expect(sinkTypes).toContain("parquet");
    expect(sinkTypes).toContain("sqlite");
    console.log(`  - Topic configs: ${topicConfig.length}`);
    console.log(
      `  - Sink types: ${topicConfig.map((t: any) => `${t.topic_name}=${t.sink_type}`).join(", ")}`
    );

    // ========================================================================
    // Step 5: Run parser job (executes actual Python, writes to disk)
    // ========================================================================
    console.log("\nStep 5: Running parser job...");
    const jobResult = await bridgeCall("run_parser_job", {
      pluginName: testId,
      inputFilePath: MCDATA_FILE,
    });

    expect(jobResult.success).toBe(true);
    console.log(`  - Job result: ${jobResult.message}`);
    console.log(
      `  - Outputs: ${jobResult.outputs.map((o: any) => `${o.topic}(${o.rows} rows)`).join(", ")}`
    );

    // Verify we got all 3 outputs
    expect(jobResult.outputs.length).toBe(3);

    // ========================================================================
    // Step 6: Verify output files exist on disk
    // ========================================================================
    console.log("\nStep 6: Verifying output files...");
    const outputVerification = await bridgeCall("verify_output_files", {
      pluginName: testId,
    });

    console.log("  Output verification:");
    for (const output of outputVerification) {
      console.log(
        `    - ${output.topic}: ${output.sinkType} at ${output.path}`
      );
      console.log(
        `      exists=${output.exists}, hasData=${output.hasData}, rows=${output.rowCount || "N/A"}`
      );
    }

    // All outputs should exist and have data
    for (const output of outputVerification) {
      expect(output.exists, `Output for ${output.topic} should exist`).toBe(
        true
      );
      expect(output.hasData, `Output for ${output.topic} should have data`).toBe(
        true
      );
    }

    // ========================================================================
    // Step 7: Verify in UI (optional - ensures frontend works)
    // ========================================================================
    console.log("\nStep 7: Verifying in UI...");
    await page.goto("/");
    await page.click('button:has-text("PARSER LAB")');

    // Find the parser we created
    const parserRow = page.locator(`.file-row:has-text("${testId}")`).first();
    await expect(parserRow).toBeVisible({ timeout: 5000 });
    console.log("  - Parser visible in Parser Lab list");

    console.log("\n=== Full Pipeline Test PASSED ===\n");
  });

  test("schema compatibility: Sentinel DB has all required tables", async () => {
    // This test verifies the bridge creates the same tables as lib.rs
    console.log("Verifying Sentinel DB schema...");

    // cf_plugin_manifest
    const manifestCols = await bridgeCall("get_table_columns", {
      tableName: "cf_plugin_manifest",
    });
    const manifestColNames = manifestCols.map((c: any) => c.name);
    expect(manifestColNames).toContain("plugin_name");
    expect(manifestColNames).toContain("source_code");
    expect(manifestColNames).toContain("source_hash");
    expect(manifestColNames).toContain("status");
    console.log("  - cf_plugin_manifest: OK");

    // cf_plugin_config
    const configCols = await bridgeCall("get_table_columns", {
      tableName: "cf_plugin_config",
    });
    const configColNames = configCols.map((c: any) => c.name);
    expect(configColNames).toContain("plugin_name");
    expect(configColNames).toContain("subscription_tags");
    expect(configColNames).toContain("enabled");
    console.log("  - cf_plugin_config: OK");

    // cf_topic_config - CRITICAL: must have sink_type
    const topicCols = await bridgeCall("get_table_columns", {
      tableName: "cf_topic_config",
    });
    const topicColNames = topicCols.map((c: any) => c.name);
    expect(topicColNames).toContain("plugin_name");
    expect(topicColNames).toContain("topic_name");
    expect(topicColNames).toContain("uri");
    expect(topicColNames).toContain("sink_type"); // This was missing before!
    console.log("  - cf_topic_config: OK (including sink_type)");

    // cf_processing_queue
    const queueCols = await bridgeCall("get_table_columns", {
      tableName: "cf_processing_queue",
    });
    const queueColNames = queueCols.map((c: any) => c.name);
    expect(queueColNames).toContain("plugin_name");
    expect(queueColNames).toContain("input_file");
    expect(queueColNames).toContain("status");
    console.log("  - cf_processing_queue: OK");

    console.log("\nSentinel DB schema verification PASSED");
  });

  test("database paths are correct", async () => {
    // This test verifies the bridge uses the correct database paths
    // SINGLE DATABASE: All tables in casparian_flow.sqlite3 (NO scout.db)
    console.log("Verifying database paths...");

    // SINGLE DATABASE at ~/.casparian_flow/casparian_flow.sqlite3
    const dbPath = path.join(CF_DIR, "casparian_flow.sqlite3");
    expect(existsSync(dbPath), "casparian_flow.sqlite3 should exist").toBe(true);
    console.log(`  - Database: ${dbPath} ✓`);

    // scout.db should NOT exist (we use single database)
    const oldScoutDbPath = path.join(CF_DIR, "scout.db");
    expect(existsSync(oldScoutDbPath), "scout.db should NOT exist").toBe(false);
    console.log(`  - No scout.db: ✓`);

    // Parsers directory should exist
    const parsersDir = path.join(CF_DIR, "parsers");
    expect(existsSync(parsersDir), "parsers directory should exist").toBe(true);
    console.log(`  - Parsers dir: ${parsersDir} ✓`);

    console.log("\nDatabase paths verification PASSED");
  });
});
