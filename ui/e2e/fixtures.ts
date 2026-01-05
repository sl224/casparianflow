/**
 * Shared Playwright test fixtures for Casparian Flow E2E tests
 *
 * Usage:
 *   import { test, expect } from './fixtures';
 *
 *   test('my test', async ({ page }) => {
 *     // Bridge mode is automatically enabled
 *   });
 */

import { test as base, expect } from '@playwright/test';

// Extend the base test to automatically enable bridge mode
export const test = base.extend({
  page: async ({ page }, use) => {
    // Enable bridge mode before each test
    await page.addInitScript(() => {
      (window as any).__CASPARIAN_BRIDGE__ = true;
    });

    await use(page);
  },
});

export { expect };

// Bridge API helper for tests that need direct backend calls
export const BRIDGE_URL = 'http://localhost:9999';

export async function bridgeCall(command: string, args: Record<string, unknown> = {}) {
  const response = await fetch(`${BRIDGE_URL}/api/rpc`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ command, args }),
  });
  const data = await response.json();
  if (data.error) throw new Error(data.error);
  return data.result;
}
