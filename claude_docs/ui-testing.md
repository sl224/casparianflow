# Casparian Deck: Agent UI Testing (E2E)

## Overview

We use a **Blind Agent Fuzzer** pattern running against a **Real Ephemeral Backend**.

This is not a unit test suite with mocks. This is an integration test suite where the UI Agent interacts with a live instance of the Casparian Flow engine (Rust Sentinel + Workers). If the test passes, it means the full stack (TypeScript → IPC → Rust → SQLite → Disk) works.

## Quick Reference

```bash
# 1. Build the Rust backend
cargo build --release

# 2. Build the Test Bridge (IPC Proxy)
cd tests/ui-agent && bun run build:bridge

# 3. Run Agent Tests (Spawns Rust backend automatically)
cd tests/ui-agent && bun run pw

# 4. Run headed (visible browser)
cd tests/ui-agent && bun run pw:headed
```

## Architecture

```
tests/ui-agent/
├── blind-agent.ts        # The Fuzzer Logic
├── agent-probes.js       # DOM discovery/validation (injected JS)
├── bridge/               # HTTP-to-ZMQ Proxy (TODO)
│   └── server.ts         # Forwards UI requests to Rust Sentinel
├── fixtures/             # Real test files (csvs, parquet)
├── playwright.config.ts  # Configures the Ephemeral Environment
└── global-setup.ts       # Spawns Rust Sentinel before tests
```

## How It Works

### 1. The Ephemeral Backend (No Mocks)

We spawn a real instance of the Rust Sentinel for the duration of the test.

**Setup**: Before tests start, `global-setup.ts` creates a temporary sandbox:
- **Database**: A fresh `test.db` (SQLite) is created
- **Process**: We spawn `./target/release/casparian start --database duckdb:/tmp/test_env/test.db`
- **Transport**: The UI sends requests to a local Node.js Bridge, which forwards them over ZMQ to the Rust process

**Why this matters**: If the Agent clicks "Delete Rule", the request goes to the real Rust Sentinel. If SQLite fails due to a foreign key constraint, the UI receives a real error, and the test fails.

### 2. The Bridge Layer

Since the Playwright browser cannot access Unix Domain Sockets (IPC) directly, we use a lightweight bridge:

```
UI (Browser)  ──HTTP──►  Test Bridge (Node)  ──ZMQ/IPC──►  Sentinel (Rust)
```

In `ui/src/lib/tauri.ts`, test mode points the `invoke` function to:
`http://localhost:9999/api/rpc` instead of Tauri bindings.

### 3. The Blind Agent (Fuzzer)

The agent operates in cycles:

**Discovery**: Scans DOM for interactive elements via `agent-probes.js`:
- Buttons, inputs, toggles, checkboxes
- Editable cells (click-to-edit pattern)
- Tab navigation

**Action (Fuzzing)**: Performs user-like actions, but faster and less predictably:
- Clicking tabs rapidly
- Typing into fields
- Toggling switches repeatedly
- Deleting and recreating rows

**Validation**: Verification is **isomorphic**:
- **DOM Check**: Did the row disappear from the table?
- **Backend Check**: Query the test.db directly to verify the record is gone

## Target Types

| Type | Description | Example |
|------|-------------|---------|
| `button` | Generic buttons | Refresh, Submit |
| `tab` | Navigation tabs | DASHBOARD, CONFIG, DATA |
| `toggle` | ON/OFF switches | Rule enabled toggle |
| `edit` | Click-to-edit cells | Pattern, Tag, Description |
| `add` | Add/create buttons | "+ Add Rule" |
| `delete` | Delete/remove buttons | "×" delete icon |
| `input` | Form inputs | Checkboxes, text fields |

## Writing Tests

### Basic Integration Test

```typescript
import { test, expect } from '@playwright/test';
import { createAgent } from './blind-agent';
import duckdb from 'duckdb';

test.describe('Real Backend Integration', () => {
  let agent: BlindAgent;
  let db: duckdb.Database;

  test.beforeAll(() => {
    // Connect to the ephemeral DB used by the Rust process
    db = new duckdb.Database('/tmp/test_env/casparian.duckdb');
  });

  test('Create Rule and Verify Persistence', async ({ page }) => {
    agent = await createAgent(page);

    // 1. UI Action: Create a Rule
    await page.click('text=Add Rule');
    await page.fill('input[name="pattern"]', '*.log');
    await page.click('button:has-text("Save")');

    // 2. UI Validation: Verify it appears in the list
    await expect(page.locator('.rule-row', { hasText: '*.log' })).toBeVisible();

    // 3. Backend Validation: Verify it actually wrote to DB
    const row = db.prepare('SELECT * FROM routing_rules WHERE pattern = ?').get('*.log');
    expect(row).toBeDefined();
    expect(row.pattern).toBe('*.log');
  });
});
```

### Stress Testing (Chaos Mode)

Run the agent in chaos mode against the real backend to find race conditions:

```typescript
test('Chaos Monkey: Rapid Toggling', async ({ page }) => {
  const agent = await createAgent(page);

  const toggles = await agent.findByType('toggle');

  // Toggle 20 times as fast as possible
  // A mock would pass. A real backend might hit SQLite locking.
  for (let i = 0; i < 20; i++) {
    await agent.act(toggles[0]);
  }

  // Verify the system didn't crash
  const systemHealth = await page.request.get('http://localhost:9999/api/pulse');
  expect(systemHealth.ok()).toBeTruthy();
});
```

### Exploration Test (Find Unknown Bugs)

```typescript
test('Random exploration', async ({ page }) => {
  const agent = await createAgent(page);

  const results = await agent.explore({
    maxActions: 20,
    excludeTypes: ['delete'], // Don't destroy data
  });

  const summary = agent.getSummary();
  expect(summary.critical).toBe(0); // No crashes
});
```

## Agent Methods

```typescript
// Discover all interactive elements
const targets = await agent.discover();

// Find by type
const toggles = await agent.findByType('toggle');
const deleteButtons = await agent.findByType('delete');

// Find by text content
const addButtons = await agent.findByText('Add Rule');

// Act on a target (click and validate)
const result = await agent.act(target);
// result.status: 'SUCCESS' | 'WARNING' | 'ERROR' | 'CRITICAL'
// result.category: 'toggle' | 'delete' | 'add' | 'navigation' | etc.

// Random exploration
await agent.explore({ maxActions: 10, excludeTypes: ['delete'] });

// Get summary of all actions
const summary = agent.getSummary();
// { total: 10, success: 7, warning: 2, error: 1, critical: 0 }
```

## Key Files

| File | Purpose |
|------|---------|
| `tests/ui-agent/global-setup.ts` | Spawns Rust Sentinel, creates temp DB |
| `tests/ui-agent/bridge/server.ts` | HTTP → ZMQ translator (TODO) |
| `ui/src/lib/tauri.ts` | Routes UI calls to Bridge during testing |
| `tests/ui-agent/blind-agent.ts` | Agent orchestration class |
| `tests/ui-agent/agent-probes.js` | Injectable DOM discovery |

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Connection Refused | Backend didn't spawn. Check `test-results/backend.log` |
| ZMQ Error | Bridge and Sentinel have mismatched socket paths |
| Database Locked | SQLite contention. Ensure WAL mode is enabled |
| Zombie Processes | Run `pkill -f casparian` to clean up |
| Blank page | Check that Bridge is running and `tauri.ts` routes to it |

## TODO: Bridge Implementation

The bridge layer needs to be built:

```typescript
// tests/ui-agent/bridge/server.ts
import { serve } from 'bun';
import { Socket } from 'zeromq';

const dealer = new Socket(zmq.DEALER);
dealer.connect('ipc:///tmp/casparian.sock');

serve({
  port: 9999,
  async fetch(req) {
    const { command, args } = await req.json();

    // Forward to Rust Sentinel via ZMQ
    await dealer.send(JSON.stringify({ command, args }));
    const [response] = await dealer.receive();

    return Response.json(JSON.parse(response.toString()));
  }
});
```

## Design Principles

1. **No mocks** - Real Rust backend, real SQLite, real disk I/O
2. **Ephemeral** - Fresh database per test run, no state pollution
3. **Isomorphic validation** - Check both DOM and database
4. **Chaos-friendly** - Agent finds race conditions humans miss
5. **Fast feedback** - Tests should complete in under 60 seconds
