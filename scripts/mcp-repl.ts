#!/usr/bin/env bun
/**
 * MCP Interactive REPL
 *
 * A command-line interface for manually testing MCP tools step by step.
 * Run with: bun run scripts/mcp-repl.ts
 */

import { spawn } from 'child_process';
import * as path from 'path';
import * as readline from 'readline';
import * as fs from 'fs';

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

// State stored across commands
const state: {
  scopeId?: string;
  discoveryId?: string;
  contractId?: string;
  files?: string[];
  schema?: any;
  parserCode?: string;
} = {};

class McpClient {
  private server: ReturnType<typeof spawn>;
  private requestId = 0;
  private pending: Map<number, { resolve: (v: JsonRpcResponse) => void; reject: (e: Error) => void }> = new Map();
  private buffer = '';
  private initialized = false;

  constructor() {
    this.server = spawn(BINARY, ['mcp-server'], {
      cwd: ROOT,
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    this.server.stdout.on('data', (data: Buffer) => {
      this.buffer += data.toString();
      this.processBuffer();
    });

    this.server.stderr.on('data', (data: Buffer) => {
      // Filter out INFO logs, show only WARN/ERROR
      const lines = data.toString().split('\n');
      for (const line of lines) {
        if (line.includes('WARN') || line.includes('ERROR')) {
          console.error('\x1b[33m[MCP]\x1b[0m', line);
        }
      }
    });

    this.server.on('error', (err) => {
      console.error('\x1b[31m[MCP Error]\x1b[0m', err);
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
        // Ignore parse errors (could be debug output)
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

      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`Timeout waiting for response to ${method}`));
        }
      }, 30000);
    });
  }

  async initialize(): Promise<void> {
    if (this.initialized) return;

    const response = await this.call('initialize', {
      protocolVersion: '2024-11-05',
      capabilities: {},
      clientInfo: { name: 'mcp-repl', version: '1.0.0' },
    });

    if (response.error) {
      throw new Error(`Init failed: ${response.error.message}`);
    }
    this.initialized = true;
  }

  async callTool(name: string, args: Record<string, unknown>): Promise<any> {
    const response = await this.call('tools/call', { name, arguments: args });

    if (response.error) {
      throw new Error(response.error.message);
    }

    const result = response.result as { content: Array<{ type: string; text?: string }> };
    const text = result.content?.[0]?.text;

    if (text) {
      try {
        return JSON.parse(text);
      } catch {
        return text;
      }
    }
    return result;
  }

  close() {
    this.server.stdin.end();
    this.server.kill();
  }
}

// Pretty print JSON with colors
function prettyPrint(obj: any, indent = 0): void {
  const spaces = '  '.repeat(indent);

  if (typeof obj === 'string') {
    console.log(`${spaces}\x1b[32m"${obj}"\x1b[0m`);
  } else if (typeof obj === 'number') {
    console.log(`${spaces}\x1b[33m${obj}\x1b[0m`);
  } else if (typeof obj === 'boolean') {
    console.log(`${spaces}\x1b[35m${obj}\x1b[0m`);
  } else if (obj === null) {
    console.log(`${spaces}\x1b[90mnull\x1b[0m`);
  } else if (Array.isArray(obj)) {
    if (obj.length === 0) {
      console.log(`${spaces}[]`);
    } else {
      console.log(`${spaces}[`);
      obj.forEach((item, i) => {
        process.stdout.write(`${'  '.repeat(indent + 1)}`);
        if (typeof item === 'object' && item !== null) {
          console.log('{');
          Object.entries(item).forEach(([k, v]) => {
            process.stdout.write(`${'  '.repeat(indent + 2)}\x1b[36m${k}\x1b[0m: `);
            if (typeof v === 'object' && v !== null) {
              console.log(JSON.stringify(v));
            } else {
              prettyPrint(v, 0);
            }
          });
          console.log(`${'  '.repeat(indent + 1)}}${i < obj.length - 1 ? ',' : ''}`);
        } else {
          prettyPrint(item, 0);
        }
      });
      console.log(`${spaces}]`);
    }
  } else if (typeof obj === 'object') {
    console.log(`${spaces}{`);
    const entries = Object.entries(obj);
    entries.forEach(([key, value], i) => {
      process.stdout.write(`${'  '.repeat(indent + 1)}\x1b[36m${key}\x1b[0m: `);
      if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
        console.log(JSON.stringify(value, null, 2).split('\n').map((l, i) => i === 0 ? l : '  '.repeat(indent + 2) + l).join('\n'));
      } else if (Array.isArray(value) && value.length > 3) {
        console.log(`[...${value.length} items]`);
      } else {
        prettyPrint(value, 0);
      }
    });
    console.log(`${spaces}}`);
  }
}

// Command handlers
const commands: Record<string, (client: McpClient, args: string[]) => Promise<void>> = {
  async help() {
    console.log(`
\x1b[1m=== MCP Interactive REPL ===\x1b[0m

\x1b[33mDiscovery Phase:\x1b[0m
  scan [path]         - Quick scan a directory (default: demo/)
  scope <name>        - Create a scope with discovered files

\x1b[33mSchema Phase:\x1b[0m
  discover            - Discover schemas from scoped files
  approve             - Approve discovered schemas as contracts

\x1b[33mParser Phase:\x1b[0m
  generate [name]     - Generate parser from discovered schema
  parser <code>       - Set parser code manually (or 'sample' for demo)
  backtest            - Run backtest against scoped files
  fix                 - Get fix suggestions for failures

\x1b[33mExecution Phase:\x1b[0m
  execute [format]    - Execute pipeline (parquet|csv|duckdb)
  query <file>        - Query output file

\x1b[33mUtility:\x1b[0m
  state               - Show current state (scope, contract, etc.)
  tools               - List available MCP tools
  raw <tool> <json>   - Call any tool with raw JSON args
  quit                - Exit REPL

\x1b[90mTip: Run commands in order: scan -> scope -> discover -> approve -> generate -> backtest -> execute\x1b[0m
`);
  },

  async scan(client, args) {
    const scanPath = args[0] || DEMO_DIR;
    console.log(`\x1b[90mScanning ${scanPath}...\x1b[0m`);

    const result = await client.callTool('quick_scan', {
      path: scanPath,
      max_depth: 3,
      include_hidden: false,
    });

    console.log(`\n\x1b[1mFound ${result.file_count} files (${formatBytes(result.total_size)})\x1b[0m\n`);

    // Show by extension
    for (const [ext, files] of Object.entries(result.by_extension || {})) {
      const fileList = files as any[];
      console.log(`  \x1b[36m.${ext}\x1b[0m: ${fileList.length} files`);
      fileList.slice(0, 3).forEach((f: any) => {
        console.log(`    - ${path.basename(f.path)} (${formatBytes(f.size)})`);
      });
      if (fileList.length > 3) {
        console.log(`    \x1b[90m... and ${fileList.length - 3} more\x1b[0m`);
      }
    }

    // Store CSV files for later use
    const csvFiles = (result.by_extension?.csv || []) as any[];
    state.files = csvFiles.map((f: any) => f.path);

    if (csvFiles.length > 0) {
      console.log(`\n\x1b[32m${csvFiles.length} CSV files stored. Run 'scope <name>' to create a scope.\x1b[0m`);
    }
  },

  async scope(client, args) {
    const name = args[0] || 'test_scope';

    if (!state.files || state.files.length === 0) {
      console.log('\x1b[31mNo files scanned. Run "scan" first.\x1b[0m');
      return;
    }

    console.log(`\x1b[90mCreating scope "${name}" with ${state.files.length} files...\x1b[0m`);

    const result = await client.callTool('apply_scope', {
      name,
      files: state.files,
      tags: ['csv', 'test'],
    });

    state.scopeId = result.scope?.scope_id;

    console.log(`\n\x1b[1mScope created:\x1b[0m`);
    console.log(`  ID: \x1b[33m${state.scopeId}\x1b[0m`);
    console.log(`  Files: ${result.file_count}`);
    console.log(`  Size: ${formatBytes(result.total_size)}`);

    if (result.files_skipped > 0) {
      console.log(`  \x1b[33mSkipped: ${result.files_skipped}\x1b[0m`);
    }

    console.log(`\n\x1b[32mRun 'discover' to analyze schemas.\x1b[0m`);
  },

  async discover(client) {
    if (!state.files || state.files.length === 0) {
      console.log('\x1b[31mNo files in scope. Run "scan" and "scope" first.\x1b[0m');
      return;
    }

    console.log(`\x1b[90mDiscovering schemas from ${state.files.length} files...\x1b[0m`);

    const result = await client.callTool('discover_schemas', {
      files: state.files,
      max_rows: 1000,
    });

    console.log(`\n\x1b[1mAnalyzed ${result.files_analyzed} files:\x1b[0m\n`);

    for (const schema of result.schemas || []) {
      state.discoveryId = schema.discovery_id;
      state.schema = schema;

      console.log(`  \x1b[36m${schema.name}\x1b[0m (${schema.rows_analyzed} rows, ${Math.round(schema.confidence * 100)}% confidence)`);
      console.log(`  Columns:`);

      for (const col of schema.columns || []) {
        const nullable = col.null_percentage > 0 ? ` (${col.null_percentage.toFixed(1)}% null)` : '';
        const ambiguous = col.is_ambiguous ? ' \x1b[33m[ambiguous]\x1b[0m' : '';
        console.log(`    - \x1b[32m${col.name}\x1b[0m: ${col.data_type}${nullable}${ambiguous}`);
      }

      if (schema.warnings?.length > 0) {
        console.log(`  \x1b[33mWarnings:\x1b[0m`);
        schema.warnings.forEach((w: string) => console.log(`    - ${w}`));
      }
      console.log();
    }

    if (result.failed_files?.length > 0) {
      console.log(`\x1b[31mFailed files:\x1b[0m`);
      result.failed_files.forEach((f: string) => console.log(`  - ${f}`));
    }

    console.log(`\x1b[32mRun 'approve' to lock the schema as a contract.\x1b[0m`);
  },

  async approve(client) {
    if (!state.schema || !state.scopeId) {
      console.log('\x1b[31mNo schema discovered. Run "discover" first.\x1b[0m');
      return;
    }

    console.log(`\x1b[90mApproving schema "${state.schema.name}"...\x1b[0m`);

    // Convert discovered schema to approval format
    const columns = (state.schema.columns || []).map((col: any) => ({
      name: col.name,
      data_type: col.data_type,
      nullable: col.null_percentage > 0,
    }));

    const result = await client.callTool('approve_schemas', {
      scope_id: state.scopeId,
      approved_by: 'mcp-repl',
      schemas: [{
        discovery_id: state.discoveryId || 'disc-1',
        name: state.schema.name,
        output_table_name: state.schema.name,
        columns,
      }],
    });

    state.contractId = result.contract_id;

    console.log(`\n\x1b[1mContract created:\x1b[0m`);
    console.log(`  ID: \x1b[33m${state.contractId}\x1b[0m`);
    console.log(`  Version: ${result.version}`);
    console.log(`  Schemas approved: ${result.schemas_approved}`);

    if (result.warnings?.length > 0) {
      console.log(`  \x1b[33mWarnings:\x1b[0m`);
      result.warnings.forEach((w: string) => console.log(`    - ${w}`));
    }

    console.log(`\n\x1b[32mRun 'generate' to create a parser from the schema, or 'parser sample' for a basic one.\x1b[0m`);
  },

  async generate(client, args) {
    if (!state.schema) {
      console.log('\x1b[31mNo schema discovered. Run "discover" first.\x1b[0m');
      return;
    }

    const topicName = args[0] || state.schema.name || 'data_parser';

    console.log(`\x1b[90mGenerating parser for "${topicName}"...\x1b[0m`);

    // Convert discovered schema to codegen format
    const columns = (state.schema.columns || []).map((col: any) => ({
      name: col.name,
      data_type: col.data_type,
      nullable: col.null_percentage > 0,
    }));

    const result = await client.callTool('generate_parser', {
      schema: {
        name: state.schema.name,
        columns,
      },
      topic_name: topicName,
      options: {
        file_format: 'csv',
        sink_type: 'duckdb',        // default per user request
        include_error_handling: true,
        include_validation: true,
      },
    });

    if (result.code) {
      state.parserCode = result.code;

      console.log(`\n\x1b[1mGenerated Parser:\x1b[0m`);
      console.log(`  Topic: \x1b[33m${result.topic_name}\x1b[0m`);
      console.log(`  Sink: \x1b[33m${result.sink_type}\x1b[0m`);
      console.log(`  File format: ${result.file_format}`);
      console.log(`  Columns: ${result.columns_count}`);
      console.log(`  Lines: ${result.code.split('\n').length}`);

      console.log(`\n\x1b[90m--- Parser Code ---\x1b[0m`);
      // Show first 25 lines
      const lines = result.code.split('\n');
      lines.slice(0, 25).forEach((line: string, i: number) => {
        console.log(`\x1b[90m${(i + 1).toString().padStart(3)}|\x1b[0m ${line}`);
      });
      if (lines.length > 25) {
        console.log(`\x1b[90m... (${lines.length - 25} more lines)\x1b[0m`);
      }
      console.log(`\x1b[90m-------------------\x1b[0m`);

      console.log(`\n\x1b[32mParser code set. Run 'backtest' to validate.\x1b[0m`);
    } else {
      console.log('\x1b[31mNo parser code generated.\x1b[0m');
      console.log(JSON.stringify(result, null, 2));
    }
  },

  async parser(client, args) {
    if (args[0] === 'sample') {
      state.parserCode = `
import polars as pl

def parse(file_path: str) -> pl.DataFrame:
    """Parse CSV file and return DataFrame."""
    df = pl.read_csv(file_path)
    return df
`.trim();
      console.log('\x1b[32mSample parser code set.\x1b[0m');
      console.log('\x1b[90m' + state.parserCode + '\x1b[0m');
    } else if (args.length > 0) {
      state.parserCode = args.join(' ');
      console.log('\x1b[32mParser code set.\x1b[0m');
    } else {
      console.log('Usage: parser sample OR parser <code>');
      if (state.parserCode) {
        console.log('\nCurrent parser:');
        console.log('\x1b[90m' + state.parserCode + '\x1b[0m');
      }
    }
  },

  async backtest(client) {
    if (!state.files || state.files.length === 0) {
      console.log('\x1b[31mNo files in scope. Run "scan" and "scope" first.\x1b[0m');
      return;
    }

    console.log(`\x1b[90mRunning backtest on ${state.files.length} files...\x1b[0m`);

    const result = await client.callTool('run_backtest', {
      scope_id: state.scopeId,
      files: state.files,
      parser_code: state.parserCode || '# no parser',
      config: {
        pass_rate_threshold: 0.95,
        early_stop_enabled: true,
      },
    });

    const status = result.success ? '\x1b[32mPASS\x1b[0m' : '\x1b[31mFAIL\x1b[0m';

    console.log(`\n\x1b[1mBacktest Result: ${status}\x1b[0m`);
    console.log(`  Pass rate: ${(result.final_pass_rate * 100).toFixed(1)}% (target: ${(result.target_pass_rate * 100).toFixed(1)}%)`);
    console.log(`  Files: ${result.files_passed}/${result.files_tested} passed`);
    console.log(`  Duration: ${result.duration_ms}ms`);
    console.log(`  Termination: ${result.termination_reason}`);

    if (result.failure_categories?.length > 0) {
      console.log(`  \x1b[33mFailure categories:\x1b[0m`);
      result.failure_categories.forEach((cat: any) => {
        console.log(`    - ${cat.category}: ${cat.count}`);
      });
    }

    if (result.top_failing_files?.length > 0) {
      console.log(`  \x1b[33mTop failing files:\x1b[0m`);
      result.top_failing_files.slice(0, 3).forEach((f: string) => {
        console.log(`    - ${path.basename(f)}`);
      });
    }

    if (result.success) {
      console.log(`\n\x1b[32mRun 'execute' to process files.\x1b[0m`);
    } else {
      console.log(`\n\x1b[33mRun 'fix' to get suggestions for fixing failures.\x1b[0m`);
    }
  },

  async fix(client) {
    console.log(`\x1b[90mAnalyzing failures...\x1b[0m`);

    const result = await client.callTool('fix_parser', {
      parser_code: state.parserCode || '# no parser',
      failures: [],
      failure_summary: {
        total_failures: 0,
        by_category: {},
      },
    });

    console.log(`\n\x1b[1mFix Analysis:\x1b[0m`);
    console.log(`  Issues analyzed: ${result.issues_analyzed}`);

    if (result.fixes?.length > 0) {
      console.log(`\n  \x1b[36mSuggested fixes:\x1b[0m`);
      result.fixes.forEach((fix: any) => {
        console.log(`\n  \x1b[33m${fix.fix_type}\x1b[0m (${Math.round(fix.confidence * 100)}% confidence)`);
        console.log(`    ${fix.description}`);
        if (fix.code_snippet) {
          console.log(`    \x1b[90m${fix.code_snippet.split('\n')[0]}...\x1b[0m`);
        }
      });
    }

    if (result.recommendations?.length > 0) {
      console.log(`\n  \x1b[36mRecommendations:\x1b[0m`);
      result.recommendations.forEach((r: string) => console.log(`    - ${r}`));
    }
  },

  async execute(client, args) {
    if (!state.files || state.files.length === 0) {
      console.log('\x1b[31mNo files in scope. Run "scan" and "scope" first.\x1b[0m');
      return;
    }

    const format = args[0] || 'parquet';
    const outputDir = path.join(ROOT, '.casparian_output');

    console.log(`\x1b[90mExecuting pipeline (${format} format)...\x1b[0m`);

    const result = await client.callTool('execute_pipeline', {
      scope_id: state.scopeId,
      files: state.files,
      parser_code: state.parserCode || '# no parser',
      contract_id: state.contractId,
      config: {
        mode: 'full',
        output_format: format,
        output_dir: outputDir,
        validate_schema: true,
      },
    });

    const status = result.success ? '\x1b[32mSUCCESS\x1b[0m' : '\x1b[31mFAILED\x1b[0m';

    console.log(`\n\x1b[1mExecution Result: ${status}\x1b[0m`);
    console.log(`  Files: ${result.files_succeeded}/${result.files_processed} succeeded`);
    console.log(`  Rows: ${result.total_rows}`);
    console.log(`  Duration: ${result.duration_ms}ms`);
    console.log(`  Output: ${result.output_dir}`);

    if (result.errors?.length > 0) {
      console.log(`  \x1b[31mErrors:\x1b[0m`);
      result.errors.slice(0, 3).forEach((e: string) => console.log(`    - ${e}`));
    }

    console.log(`\n\x1b[90mNote: This is currently a mock execution (counts lines but doesn't write real output).\x1b[0m`);
  },

  async query(client, args) {
    if (!args[0]) {
      console.log('Usage: query <file_path>');
      console.log('Example: query demo/data/sample_data.csv');
      return;
    }

    const source = path.isAbsolute(args[0]) ? args[0] : path.join(ROOT, args[0]);

    console.log(`\x1b[90mQuerying ${source}...\x1b[0m`);

    const result = await client.callTool('query_output', {
      source,
      limit: 10,
    });

    console.log(`\n\x1b[1mQuery Result:\x1b[0m`);
    console.log(`  Columns: ${result.columns?.join(', ')}`);
    console.log(`  Rows: ${result.row_count}${result.truncated ? ' (truncated)' : ''}`);

    if (result.rows?.length > 0) {
      console.log(`\n  Data (first ${Math.min(5, result.rows.length)} rows):`);
      console.log('  ' + '-'.repeat(60));

      // Print header
      const cols = result.columns || [];
      console.log('  ' + cols.map((c: string) => c.padEnd(12).slice(0, 12)).join(' | '));
      console.log('  ' + '-'.repeat(60));

      // Print rows
      result.rows.slice(0, 5).forEach((row: any) => {
        const values = cols.map((c: string) => {
          const v = row[c];
          return String(v ?? '').padEnd(12).slice(0, 12);
        });
        console.log('  ' + values.join(' | '));
      });
    }
  },

  async state() {
    console.log('\n\x1b[1mCurrent State:\x1b[0m');
    console.log(`  Scope ID: ${state.scopeId || '\x1b[90m(none)\x1b[0m'}`);
    console.log(`  Discovery ID: ${state.discoveryId || '\x1b[90m(none)\x1b[0m'}`);
    console.log(`  Contract ID: ${state.contractId || '\x1b[90m(none)\x1b[0m'}`);
    console.log(`  Files: ${state.files?.length || 0}`);
    console.log(`  Schema: ${state.schema?.name || '\x1b[90m(none)\x1b[0m'}`);
    console.log(`  Parser: ${state.parserCode ? 'set' : '\x1b[90m(none)\x1b[0m'}`);
  },

  async tools(client) {
    const response = await client.call('tools/list');
    const result = response.result as { tools: Array<{ name: string; description: string }> };

    console.log('\n\x1b[1mAvailable MCP Tools:\x1b[0m\n');

    for (const tool of result.tools || []) {
      console.log(`  \x1b[36m${tool.name}\x1b[0m`);
      console.log(`    ${tool.description}\n`);
    }
  },

  async raw(client, args) {
    if (args.length < 2) {
      console.log('Usage: raw <tool_name> <json_args>');
      console.log('Example: raw quick_scan {"path": "/tmp"}');
      return;
    }

    const toolName = args[0];
    const jsonStr = args.slice(1).join(' ');

    let toolArgs: Record<string, unknown>;
    try {
      toolArgs = JSON.parse(jsonStr);
    } catch (e) {
      console.log('\x1b[31mInvalid JSON\x1b[0m');
      return;
    }

    console.log(`\x1b[90mCalling ${toolName}...\x1b[0m`);
    const result = await client.callTool(toolName, toolArgs);
    console.log(JSON.stringify(result, null, 2));
  },
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

async function main() {
  console.log('\x1b[1m=== MCP Interactive REPL ===\x1b[0m');
  console.log('Type "help" for available commands\n');

  const client = new McpClient();

  // Wait for server to start
  await new Promise(r => setTimeout(r, 1000));

  try {
    await client.initialize();
    console.log('\x1b[32mMCP server connected.\x1b[0m\n');
  } catch (e) {
    console.error('\x1b[31mFailed to initialize MCP server\x1b[0m', e);
    process.exit(1);
  }

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  const prompt = () => {
    rl.question('\x1b[1mmcp>\x1b[0m ', async (line) => {
      const [cmd, ...args] = line.trim().split(/\s+/);

      if (!cmd) {
        prompt();
        return;
      }

      if (cmd === 'quit' || cmd === 'exit' || cmd === 'q') {
        console.log('Goodbye!');
        client.close();
        rl.close();
        process.exit(0);
      }

      const handler = commands[cmd];
      if (handler) {
        try {
          await handler(client, args);
        } catch (e: any) {
          console.error(`\x1b[31mError:\x1b[0m ${e.message}`);
        }
      } else {
        console.log(`Unknown command: ${cmd}. Type "help" for available commands.`);
      }

      console.log();
      prompt();
    });
  };

  prompt();
}

main().catch(console.error);
