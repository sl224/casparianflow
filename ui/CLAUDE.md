# Claude Code Instructions for UI

## Testing Protocol

**After ANY UI code change, run these checks:**

```bash
# 1. Type check (catches TypeScript errors, but NOT template runtime errors)
bun run check

# 2. Build (catches Svelte compile errors including template issues)
bun run build

# 3. E2E tests (catches runtime errors by actually clicking through the app)
bun run test:e2e
```

### Why E2E Tests Matter

On 2024-12-28, we shipped a crash because:
1. A function was placed in `<script module>` but used in the template
2. `bun run check` passed (type checking doesn't catch this)
3. `bun run build` passed (build doesn't catch template scope errors)
4. Nobody actually clicked the tab before shipping

**Playwright E2E tests would have caught this immediately.**

### E2E Test Philosophy

With LLMs generating code:
- Writing tests is cheap (minutes, not hours)
- Maintaining tests is cheap (LLM can fix broken selectors)
- The cost-benefit strongly favors E2E tests

### Running E2E Tests

```bash
# Run all E2E tests (starts dev server automatically)
bun run test:e2e

# Run with UI for debugging
bun run test:e2e -- --ui

# Run specific test file
bun run test:e2e -- e2e/tabs.spec.ts
```

### Adding New E2E Tests

When adding new UI features, add E2E tests in `/ui/e2e/`:

```typescript
import { test, expect } from '@playwright/test';

test('new feature works', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', err => errors.push(err.message));

  await page.goto('/');
  await page.click('button:has-text("TAB_NAME")');

  // Verify content loads
  await expect(page.locator('.expected-element')).toBeVisible();

  // Verify no JS errors
  expect(errors).toHaveLength(0);
});
```

### Key Lesson

> "Type checking is necessary but not sufficient. Build the app. Click the app. Or let Playwright click it for you."

## Svelte 5 Gotchas

### Module vs Instance Scope

Functions in `<script module>` are NOT accessible in templates:

```svelte
<!-- WRONG - will crash at runtime -->
<script module>
  function helper() { return "x"; }
</script>
{helper()}  <!-- ReferenceError: helper is not defined -->

<!-- CORRECT -->
<script>
  function helper() { return "x"; }
</script>
{helper()}  <!-- Works -->
```

## Node Version

Playwright requires Node 18.19+. The project uses Node 20 via nvm:

```bash
# E2E tests automatically use Node 20 via PATH override in package.json
bun run test:e2e
```
