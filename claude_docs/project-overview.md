# Casparian Flow - Project Overview

## What Is This?

Casparian Flow is a distributed job processing system with:
- **Rust backend** - Sentinel (job dispatcher), Workers, ZeroMQ messaging
- **Tauri desktop app** - "Casparian Deck" dashboard for monitoring and config
- **Python plugins** - User-defined job processors (no SDK required)

## Tech Stack

| Layer | Technology |
|-------|------------|
| Backend | Rust, SQLite, ZeroMQ |
| Desktop App | Tauri v2, Svelte 5, TypeScript |
| Styling | CSS variables, dark theme |
| Testing | Playwright (UI), Cargo test (Rust) |
| Package Manager | Bun (UI), Cargo (Rust) |
| Plugin Runtime | Python (pandas, pyarrow) via bridge_shim |

## Directory Structure

```
casparianflow/
├── crates/                     # Rust source
│   ├── casparian/              # CLI binary
│   ├── casparian_sentinel/     # Job dispatcher
│   ├── casparian_worker/       # Job executor
│   │   └── shim/bridge_shim.py # Embedded Python shim
│   ├── cf_protocol/            # Wire protocol
│   └── cf_security/            # Signing, Azure auth
├── ui/                         # Tauri + Svelte desktop app
│   └── src/
│       ├── lib/
│       │   ├── components/     # Svelte components
│       │   ├── stores/         # Svelte 5 stores
│       │   └── tauri.ts        # Tauri API wrapper
│       └── routes/             # SvelteKit pages
├── tests/
│   └── ui-agent/               # Playwright blind agent tests
├── demo/
│   ├── plugins/                # Example plugins (no SDK needed)
│   └── data/                   # Sample data files
└── claude_docs/                # Context docs for Claude sessions
```

## Key Concepts

### Plugin Contract (No SDK)

Plugins are simple Python files. No inheritance or SDK imports required:

```python
import pandas as pd
import pyarrow as pa

MANIFEST = {
    "pattern": "*.csv",
    "topic": "output",
}

class Handler:
    def execute(self, file_path: str):
        df = pd.read_csv(file_path)
        yield pa.Table.from_pandas(df)
```

### Bridge Mode Execution

Plugins run in isolated venvs via subprocess:

```
Rust Worker
    │
    ├── Creates venv from lockfile (uv sync)
    ├── Spawns Python subprocess (bridge_shim.py)
    ├── Passes plugin code via env var (base64)
    └── Receives Arrow IPC batches via Unix socket
```

### Svelte 5 Runes

The UI uses Svelte 5's new runes syntax:

```typescript
let count = $state(0);
let doubled = $derived(count * 2);

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

## Running the App

```bash
# Full Tauri app (desktop)
cd ui && bun tauri dev

# Run UI tests (ephemeral backend)
cd tests/ui-agent && bun run pw

# Build release
cargo build --release
```

## Common Tasks

### Adding a New Tauri Command

1. **Rust** (`ui/src-tauri/src/*.rs`): Define with `#[tauri::command]`
2. **Register** (`ui/src-tauri/src/lib.rs`): Add to `invoke_handler`
3. **Use** (`ui/src/lib/stores/*.ts`): Call via `invoke()`

### Writing a New Plugin

Create a Python file with:
```python
MANIFEST = {"pattern": "*.csv", "topic": "my_output"}

class Handler:
    def execute(self, file_path: str):
        # Process file, yield Arrow tables
        yield pa.Table.from_pandas(df)
```

### Testing UI Changes

```bash
cd tests/ui-agent
bun run pw:headed  # Watch tests run in browser
```

## Database Schema

Main tables (SQLite):
- `routing_rules` - Pattern matching rules for file routing
- `plugin_manifests` - Registered plugins with source code
- `processing_jobs` - Job queue and history
- `plugin_environments` - Cached lockfiles for venv management

## CLI Commands

```bash
# Start sentinel
./target/release/casparian start --database sqlite://db.sqlite3

# Worker mode
./target/release/casparian worker --connect tcp://localhost:5555
```
