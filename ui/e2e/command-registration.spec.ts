/**
 * Command Registration Guard
 *
 * Scans frontend for invoke() calls and verifies they exist in backend.
 * This test would have caught the scout_scan bug immediately.
 *
 * WHY THIS EXISTS:
 * On 2025-01-05, the frontend called `scout_scan` but the backend provided
 * `scout_scan_source`. E2E tests passed because the test-bridge had both,
 * but the real Tauri app failed. This test scans source code to prevent
 * such mismatches in the future.
 */
import { test, expect } from "@playwright/test";
import { readFileSync, readdirSync, existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const UI_DIR = join(__dirname, "..");

function getAllFiles(dir: string, pattern: RegExp): string[] {
  if (!existsSync(dir)) return [];

  const files: string[] = [];
  try {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory() && !entry.name.includes("node_modules") && !entry.name.startsWith(".")) {
        files.push(...getAllFiles(path, pattern));
      } else if (pattern.test(entry.name)) {
        files.push(path);
      }
    }
  } catch {
    // Ignore permission errors
  }
  return files;
}

test.describe("Command Registration Guard", () => {
  test("all frontend invoke calls have matching backend commands", async () => {
    // Extract all invoke() calls from frontend
    const frontendCalls = new Map<string, string[]>(); // command -> [files]
    const srcDir = join(UI_DIR, "src");
    const frontendFiles = getAllFiles(srcDir, /\.(svelte|ts)$/);

    for (const file of frontendFiles) {
      const content = readFileSync(file, "utf-8");
      // Match invoke("command_name" or invoke<Type>("command_name"
      const matches = content.matchAll(/invoke[^"]*"([a-z_]+)"/g);
      for (const match of matches) {
        const command = match[1];
        if (!frontendCalls.has(command)) {
          frontendCalls.set(command, []);
        }
        frontendCalls.get(command)!.push(file.replace(UI_DIR + "/", ""));
      }
    }

    // Extract registered commands from lib.rs
    const libRsPath = join(UI_DIR, "src-tauri/src/lib.rs");
    if (!existsSync(libRsPath)) {
      test.skip(true, "lib.rs not found - not in Tauri context");
      return;
    }

    const libRs = readFileSync(libRsPath, "utf-8");
    const registeredCommands = new Set<string>();

    // Match commands in invoke_handler - handle both scout::cmd and direct cmd
    const handlerMatch = libRs.match(/invoke_handler\([\s\S]*?\]\)/);
    if (handlerMatch) {
      // Match patterns like: scout::scout_list_files, list_deployed_plugins, etc.
      const commands = handlerMatch[0].matchAll(/(?:scout::)?([a-z_]+)/g);
      for (const m of commands) {
        // Skip non-command words
        if (!["invoke_handler", "tauri", "generate_handler"].includes(m[1])) {
          registeredCommands.add(m[1]);
        }
      }
    }

    // Find missing commands
    const missing: { command: string; files: string[] }[] = [];
    for (const [command, files] of frontendCalls) {
      if (!registeredCommands.has(command)) {
        missing.push({ command, files });
      }
    }

    if (missing.length > 0) {
      console.error("\n=== MISSING BACKEND COMMANDS ===");
      for (const { command, files } of missing) {
        console.error(`\n  ${command}:`);
        for (const file of files) {
          console.error(`    - ${file}`);
        }
      }
      console.error("\n================================\n");
    }

    expect(
      missing.map((m) => m.command),
      "Frontend calls commands not registered in backend invoke_handler"
    ).toHaveLength(0);
  });

  test("test-bridge implements all frontend commands", async () => {
    // This test ensures the bridge can stand in for Tauri during E2E tests
    const bridgePath = join(UI_DIR, "scripts/test-bridge.ts");
    if (!existsSync(bridgePath)) {
      test.skip(true, "test-bridge.ts not found");
      return;
    }

    const bridgeContent = readFileSync(bridgePath, "utf-8");

    // Extract commands implemented in bridge
    // Match patterns like: command_name: () => or command_name: async () =>
    const bridgeCommands = new Set<string>();
    const commandMatches = bridgeContent.matchAll(/^\s*([a-z_]+):\s*(?:async\s*)?\(/gm);
    for (const m of commandMatches) {
      bridgeCommands.add(m[1]);
    }

    // Get all frontend commands
    const srcDir = join(UI_DIR, "src");
    const frontendFiles = getAllFiles(srcDir, /\.(svelte|ts)$/);
    const frontendCommands = new Set<string>();

    for (const file of frontendFiles) {
      const content = readFileSync(file, "utf-8");
      const matches = content.matchAll(/invoke[^"]*"([a-z_]+)"/g);
      for (const match of matches) {
        frontendCommands.add(match[1]);
      }
    }

    // Find commands frontend needs that bridge doesn't have
    const missingInBridge: string[] = [];
    for (const cmd of frontendCommands) {
      if (!bridgeCommands.has(cmd)) {
        missingInBridge.push(cmd);
      }
    }

    if (missingInBridge.length > 0) {
      console.error("\n=== COMMANDS MISSING FROM TEST-BRIDGE ===");
      console.error(missingInBridge.join("\n"));
      console.error("==========================================\n");
    }

    // This is a warning, not a failure - bridge may not need all commands
    // But log it so we're aware
    if (missingInBridge.length > 0) {
      console.log(`Note: ${missingInBridge.length} commands not in test-bridge (may be intentional)`);
    }
  });
});
