# Blind Agent UI Testing Framework

## Overview

Casparian Deck uses a **Blind Agent** testing pattern for automated UI testing. This approach simulates how an AI agent would interact with the UI without visual inspection - discovering elements, acting on them, and validating results through DOM changes.

## Quick Reference

```bash
# Run all tests (headless)
cd tests/ui-agent && bun run pw

# Run tests with visible browser (for debugging)
cd tests/ui-agent && bun run pw:headed

# Run specific test file
cd tests/ui-agent && bunx playwright test routing-table.test.ts

# View test report
cd tests/ui-agent && bunx playwright show-report
```

## Architecture

```
tests/ui-agent/
├── blind-agent.ts       # Main agent orchestration class
├── agent-probes.js      # Injectable JS for DOM discovery/validation
├── routing-table.test.ts # Playwright tests for RoutingTable
├── playwright.config.ts  # Playwright configuration
├── global-setup.ts       # Pre-test environment setup
└── test-results/         # Screenshots and artifacts
```

## How It Works

### 1. Mock Layer (No Tauri Required)

Tests run in a browser against the Vite dev server. Since Tauri APIs aren't available in browsers, we have a mock layer:

- **`ui/src/lib/tauri-mock.ts`** - Mock implementations of Tauri `invoke` commands
- **`ui/src/lib/tauri.ts`** - Wrapper that auto-switches between real Tauri and mocks

The mock provides:
- `get_routing_rules` - Returns mock routing rules
- `create_routing_rule` / `update_routing_rule` / `delete_routing_rule` - CRUD operations
- `get_system_pulse` - Mock system metrics
- `listen('system-pulse')` - Periodic mock events

### 2. Blind Agent Pattern

The agent operates in 4 phases:

```
Discovery → Execution → Validation → Recovery
```

**Discovery**: Scans DOM for interactive elements using `agent-probes.js`:
- Buttons, inputs, toggles, checkboxes
- Editable cells (click-to-edit pattern)
- Delete buttons, add buttons
- Tab navigation

**Execution**: Clicks/interacts with discovered targets

**Validation**: Compares DOM state before/after to detect:
- Row additions/deletions
- Toggle state changes
- Input value changes
- Navigation changes

**Recovery**: Handles errors gracefully, continues exploration

### 3. Target Types

The agent categorizes elements:

| Type | Description | Example |
|------|-------------|---------|
| `button` | Generic buttons | Refresh, Submit |
| `tab` | Navigation tabs | DASHBOARD, CONFIG, DATA |
| `toggle` | ON/OFF switches | Rule enabled toggle |
| `edit` | Click-to-edit cells | Pattern, Tag, Description |
| `add` | Add/create buttons | "+ Add Rule" |
| `delete` | Delete/remove buttons | "×" delete icon |
| `input` | Form inputs | Checkboxes, text fields |

## Writing New Tests

### Basic Test Structure

```typescript
import { test, expect } from '@playwright/test';
import { createAgent, BlindAgent } from './blind-agent';

test.describe('MyComponent Tests', () => {
  let agent: BlindAgent;

  test.beforeEach(async ({ page }) => {
    await page.goto('http://localhost:1420');
    await page.waitForLoadState('networkidle');

    // Navigate to the right tab/view
    await page.click('.tab:has-text("CONFIG")');
    await page.waitForSelector('.my-component');

    // Initialize agent
    agent = await createAgent(page, { waitMs: 300 });
  });

  test('should discover elements', async ({ page }) => {
    const targets = await agent.discover();
    console.log(`Found ${targets.length} targets`);

    // Verify expected elements exist
    const buttons = targets.filter(t => t.type === 'button');
    expect(buttons.length).toBeGreaterThan(0);
  });

  test('should toggle something', async ({ page }) => {
    const toggles = await agent.findByType('toggle');
    if (toggles.length === 0) return; // Skip if no toggles

    const entry = await agent.act(toggles[0]);
    expect(entry.result.status).toBe('SUCCESS');
  });
});
```

### Agent Methods

```typescript
// Discover all interactive elements
const targets = await agent.discover();

// Find by type
const toggles = await agent.findByType('toggle');
const deleteButtons = await agent.findByType('delete');

// Find by text content
const addButtons = await agent.findByText('Add Rule');
const refreshBtn = await agent.findByText('Refresh');

// Act on a target (click and validate)
const result = await agent.act(target);
// result.status: 'SUCCESS' | 'WARNING' | 'ERROR' | 'CRITICAL'
// result.category: 'toggle' | 'delete' | 'add' | 'navigation' | etc.
// result.msg: Human-readable description

// Random exploration (excludes destructive actions)
await agent.explore({
  maxActions: 10,
  excludeTypes: ['delete'],
});

// Get summary of all actions
const summary = agent.getSummary();
// { total: 10, success: 7, warning: 2, error: 1, critical: 0 }
```

### Common Test Patterns

**Testing CRUD operations:**
```typescript
test('Add then Delete', async ({ page }) => {
  const initialCount = await page.locator('.table-row').count();

  // Add
  const addBtn = await agent.findByText('Add');
  await agent.act(addBtn[0]);
  expect(await page.locator('.table-row').count()).toBe(initialCount + 1);

  // Delete
  const deleteBtn = await agent.findByType('delete');
  await agent.act(deleteBtn[deleteBtn.length - 1]); // Last one
  expect(await page.locator('.table-row').count()).toBe(initialCount);
});
```

**Testing keyboard shortcuts:**
```typescript
test('Escape cancels edit', async ({ page }) => {
  // Enter edit mode
  await page.click('.editable-cell');
  await page.fill('input', 'new value');

  // Press Escape
  await page.keyboard.press('Escape');

  // Input should be gone
  expect(await page.isVisible('input.cell-input')).toBe(false);
});
```

**Exploration test (finds bugs via random actions):**
```typescript
test('Random exploration', async ({ page }) => {
  const results = await agent.explore({
    maxActions: 20,
    excludeTypes: ['delete'], // Don't delete things
  });

  const summary = agent.getSummary();
  expect(summary.critical).toBe(0); // No crashes
});
```

## Adding Mock Data

Edit `ui/src/lib/tauri-mock.ts` to add mock data:

```typescript
// Add new mock command
case 'my_new_command': {
  return { data: 'mock response' } as T;
}
```

## Test Results

After running tests:
- **Screenshots**: `test-results/<test-name>/test-finished-1.png`
- **HTML Report**: `bunx playwright show-report`
- **JSON Results**: `test-results.json`

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Blank page | Check mock layer is working - look for `[TauriMock]` in console |
| Tab not switching | Use correct selector: `.tab:has-text("CONFIG")` |
| Element not found | Add `waitMs` option or explicit `waitForSelector` |
| Flaky tests | Increase timeouts, use `test.skip` for known issues |
| Tests pass but UI looks wrong | Run `bun run pw:headed` to see what's happening |

## Key Files to Update

When adding new UI features:

1. **Add mock data**: `ui/src/lib/tauri-mock.ts`
2. **Create tests**: `tests/ui-agent/<component>.test.ts`
3. **Update probes** (if new element types): `tests/ui-agent/agent-probes.js`

## Design Principles

1. **No visual inspection** - Agent only sees DOM structure, not pixels
2. **Self-healing** - Agent discovers elements dynamically, not hardcoded selectors
3. **Safe exploration** - Exclude destructive actions from random exploration
4. **Fast feedback** - Tests run against Vite (not full Tauri), ~30s for full suite
5. **Mock everything** - No backend required, predictable test data
