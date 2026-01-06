/**
 * Status Consistency E2E Test
 *
 * CRITICAL: Verifies all code paths use ACTIVE status for deployed plugins.
 *
 * Background: On 2025-01-05, plugins deployed via lib.rs had status='PENDING',
 * but job processor (main.rs:801) only queries for 'ACTIVE' or 'DEPLOYED'.
 * Result: "Plugin not found" even though plugin WAS in database.
 *
 * This test prevents that regression.
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

test.describe("Status Consistency", () => {
  test("deployed plugins have ACTIVE status", async () => {
    const testId = `status_test_${Date.now()}`;
    const code = `import polars as pl

TOPIC = "status_test"
SINK = "parquet"

def parse(input_path: str) -> pl.DataFrame:
    return pl.read_csv(input_path)`;

    // Publish via bridge
    await bridgeCall("publish_parser", {
      parserKey: testId,
      sourceCode: code,
      sinkType: "parquet",
      outputPath: `~/.casparian_flow/output/${testId}/`,
      outputMode: "single",
    });

    // Verify status via bridge validation command
    const result = await bridgeCall("validate_plugin_processable", {
      pluginName: testId,
    }) as { processable: boolean; status?: string; reason?: string };

    expect(result.processable, `Plugin ${testId} must be processable: ${result.reason}`).toBe(true);
    expect(result.status, "Status must be ACTIVE for job processor to find it").toBe("ACTIVE");
  });

  test("job processor can find deployed plugin using same query as main.rs", async () => {
    const testId = `findable_${Date.now()}`;
    const code = `import polars as pl

TOPIC = "findable"
SINK = "parquet"

def parse(input_path: str) -> pl.DataFrame:
    return pl.read_csv(input_path)`;

    // Publish
    await bridgeCall("publish_parser", {
      parserKey: testId,
      sourceCode: code,
      sinkType: "parquet",
      outputPath: `~/.casparian_flow/output/${testId}/`,
      outputMode: "single",
    });

    // Use bridge's validate_plugin_processable which uses exact main.rs query
    const result = await bridgeCall("validate_plugin_processable", {
      pluginName: testId,
    }) as { processable: boolean; status?: string; reason?: string };

    expect(result.processable, `Job processor must be able to find plugin: ${result.reason}`).toBe(true);
  });

  test("list_all_plugins shows deployed plugins with correct status", async () => {
    const testId = `list_test_${Date.now()}`;
    const code = `import polars as pl

TOPIC = "list_test"
SINK = "parquet"

def parse(p): return pl.read_csv(p)`;

    // Publish
    await bridgeCall("publish_parser", {
      parserKey: testId,
      sourceCode: code,
      sinkType: "parquet",
      outputPath: `/tmp/${testId}/`,
      outputMode: "single",
    });

    // List all plugins
    const plugins = await bridgeCall("list_all_plugins", {}) as Array<{
      plugin_name: string;
      status: string;
    }>;

    const ourPlugin = plugins.find(p => p.plugin_name === testId);
    expect(ourPlugin, `Plugin ${testId} should be in list`).toBeTruthy();
    expect(ourPlugin!.status).toBe("ACTIVE");
  });

  test("get_deployed_plugin returns correct details", async () => {
    const testId = `details_test_${Date.now()}`;
    const code = `import polars as pl

TOPIC = "details_test"
SINK = "parquet"

def parse(p): return pl.read_csv(p)`;

    // Publish
    await bridgeCall("publish_parser", {
      parserKey: testId,
      sourceCode: code,
      sinkType: "parquet",
      outputPath: `/tmp/${testId}/`,
      outputMode: "single",
    });

    // Get details
    const plugin = await bridgeCall("get_deployed_plugin", {
      name: testId,
    }) as { plugin_name: string; version: string; status: string } | null;

    expect(plugin, `Plugin ${testId} should exist`).toBeTruthy();
    expect(plugin!.plugin_name).toBe(testId);
    expect(plugin!.status).toBe("ACTIVE");
    expect(plugin!.version).toBeTruthy();
  });
});
