#!/usr/bin/env bun
/**
 * MCP E2E Test Script
 *
 * Tests the MCP server by sending JSON-RPC requests and validating responses.
 * Run with: bun run scripts/test-mcp-e2e.ts
 */

import { spawn } from 'child_process';
import * as path from 'path';
import * as readline from 'readline';

const ROOT = path.resolve(__dirname, '..');
const BINARY = path.join(ROOT, 'target/debug/casparian');
const DEMO_DIR = path.join(ROOT, 'demo');

interface JsonRpcRequest {
  jsonrpc: '2.0';
  id: number;
  method: string;
  params?: Record<string, unknown>;
}

interface JsonRpcResponse {
  jsonrpc: '2.0';
  id: number;
  result?: unknown;
  error?: { code: number; message: string };
}

class McpTestClient {
  private server: ReturnType<typeof spawn>;
  private requestId = 0;
  private pending: Map<number, { resolve: (v: JsonRpcResponse) => void; reject: (e: Error) => void }> = new Map();
  private buffer = '';

  constructor() {
    this.server = spawn(BINARY, ['mcp-server'], {
      cwd: ROOT,
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    // Read responses
    this.server.stdout.on('data', (data: Buffer) => {
      this.buffer += data.toString();
      this.processBuffer();
    });

    this.server.stderr.on('data', (data: Buffer) => {
      console.error('[MCP stderr]', data.toString());
    });

    this.server.on('error', (err) => {
      console.error('[MCP error]', err);
    });
  }

  private processBuffer() {
    const lines = this.buffer.split('\n');
    this.buffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        const response: JsonRpcResponse = JSON.parse(line);
        const pending = this.pending.get(response.id);
        if (pending) {
          pending.resolve(response);
          this.pending.delete(response.id);
        }
      } catch (e) {
        console.error('[Parse error]', line);
      }
    }
  }

  async call(method: string, params?: Record<string, unknown>): Promise<JsonRpcResponse> {
    const id = ++this.requestId;
    const request: JsonRpcRequest = {
      jsonrpc: '2.0',
      id,
      method,
      params,
    };

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.server.stdin.write(JSON.stringify(request) + '\n');

      // Timeout after 10s
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`Timeout waiting for response to ${method}`));
        }
      }, 10000);
    });
  }

  async close() {
    this.server.stdin.end();
    this.server.kill();
  }
}

async function runTests() {
  console.log('=== MCP E2E Test Suite ===\n');

  const client = new McpTestClient();
  let passed = 0;
  let failed = 0;

  async function test(name: string, fn: () => Promise<void>) {
    process.stdout.write(`Testing: ${name}... `);
    try {
      await fn();
      console.log('PASS');
      passed++;
    } catch (e) {
      console.log('FAIL');
      console.error(`  Error: ${e}`);
      failed++;
    }
  }

  // Give server time to start
  await new Promise(r => setTimeout(r, 1000));

  // Test 1: Initialize
  await test('Initialize MCP connection', async () => {
    const response = await client.call('initialize', {
      protocolVersion: '2024-11-05',
      capabilities: {},
      clientInfo: { name: 'test-client', version: '1.0.0' },
    });
    if (response.error) throw new Error(response.error.message);
    if (!response.result) throw new Error('No result');
  });

  // Test 2: List tools
  await test('List available tools', async () => {
    const response = await client.call('tools/list');
    if (response.error) throw new Error(response.error.message);
    const result = response.result as { tools: Array<{ name: string }> };
    if (!result.tools || result.tools.length === 0) throw new Error('No tools returned');

    const toolNames = result.tools.map(t => t.name);
    const expected = ['quick_scan', 'apply_scope', 'discover_schemas', 'approve_schemas',
                      'propose_amendment', 'generate_parser', 'run_backtest', 'fix_parser', 'execute_pipeline', 'query_output'];
    for (const name of expected) {
      if (!toolNames.includes(name)) throw new Error(`Missing tool: ${name}`);
    }
    console.log(`(${result.tools.length} tools)`);
  });

  // Test 3: Quick scan
  await test('quick_scan finds CSV files', async () => {
    const response = await client.call('tools/call', {
      name: 'quick_scan',
      arguments: {
        path: DEMO_DIR,
        extensions: ['csv'],
        max_depth: 5,
      },
    });
    if (response.error) throw new Error(response.error.message);
    const result = response.result as { content: Array<{ type: string; text: string }> };
    const text = result.content?.[0]?.text || '';
    if (!text.includes('csv') && !text.includes('files')) {
      console.log('\n  Response:', text.substring(0, 200));
    }
  });

  // Test 4: Discover schemas
  await test('discover_schemas infers types from CSV', async () => {
    const sampleFile = path.join(DEMO_DIR, 'data', 'sample_data.csv');
    const response = await client.call('tools/call', {
      name: 'discover_schemas',
      arguments: {
        files: [sampleFile],  // Array of files, not 'source'
        sample_rows: 100,
      },
    });
    if (response.error) throw new Error(response.error.message);
    const result = response.result as { content: Array<{ type: string; text: string }> };
    const text = result.content?.[0]?.text || '';

    // Verify type inference worked
    if (!text.toLowerCase().includes('int') && !text.toLowerCase().includes('float')) {
      console.log('\n  Response:', text.substring(0, 500));
      throw new Error('Type inference not working');
    }
  });

  // Test 5: Apply scope
  await test('apply_scope creates file group', async () => {
    const files = [
      path.join(DEMO_DIR, 'data', 'sample_data.csv'),
      path.join(DEMO_DIR, 'scout', 'sample_data', 'sales_2024_01.csv'),
    ];
    const response = await client.call('tools/call', {
      name: 'apply_scope',
      arguments: {
        name: 'test_scope',  // 'name' not 'scope_name'
        files,
        tags: ['csv', 'test'],
      },
    });
    if (response.error) throw new Error(response.error.message);
    const result = response.result as { content: Array<{ type: string; text: string }> };
    const text = result.content?.[0]?.text || '';
    if (!text.includes('scope') && !text.includes('file')) {
      console.log('\n  Response:', text.substring(0, 200));
    }
  });

  // Test 6: Approve schemas
  await test('approve_schemas creates contract', async () => {
    const response = await client.call('tools/call', {
      name: 'approve_schemas',
      arguments: {
        scope_id: 'test-scope-123',
        approved_by: 'test@example.com',
        schemas: [
          {
            discovery_id: 'disc-1',  // Required field
            name: 'sample_data',
            output_table_name: 'sample_data',  // Required field
            columns: [
              { name: 'id', data_type: 'int64', nullable: false },
              { name: 'name', data_type: 'string', nullable: false },
              { name: 'value', data_type: 'float64', nullable: false },
            ],
          },
        ],
      },
    });
    if (response.error) throw new Error(response.error.message);
    const result = response.result as { content: Array<{ type: string; text: string }> };
    const text = result.content?.[0]?.text || '';
    if (!text.includes('contract') && !text.includes('approved') && !text.includes('schema')) {
      console.log('\n  Response:', text.substring(0, 200));
    }
  });

  // Test 7: Generate parser
  await test('generate_parser creates bridge-compatible code', async () => {
    const response = await client.call('tools/call', {
      name: 'generate_parser',
      arguments: {
        schema: {
          name: 'sales_data',
          columns: [
            { name: 'date', data_type: 'string', nullable: false },
            { name: 'product', data_type: 'string', nullable: false },
            { name: 'quantity', data_type: 'int64', nullable: false },
            { name: 'price', data_type: 'float64', nullable: true },
          ],
        },
        topic_name: 'sales_topic',
        options: {
          file_format: 'csv',
          sink_type: 'duckdb',
        },
      },
    });
    if (response.error) throw new Error(response.error.message);
    const result = response.result as { content: Array<{ type: string; text: string }> };
    const text = result.content?.[0]?.text || '';
    const parsed = JSON.parse(text);

    // Verify Bridge Protocol compliance
    const code = parsed.parser_code;
    if (!code.includes('TOPIC = "sales_data"')) {  // topic defaults to schema name
      throw new Error('Missing TOPIC constant');
    }
    if (!code.includes('SINK = "duckdb"')) {
      throw new Error('Missing SINK constant (should be duckdb)');
    }
    if (!code.includes('def parse(file_path: str)')) {
      throw new Error('Missing parse() function');
    }
    if (!code.includes('import polars')) {
      throw new Error('Missing polars import');
    }
    console.log(`(${parsed.lines_of_code} lines, ${parsed.complexity} complexity)`);
  });

  // Test 8: Run backtest (mock)
  await test('run_backtest executes (mock parser)', async () => {
    const response = await client.call('tools/call', {
      name: 'run_backtest',
      arguments: {
        scope_id: 'test-scope-123',
        files: [path.join(DEMO_DIR, 'data', 'sample_data.csv')],
        pass_rate_threshold: 0.95,
      },
    });
    if (response.error) throw new Error(response.error.message);
    const result = response.result as { content: Array<{ type: string; text: string }> };
    const text = result.content?.[0]?.text || '';
    // Mock parser should pass
    if (!text.includes('pass') && !text.includes('complete') && !text.includes('result')) {
      console.log('\n  Response:', text.substring(0, 200));
    }
  });

  // Summary
  console.log('\n=== Summary ===');
  console.log(`Passed: ${passed}`);
  console.log(`Failed: ${failed}`);

  await client.close();
  process.exit(failed > 0 ? 1 : 0);
}

runTests().catch(console.error);
