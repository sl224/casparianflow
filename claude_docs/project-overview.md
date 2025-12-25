# Casparian Flow - Project Overview

## What Is This?

Casparian Flow is a distributed job processing system with:
- **Rust backend** (`src/`) - Sentinel (job dispatcher), workers, ZeroMQ messaging
- **Tauri desktop app** (`ui/`) - "Casparian Deck" dashboard for monitoring and config
- **Python plugins** - User-defined job processors

## Tech Stack

| Layer | Technology |
|-------|------------|
| Backend | Rust, SQLite, ZeroMQ |
| Desktop App | Tauri v2, Svelte 5, TypeScript |
| Styling | CSS variables, dark theme |
| Testing | Playwright (UI), Cargo test (Rust) |
| Package Manager | Bun (UI), Cargo (Rust) |

## Directory Structure

```
casparianflow/
├── src/                    # Rust source (sentinel, worker, CLI)
├── ui/                     # Tauri + Svelte desktop app
│   └── src/
│       ├── lib/
│       │   ├── components/ # Svelte components
│       │   ├── stores/     # Svelte 5 stores ($state, $effect)
│       │   ├── tauri.ts    # Tauri API wrapper (real or mock)
│       │   └── tauri-mock.ts # Mock for browser testing
│       └── routes/         # SvelteKit pages
├── tests/
│   └── ui-agent/           # Playwright blind agent tests
├── demo/                   # Demo data, schema.sql
└── claude_docs/            # Context docs for Claude sessions
```

## Key Concepts

### Svelte 5 Runes

The UI uses Svelte 5's new runes syntax:
```typescript
// State
let count = $state(0);

// Derived
let doubled = $derived(count * 2);

// Effects
$effect(() => {
  console.log('count changed:', count);
});
```

### Tauri Commands

Frontend calls Rust via `invoke`:
```typescript
import { invoke } from '$lib/tauri';

const rules = await invoke<RoutingRule[]>('get_routing_rules');
await invoke('create_routing_rule', { pattern: '*.csv', tag: 'data' });
```

### Store Pattern

Stores in `ui/src/lib/stores/` use class-based pattern with runes:
```typescript
class MyStore {
  data = $state<Item[]>([]);
  loading = $state(false);

  async load() {
    this.loading = true;
    this.data = await invoke('get_items');
    this.loading = false;
  }
}
export const myStore = new MyStore();
```

## Running the App

```bash
# Full Tauri app (desktop)
cd ui && bun tauri dev

# Just the web UI (for testing, uses mocks)
cd ui && bun run dev

# Run UI tests
cd tests/ui-agent && bun run pw
```

## Common Tasks

### Adding a New Tauri Command

1. **Rust** (`src/commands/*.rs`): Define command with `#[tauri::command]`
2. **Register** (`src/lib.rs`): Add to `invoke_handler`
3. **Mock** (`ui/src/lib/tauri-mock.ts`): Add mock response
4. **Use** (`ui/src/lib/stores/*.ts`): Call via `invoke()`

### Adding a New UI Component

1. Create `ui/src/lib/components/MyComponent.svelte`
2. Add mock data to `ui/src/lib/tauri-mock.ts` if needed
3. Write tests in `tests/ui-agent/my-component.test.ts`

### Testing Approach

- **Unit tests**: Cargo test for Rust logic
- **UI tests**: Playwright blind agent tests (see `claude_docs/ui-testing.md`)
- **Integration**: Full Tauri app with real SQLite database

## UI Components

| Component | Purpose |
|-----------|---------|
| `RoutingTable` | CRUD for routing rules (pattern → tag mapping) |
| `DataGrid` | Displays parquet query results |
| `LogViewer` | Shows job logs and details |
| `TitleBar` | Custom window controls (minimize, maximize, close) |

## Database Schema

Main tables (SQLite):
- `routing_rules` - Pattern matching rules for file routing
- `topic_configs` - Plugin topic configurations
- `jobs` - Job queue and history

## Environment Variables

- `CASPARIAN_DATABASE` - SQLite connection string (e.g., `sqlite:///path/to/db.sqlite3`)
