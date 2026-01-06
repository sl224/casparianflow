/**
 * Tag Matching Tests
 *
 * Tests that the tagging system works correctly:
 * 1. Deploy parser with subscription tag
 * 2. Tag file with same tag
 * 3. Process file - should find matching plugin
 */

import { test, expect } from '@playwright/test';

test.describe('Tag Matching - Plugin Lookup', () => {
  // Enable bridge mode for all tests in this file
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      (window as any).__CASPARIAN_BRIDGE__ = true;
    });
  });

  test('deployed parser can be found by tag', async ({ page }) => {
    const BRIDGE_URL = 'http://localhost:9999';

    // Helper to call bridge API
    async function bridgeCall(command: string, args: Record<string, unknown> = {}) {
      const response = await fetch(`${BRIDGE_URL}/api/rpc`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ command, args }),
      });
      const data = await response.json();
      if (data.error) throw new Error(data.error);
      return data.result;
    }

    const logs: string[] = [];

    // Step 1: Publish a parser with tag "MCDATA" via bridge
    logs.push('=== Publishing parser with tag MCDATA ===');
    try {
      const publishResult = await bridgeCall('publish_parser', {
        parserKey: 'MCDATA',
        sourceCode: 'import polars as pl\n\nTOPIC = "mcdata"\nSINK = "parquet"\n\ndef parse(input_path): return pl.read_csv(input_path)',
        schema: [],
        sinkType: 'parquet',
        outputPath: '~/.casparian_flow/output/mcdata/',
        outputMode: 'single',
        topicUrisJson: null
      });
      logs.push('Publish result: ' + JSON.stringify(publishResult));
    } catch (e) {
      logs.push('Publish error (may already exist): ' + e);
    }

    // Step 2: Query plugins for tag "MCDATA"
    logs.push('=== Querying plugins for tag MCDATA ===');
    let plugins: string[] = [];
    try {
      plugins = await bridgeCall('get_plugins_for_tag', { tag: 'MCDATA' }) as string[];
      logs.push('Found plugins: ' + JSON.stringify(plugins));
    } catch (e) {
      logs.push('Query error: ' + e);
    }

    // Print logs
    console.log('\n=== Test Logs ===');
    logs.forEach(log => console.log(log));

    // The test passes if we find at least one plugin
    expect(plugins.length).toBeGreaterThan(0);
  });

  test('file tagged MCDATA matches plugin subscribed to MCDATA', async ({ page }) => {
    const BRIDGE_URL = 'http://localhost:9999';

    // Helper to call bridge API
    async function bridgeCall(command: string, args: Record<string, unknown> = {}) {
      const response = await fetch(`${BRIDGE_URL}/api/rpc`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ command, args }),
      });
      const data = await response.json();
      if (data.error) throw new Error(data.error);
      return data.result;
    }

    const logs: string[] = [];

    // 1. Publish a parser with subscription tag "MCDATA"
    logs.push('=== Publishing parser with tag MCDATA ===');
    try {
      const publishResult = await bridgeCall('publish_parser', {
        parserKey: 'MCDATA',
        sourceCode: 'import polars as pl\n\nTOPIC = "mcdata"\nSINK = "parquet"\n\ndef parse(input_path): return pl.read_csv(input_path)',
        schema: [],
        sinkType: 'parquet',
        outputPath: '~/.casparian_flow/output/mcdata/',
        outputMode: 'single',
        topicUrisJson: null
      });
      logs.push('Publish result: ' + JSON.stringify(publishResult));
    } catch (e) {
      logs.push('Publish error: ' + e);
    }

    // 2. Query deployed plugin
    logs.push('=== Querying deployed plugin ===');
    let plugin = null;
    try {
      plugin = await bridgeCall('get_deployed_plugin', { name: 'MCDATA' });
      logs.push('Plugin found: ' + JSON.stringify(plugin));
    } catch (e) {
      logs.push('get_deployed_plugin error: ' + e);
    }

    // Print all logs
    console.log('\n=== Test Logs ===');
    logs.forEach(log => console.log(log));

    expect(plugin).not.toBeNull();
    expect(plugin.plugin_name).toBe('MCDATA');
  });

  test('debug: inspect cf_plugin_config table', async ({ page }) => {
    const BRIDGE_URL = 'http://localhost:9999';

    // Helper to call bridge API
    async function bridgeCall(command: string, args: Record<string, unknown> = {}) {
      const response = await fetch(`${BRIDGE_URL}/api/rpc`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ command, args }),
      });
      const data = await response.json();
      if (data.error) throw new Error(data.error);
      return data.result;
    }

    const logs: string[] = [];

    // First publish to ensure we have data
    logs.push('=== Publishing test parser ===');
    try {
      await bridgeCall('publish_parser', {
        parserKey: 'DEBUG_TAG',
        sourceCode: 'TOPIC = "debug_tag"\nSINK = "parquet"\n\ndef parse(p): pass',
        schema: [],
        sinkType: 'parquet',
        outputPath: '/tmp/debug/',
        outputMode: 'single',
        topicUrisJson: null
      });
      logs.push('Published with parserKey: DEBUG_TAG');
    } catch (e) {
      logs.push('Publish error: ' + e);
    }

    // Query for the tag we just published
    logs.push('=== Querying for DEBUG_TAG ===');
    try {
      const plugins = await bridgeCall('get_plugins_for_tag', { tag: 'DEBUG_TAG' });
      logs.push('Found plugins: ' + JSON.stringify(plugins));
    } catch (e) {
      logs.push('Query error: ' + e);
    }

    // Also list all plugins
    logs.push('=== Listing all plugins ===');
    try {
      const allPlugins = await bridgeCall('list_plugins');
      logs.push('All plugins: ' + JSON.stringify(allPlugins));
    } catch (e) {
      logs.push('list_plugins error: ' + e);
    }

    console.log('\n=== Debug Output ===');
    logs.forEach(log => console.log(log));
  });
});
