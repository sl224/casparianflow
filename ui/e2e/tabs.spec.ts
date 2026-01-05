/**
 * Tab Navigation E2E Tests
 *
 * These tests verify that all tabs can be clicked without crashing.
 * This is exactly the kind of test that would have caught the truncateValue bug.
 */

import { test, expect } from '@playwright/test';

test.describe('Tab Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    // Wait for app to load
    await expect(page.locator('.logo-text')).toContainText('CASPARIAN');
  });

  test('Dashboard tab loads without errors', async ({ page }) => {
    await page.click('button:has-text("DASHBOARD")');

    // Verify dashboard content is visible
    await expect(page.locator('.dashboard')).toBeVisible();
    await expect(page.locator('.panel-title:has-text("WORKERS")')).toBeVisible();

    // Check for console errors
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));
    await page.waitForTimeout(500);
    expect(errors).toHaveLength(0);
  });

  test('Pipelines tab loads without errors', async ({ page }) => {
    // This test would have caught the truncateValue bug
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PIPELINES")');

    // Wait for component to mount and any async operations
    await page.waitForTimeout(1000);

    // Verify pipelines content is visible (ScoutTab with unified tree view)
    await expect(page.locator('.scout-tab')).toBeVisible();
    // Check for the header title (Scout - File Discovery)
    await expect(page.locator('.title:has-text("SCOUT")')).toBeVisible();

    // The key check: no JavaScript errors
    expect(errors).toHaveLength(0);
  });

  test('Jobs tab loads without errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("JOBS")');

    await page.waitForTimeout(500);

    // Verify jobs view content
    await expect(page.locator('.jobs-view')).toBeVisible();
    await expect(page.locator('.jobs-tab')).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('Publish tab loads without errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PUBLISH")');

    await page.waitForTimeout(500);

    // Verify publish view content
    await expect(page.locator('.publish-view')).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('Can navigate between all tabs without crash', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    // Click through all tabs (4 tabs)
    const tabs = ['DASHBOARD', 'PIPELINES', 'JOBS', 'PUBLISH'];

    for (const tab of tabs) {
      await page.click(`button:has-text("${tab}")`);
      await page.waitForTimeout(300);

      // Verify tab button is active
      await expect(page.locator(`button:has-text("${tab}")`)).toHaveClass(/active/);
    }

    // Go back to Pipelines one more time to ensure repeated navigation works
    await page.click('button:has-text("PIPELINES")');
    await page.waitForTimeout(500);

    // No errors throughout navigation
    expect(errors).toHaveLength(0);
  });
});
