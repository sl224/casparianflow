# Claude Code Instructions for UI

## Quick Reference

```bash
bun run check      # Type check (catches TypeScript errors)
bun run build      # Build (catches Svelte compile errors)
bun run test:e2e   # E2E tests (catches runtime errors)
bun run dev        # Development server (port 1420)
```

**Always run all three checks after any UI change.**

---

## UI Architecture

### Tabs Overview

| Tab | Component | Purpose |
|-----|-----------|---------|
| DASHBOARD | `+page.svelte` | System metrics, real-time pulse |
| PIPELINES | `scout/*.svelte` | Scout sources, tagging rules, file discovery |
| PARSER LAB | `parser-lab/*.svelte` | Develop and test parsers |
| JOBS | `+page.svelte` | Processing queue, job monitoring |
| PUBLISH | `+page.svelte` | Plugin publishing (future) |

### Key Files

```
ui/
├── src/
│   ├── routes/
│   │   └── +page.svelte           # Main app with all tabs
│   └── lib/
│       ├── components/
│       │   ├── parser-lab/
│       │   │   ├── ParserLabTab.svelte   # Main view: list + actions
│       │   │   ├── FileEditor.svelte     # Parser editor (full page)
│       │   │   └── SinkConfig.svelte     # Sink configuration widget
│       │   ├── scout/
│       │   │   ├── ScoutTab.svelte       # Pipelines tab
│       │   │   ├── SourceList.svelte     # Source management
│       │   │   └── FileList.svelte       # Discovered files
│       │   └── shredder/                 # DEPRECATED - legacy code
│       └── stores/
│           └── scout.svelte.ts           # Scout state management
└── src-tauri/
    └── src/
        ├── lib.rs                        # Tauri command registration
        └── scout.rs                      # All Tauri commands
```

---

## Parser Lab (v6 - Parser-Centric)

### Concept

Parser Lab is where users develop and test parsers before publishing them as plugins.

**Flow:**
1. User opens a file OR loads existing parser code OR loads sample
2. Parser editor opens with the file as test data
3. User writes/edits parser code
4. User tests parser (Ctrl+Enter)
5. Output shown in real-time
6. User configures sink (Parquet, CSV, SQLite)
7. User publishes parser as plugin (future)

### Data Model

```typescript
// Parser is the top-level entity (no project wrapper)
interface ParserLabParser {
  id: string;
  name: string;
  filePattern: string;        // e.g., "RFC_DB", "*.log"
  patternType: string;        // "all", "key_column", "glob"
  sourceCode: string | null;
  validationStatus: string;   // "pending", "valid", "invalid"
  validationError: string | null;
  validationOutput: string | null;
  sinkType: string;           // "parquet", "csv", "sqlite"
  sinkConfigJson: string | null;
  isSample: boolean;          // true for bundled sample
  // ...timestamps
}

// Test files belong to a parser
interface ParserLabTestFile {
  id: string;
  parserId: string;
  filePath: string;
  fileName: string;
  fileSize: number | null;
}
```

### Tauri Commands

```typescript
// Parsers
parser_lab_create_parser(name, filePattern?) -> Parser
parser_lab_get_parser(parserId) -> Parser | null
parser_lab_update_parser(parser) -> void
parser_lab_delete_parser(parserId) -> void
parser_lab_list_parsers(limit?) -> ParserSummary[]

// Test files
parser_lab_add_test_file(parserId, filePath) -> TestFile
parser_lab_remove_test_file(testFileId) -> void
parser_lab_list_test_files(parserId) -> TestFile[]

// Operations
parser_lab_validate_parser(parserId, testFileId) -> Parser
parser_lab_import_plugin(pluginPath) -> Parser
parser_lab_load_sample() -> Parser  // Creates bundled sample
```

### Sample Parser

The app bundles a sample parser to help users understand the workflow:
- Sample CSV: `~/.casparian_flow/samples/transactions.csv`
- Sample code: polars-based parser with type conversion
- "Load Sample" button in ParserLabTab.svelte

---

## Testing Protocol

### Why E2E Tests Matter

On 2024-12-28, we shipped a crash because:
1. A function was placed in `<script module>` but used in the template
2. `bun run check` passed (type checking doesn't catch this)
3. `bun run build` passed (build doesn't catch template scope errors)
4. Nobody actually clicked the tab before shipping

**Playwright E2E tests catch this immediately.**

### Running E2E Tests

```bash
# Run all E2E tests
bun run test:e2e

# Run with UI for debugging
bun run test:e2e -- --ui

# Run specific test file
bun run test:e2e -- e2e/tabs.spec.ts
```

### Adding E2E Tests

When adding new features, add tests in `e2e/`:

```typescript
import { test, expect } from '@playwright/test';

test('parser lab loads sample', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', err => errors.push(err.message));

  await page.goto('/');
  await page.click('button:has-text("PARSER LAB")');
  await page.click('button:has-text("Load Sample")');

  // Verify editor opens
  await expect(page.locator('.file-editor')).toBeVisible();

  // Verify no JS errors
  expect(errors).toHaveLength(0);
});
```

### Key Lesson

> "Type checking is necessary but not sufficient. Build the app. Click the app. Or let Playwright click it for you."

---

## Svelte 5 Patterns

### State with Runes

```svelte
<script lang="ts">
  // Props
  interface Props {
    parserId: string;
    onBack: () => void;
  }
  let { parserId, onBack }: Props = $props();

  // State
  let isLoading = $state(true);
  let data = $state<SomeType | null>(null);

  // Derived
  let isValid = $derived(data?.status === 'valid');
</script>
```

### Common Gotchas

**Module vs Instance Scope:**
```svelte
<!-- WRONG - crashes at runtime -->
<script module>
  function helper() { return "x"; }
</script>
{helper()}  <!-- ReferenceError -->

<!-- CORRECT -->
<script>
  function helper() { return "x"; }
</script>
{helper()}  <!-- Works -->
```

**Nested Buttons (invalid HTML):**
```svelte
<!-- WRONG - browser will "repair" the HTML -->
<button onclick={selectItem}>
  <button onclick={deleteItem}>X</button>
</button>

<!-- CORRECT - use div with role="button" -->
<div role="button" tabindex="0" onclick={selectItem} onkeydown={handleKey}>
  <button onclick={deleteItem}>X</button>
</div>
```

---

## Tauri Integration

### Calling Rust Commands

```typescript
import { invoke } from "@tauri-apps/api/core";

// Type-safe invoke
const parser = await invoke<ParserLabParser>("parser_lab_get_parser", {
  parserId: "some-id"
});

// Error handling
try {
  await invoke("parser_lab_delete_parser", { parserId });
} catch (e) {
  error = String(e);  // Error message from Rust
}
```

### Adding New Commands

1. Add function in `src-tauri/src/scout.rs`:
```rust
#[tauri::command]
pub fn my_new_command(
    state: State<'_, ScoutState>,
    param: String,
) -> Result<MyReturnType, String> {
    // Implementation
}
```

2. Register in `src-tauri/src/lib.rs`:
```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands
    scout::my_new_command,
])
```

3. Call from frontend:
```typescript
const result = await invoke<MyReturnType>("my_new_command", {
  param: "value"
});
```

---

## Database Architecture (CRITICAL)

### SINGLE DATABASE

**Everything uses ONE database: `~/.casparian_flow/casparian_flow.sqlite3`**

Never use relative paths. Never create multiple database files.

```
~/.casparian_flow/
├── casparian_flow.sqlite3      # THE ONLY DATABASE (all tables)
├── parsers/                    # Deployed parser .py files
├── output/                     # Parser output files (parquet, sqlite, csv)
├── shredder_env/               # Python virtual environment
└── samples/                    # Sample files for demos
```

### Tables in casparian_flow.sqlite3

| Prefix | Tables | Purpose |
|--------|--------|---------|
| `parser_lab_*` | parsers, test_files | Parser Lab development |
| `scout_*` | sources, files, tagging_rules | File discovery & tagging |
| `cf_*` | plugin_manifest, processing_queue, topic_config | Sentinel backend |

### Why Single Database

On 2025-01-05, we had TWO databases (`scout.db` and `casparian_flow.sqlite3`).
This caused:
1. Parser Lab published to one DB, jobs ran from another
2. "Plugin not found" errors when the plugin WAS deployed
3. Confusion about which DB had what data

**ONE database = ONE source of truth = no sync issues.**

### Code Patterns

**Rust (scout.rs):**
```rust
// CORRECT - single database
let default_path = dirs::home_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join(".casparian_flow")
    .join("casparian_flow.sqlite3");

// WRONG - separate scout.db
.join("scout.db");  // NO! Use casparian_flow.sqlite3
```

**TypeScript (test-bridge.ts):**
```typescript
// CORRECT - single database
const DB_PATH = join(CF_DIR, 'casparian_flow.sqlite3');
const db = new Database(DB_PATH);

// WRONG - separate databases
const db = new Database('scout.db');
const sentinelDb = new Database('casparian_flow.sqlite3');
```

**SQLite URL (lib.rs):**
```rust
// CORRECT - mode=rwc auto-creates
format!("sqlite:{}?mode=rwc", db_path.display())
```

### Verification

```bash
# Should show ONLY casparian_flow.sqlite3
ls -la ~/.casparian_flow/*.sqlite3
# Should show NO scout.db
ls -la ~/.casparian_flow/scout.db  # Should not exist!
```

---

## Plugin Dependencies

Parser validation runs Python code in an isolated environment:

- **Location:** `~/.casparian_flow/shredder_env`
- **Packages:** polars, pandas, pyarrow
- **Manager:** `ShredderEnvManager` in `src-tauri/src/scout.rs`
- **Creation:** First validation takes 10-30 seconds (installs deps)
- **Subsequent:** Instant (cached environment)

---

## Node Version

Playwright requires Node 18.19+. The project uses Node 20 via nvm:

```bash
# E2E tests automatically use Node 20 via PATH override in package.json
bun run test:e2e
```

---

## Common Tasks

### Add a New Tab

1. Add tab button in `src/routes/+page.svelte`
2. Add component in `src/lib/components/`
3. Add tab content section in `+page.svelte`
4. Add E2E test for the tab

### Update Parser Lab Types

1. Update Rust structs in `src-tauri/src/scout.rs`
2. Update TypeScript interfaces in component files
3. Update database schema in `crates/casparian_scout/src/db.rs`
4. Run `cargo check` and `bun run check`

### Debug Tauri Command Failures

1. Check Tauri stderr output (run app from terminal)
2. Look for Rust panic or error messages
3. Add logging: `tracing::info!("Debug: {:?}", value);`
4. Check SQLite queries with `.map_err(|e| format!("Error: {}", e))`

---

## Bridge Schema Compatibility (CRITICAL)

### The Bug (2025-01-05)

The test bridge (`scripts/test-bridge.ts`) had a completely different database schema than the Rust code (`crates/casparian_scout/src/db.rs`):

**Bridge (wrong):**
```sql
scout_files: id, source_id, file_path, file_name, file_size, tag, ...
```

**Rust (correct):**
```sql
scout_files: id, source_id, path, rel_path, size, mtime, content_hash, ...
```

Result: Tests passed (used fake schema), real app crashed (expected real schema).

### The Fix

1. Bridge schema now **exactly matches** Rust schema
2. Added `e2e/schema-compat.spec.ts` that verifies:
   - `scout_files` has `mtime`, `path`, `size` (not `file_path`, `file_size`)
   - `scout_sources` table exists
   - Required indexes exist

### Keeping Schemas in Sync

When you modify the Rust schema in `crates/casparian_scout/src/db.rs`:
1. Update `scripts/test-bridge.ts` initDb() to match
2. Run `bun run test:e2e -- e2e/schema-compat.spec.ts` to verify
3. If test fails, you forgot to update the bridge

### Why We Have a Bridge

The bridge exists because Playwright tests run against Vite dev server, which doesn't have Tauri. The bridge provides HTTP endpoints that mimic Tauri commands.

The bridge is NOT a mock - it uses the same SQLite database (`~/.casparian_flow/casparian_flow.sqlite3`) with the same schema. It's a translation layer, not a fake.

---

## Deprecated Code

The following are deprecated and will be removed:

- `src/lib/components/shredder/` - Legacy shredder UI (replaced by Parser Lab)
- `e2e/shredder.spec.ts` - Removed (tests for non-existent "SHREDDER" tab)

Do not add new code to these directories.
