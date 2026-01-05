#!/usr/bin/env bun
/**
 * Reset all Casparian Flow databases
 *
 * Usage:
 *   bun run scripts/reset-db.ts
 *   bun run reset-db
 */

import { unlinkSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { homedir } from 'os';

const CF_DIR = join(homedir(), '.casparian_flow');
// Handle being run from ui/ or project root
const UI_DIR = process.cwd().endsWith('ui') ? process.cwd() : join(process.cwd(), 'ui');
const PROJECT_ROOT = dirname(UI_DIR);

const dbFiles = [
  'scout.db',
  'scout.db-shm',
  'scout.db-wal',
  'sentinel.db',
  'sentinel.db-shm',
  'sentinel.db-wal',
  'casparian_flow.sqlite3',
  'casparian_flow.sqlite3-shm',
  'casparian_flow.sqlite3-wal',
];

// Additional databases in specific locations
const additionalDbs = [
  join(PROJECT_ROOT, 'casparian_flow.sqlite3'),  // Legacy location
  join(PROJECT_ROOT, 'casparian_flow.sqlite3-shm'),
  join(PROJECT_ROOT, 'casparian_flow.sqlite3-wal'),
];

console.log('Resetting Casparian Flow databases...\n');

let deleted = 0;

// ~/.casparian_flow databases
for (const file of dbFiles) {
  const path = join(CF_DIR, file);
  if (existsSync(path)) {
    unlinkSync(path);
    console.log(`  ✓ Deleted ~/.casparian_flow/${file}`);
    deleted++;
  }
}

// ui/src-tauri databases
const tauriDir = join(process.cwd(), 'src-tauri');
for (const file of dbFiles) {
  const path = join(tauriDir, file);
  if (existsSync(path)) {
    unlinkSync(path);
    console.log(`  ✓ Deleted src-tauri/${file}`);
    deleted++;
  }
}

// Project root databases (Sentinel)
for (const path of additionalDbs) {
  if (existsSync(path)) {
    unlinkSync(path);
    console.log(`  ✓ Deleted ${path}`);
    deleted++;
  }
}

if (deleted === 0) {
  console.log('  No database files found.');
} else {
  console.log(`\n✓ Deleted ${deleted} file(s). Databases will be recreated on next app launch.`);
}
