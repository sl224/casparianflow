/**
 * MCDATA Parser E2E Test
 *
 * Tests parsing a multi-record MCDATA file with DEMUX output.
 * Uses bridge API for backend calls, UI for interaction verification.
 *
 * Test file: test-fixtures/scout/sample_data/invoice_batch.mcdata
 * Format: H|header, L|line_item, T|total records
 */

import { test, expect } from "@playwright/test";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Use fixture file (always exists)
const MCDATA_FILE = path.resolve(
  __dirname,
  "../test-fixtures/scout/sample_data/invoice_batch.mcdata"
);

const BRIDGE_URL = "http://localhost:9999";

// DEMUX parser for invoice MCDATA format
const MCDATA_PARSER_CODE = `import polars as pl
from typing import Dict

def parse(input_path: str) -> Dict[str, pl.DataFrame]:
    """Parse MCDATA invoice file into separate tables."""
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

    return {
        'headers': pl.DataFrame(headers),
        'line_items': pl.DataFrame(line_items),
        'totals': pl.DataFrame(totals)
    }
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

test.describe("MCDATA Parser", () => {
  test.setTimeout(120000); // 2 minutes

  test.beforeEach(async ({ page }) => {
    // Enable bridge mode
    await page.addInitScript(() => {
      (window as any).__CASPARIAN_BRIDGE__ = true;
    });
  });

  test("parse multi-record MCDATA file with DEMUX output", async ({ page }) => {
    // Step 1: Create parser with DEMUX code
    console.log("Creating MCDATA parser...");
    const parser = await bridgeCall("parser_lab_create_parser", {
      name: `mcdata_invoice_${Date.now()}`,
      filePattern: "*.mcdata",
    });
    expect(parser.id).toBeTruthy();

    // Update with parser code
    await bridgeCall("parser_lab_update_parser", {
      parser: {
        ...parser,
        sourceCode: MCDATA_PARSER_CODE,
      },
    });

    // Add test file
    const testFile = await bridgeCall("parser_lab_add_test_file", {
      parserId: parser.id,
      filePath: MCDATA_FILE,
    });
    console.log(`Added test file: ${testFile.fileName}`);

    // Step 2: Validate via bridge
    console.log("Validating parser...");
    const validated = await bridgeCall("parser_lab_validate_parser", {
      parserId: parser.id,
      testFileId: testFile.id,
    });

    expect(validated.validationStatus).toBe("valid");
    expect(validated.detectedTopicsJson).toBeTruthy();

    const topics = JSON.parse(validated.detectedTopicsJson);
    console.log(`Detected ${topics.length} topics:`, topics);
    expect(topics).toContain("headers");
    expect(topics).toContain("line_items");
    expect(topics).toContain("totals");

    // Step 3: Open in UI and verify
    console.log("Verifying in UI...");
    await page.goto("/");
    await page.click('button:has-text("PARSER LAB")');

    // Find and click the parser
    const parserRow = page.locator(`.file-row:has-text("mcdata_invoice")`).first();
    await expect(parserRow).toBeVisible({ timeout: 5000 });
    await parserRow.click();

    await expect(page.locator(".file-editor")).toBeVisible({ timeout: 5000 });
    await expect(page.locator(".badge.valid")).toBeVisible({ timeout: 5000 });

    // Check multi-output badge shows 3 tables
    const multiBadge = page.locator('.badge:has-text("3 tables")');
    await expect(multiBadge).toBeVisible();

    // Step 4: Verify output sections show all 3 tables
    // The validation output shows collapsible sections for each topic
    await expect(page.locator('button:has-text("headers")')).toBeVisible();
    await expect(page.locator('button:has-text("line_items")')).toBeVisible();
    await expect(page.locator('button:has-text("totals")')).toBeVisible();

    console.log("MCDATA parser test passed");
  });

  test("deploy MCDATA parser and verify plugin", async ({ page }) => {
    // Create and validate parser
    const parser = await bridgeCall("parser_lab_create_parser", {
      name: `mcdata_deploy_${Date.now()}`,
      filePattern: "*.mcdata",
    });

    await bridgeCall("parser_lab_update_parser", {
      parser: { ...parser, sourceCode: MCDATA_PARSER_CODE },
    });

    const testFile = await bridgeCall("parser_lab_add_test_file", {
      parserId: parser.id,
      filePath: MCDATA_FILE,
    });

    const validated = await bridgeCall("parser_lab_validate_parser", {
      parserId: parser.id,
      testFileId: testFile.id,
    });
    expect(validated.validationStatus).toBe("valid");

    // Deploy via bridge
    const subscriptionTag = `MCDATA_TEST_${Date.now()}`;
    console.log(`Deploying as ${subscriptionTag}...`);

    const deployResult = await bridgeCall("publish_parser", {
      parserKey: subscriptionTag,
      sourceCode: MCDATA_PARSER_CODE,
      schema: [],
      sinkType: "parquet",
      outputPath: `~/.casparian_flow/output/${subscriptionTag}/`,
      outputMode: "multi",
      topicUrisJson: JSON.stringify({
        headers: { type: "parquet", path: `~/.casparian_flow/output/${subscriptionTag}/headers/` },
        line_items: { type: "sqlite", path: `~/.casparian_flow/output/${subscriptionTag}/line_items.db` },
        totals: { type: "parquet", path: `~/.casparian_flow/output/${subscriptionTag}/totals/` },
      }),
    });

    expect(deployResult.success).toBe(true);
    console.log(`Deployed: ${deployResult.pluginName}`);

    // Verify plugin exists
    const plugin = await bridgeCall("get_deployed_plugin", {
      name: subscriptionTag,
    });
    expect(plugin).toBeTruthy();
    expect(plugin.status).toBe("ACTIVE");

    // Verify topic config
    const topicConfig = await bridgeCall("get_topic_config", {
      pluginName: subscriptionTag,
    });
    expect(topicConfig.length).toBe(3);

    const sinkTypes = topicConfig.map((t: any) => t.sink_type);
    expect(sinkTypes).toContain("parquet");
    expect(sinkTypes).toContain("sqlite");

    console.log("Deploy test passed");
  });
});
