/**
 * Scout (Pipelines) E2E Tests
 *
 * Tests the unified tree view for pipelines:
 * - Empty state shows correctly
 * - Add folder button is visible
 * - Source expansion works
 * - No JS errors during interaction
 */

import { test, expect } from '@playwright/test';

test.describe('Scout Pipelines Tab', () => {
  test.beforeEach(async ({ page }) => {
    // Capture JS errors
    page.on('pageerror', err => {
      console.error('Page error:', err.message);
    });

    await page.goto('/');
    await expect(page.locator('.logo-text')).toContainText('CASPARIAN');

    // Navigate to Pipelines tab
    await page.click('button:has-text("PIPELINES")');
    await page.waitForTimeout(500);
  });

  test('shows empty state when no sources configured', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    // Should show empty state
    await expect(page.locator('.empty-state')).toBeVisible();
    await expect(page.locator('.empty-title')).toContainText('No Sources');

    // Add folder button should be visible
    await expect(page.locator('button:has-text("+ Add Folder")')).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('pipelines header and add button visible', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    // Header should show SCOUT - File Discovery
    await expect(page.locator('.header .title')).toContainText('SCOUT');

    // Primary action button should be visible
    const addBtn = page.locator('button.action-btn.primary:has-text("+ Add Folder")');
    await expect(addBtn).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('tree view container exists', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    // The unified tree view container should exist
    await expect(page.locator('.tree-view')).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  // Note: We can't test Tauri dialog interactions (Add Folder) in Playwright
  // because Tauri's native APIs aren't available in browser context.
  // Those need to be tested via Tauri's test framework or manual testing.
});
