/**
 * File Processing E2E Tests
 *
 * Tests the flow: Select file → Assign plugin → Process → Verify status
 * These tests verify UI interactions work without JS errors.
 */
import { test, expect } from '@playwright/test';

test.describe('File Processing Flow', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('.logo-text')).toContainText('CASPARIAN');
    await page.click('button:has-text("PIPELINES")');
    await page.waitForTimeout(500);
  });

  test('Scout tab loads and shows file list or empty state', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    // Scout tab should be visible
    await expect(page.locator('.scout-tab')).toBeVisible();

    // Should show either file list or empty state
    const fileList = page.locator('.file-list');
    const emptyState = page.locator('.empty-state');

    // One of these should be visible
    const hasFileList = await fileList.isVisible();
    const hasEmptyState = await emptyState.isVisible();
    expect(hasFileList || hasEmptyState).toBe(true);

    expect(errors).toHaveLength(0);
  });

  test('Clicking file shows detail pane (when files exist)', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    const fileItems = page.locator('.file-item');
    const fileCount = await fileItems.count();

    if (fileCount > 0) {
      // Click first file
      await fileItems.first().click();
      await page.waitForTimeout(300);

      // Detail pane should appear
      await expect(page.locator('.detail-pane')).toBeVisible();

      // Should show file info sections
      await expect(page.locator('.section-title:has-text("FILE INFO")')).toBeVisible();
    }

    expect(errors).toHaveLength(0);
  });

  test('Process File button is visible for tagged files', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    const fileItems = page.locator('.file-item');
    const fileCount = await fileItems.count();

    if (fileCount > 0) {
      // Click first file
      await fileItems.first().click();
      await page.waitForTimeout(300);

      // Check if detail pane has actions section
      const actionsSection = page.locator('.actions-section');
      if (await actionsSection.isVisible()) {
        // Process button should exist (may be disabled if no plugin)
        const processBtn = page.locator('.action-btn.primary');
        await expect(processBtn).toBeVisible();
      }
    }

    expect(errors).toHaveLength(0);
  });

  test('Manual plugin override UI works', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    const fileItems = page.locator('.file-item');
    const fileCount = await fileItems.count();

    if (fileCount > 0) {
      // Click first file
      await fileItems.first().click();
      await page.waitForTimeout(300);

      // Look for plugin override dropdown/select
      const pluginSelect = page.locator('.plugin-select, select[name="plugin"]');
      if (await pluginSelect.isVisible()) {
        // Click to open dropdown
        await pluginSelect.click();
        await page.waitForTimeout(200);
      }
    }

    expect(errors).toHaveLength(0);
  });
});

test.describe('Jobs Tab', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('.logo-text')).toContainText('CASPARIAN');
  });

  test('Jobs tab loads and shows job list', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("JOBS")');
    await page.waitForTimeout(500);

    // Jobs tab should render
    await expect(page.locator('.jobs-tab')).toBeVisible();

    // Should show job list (may be empty)
    await expect(page.locator('.job-list')).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('Jobs tab filter buttons work', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("JOBS")');
    await page.waitForTimeout(500);

    // Click through filter buttons
    const filters = ['ALL', 'COMPLETED', 'FAILED', 'RUNNING'];
    for (const filter of filters) {
      const filterBtn = page.locator(`.filter-btn:has-text("${filter}")`);
      if (await filterBtn.isVisible()) {
        await filterBtn.click();
        await page.waitForTimeout(100);
      }
    }

    expect(errors).toHaveLength(0);
  });

  test('Clicking job shows log viewer', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("JOBS")');
    await page.waitForTimeout(500);

    const jobItems = page.locator('.job-item');
    const jobCount = await jobItems.count();

    if (jobCount > 0) {
      await jobItems.first().click();
      await page.waitForTimeout(300);

      // Log viewer should be visible
      await expect(page.locator('.log-viewer')).toBeVisible();
    }

    expect(errors).toHaveLength(0);
  });
});
