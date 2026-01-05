/**
 * Parser Lab E2E Tests
 *
 * Tests the Parser Lab workflow: create parser, load sample, test execution.
 * These tests ensure the critical parser development loop works correctly.
 *
 * Test Categories:
 * 1. UI Navigation - Tests that work without backend (Vite dev server)
 * 2. Backend Integration - Tests that require Tauri or Bridge mode
 *
 * For backend tests, run against actual Tauri app:
 *   bun run tauri dev
 *   # Then in another terminal:
 *   PLAYWRIGHT_BASE_URL=http://localhost:1420 bun run test:e2e
 */

import { test, expect } from '@playwright/test';

// Check if running with backend (Tauri or Bridge)
// In browser context, we can't detect this, so we'll check for error messages
const skipBackendTests = process.env.SKIP_BACKEND_TESTS === 'true';

test.describe('Parser Lab - UI Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('.logo-text')).toContainText('CASPARIAN');
  });

  test('Parser Lab tab loads without errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.waitForTimeout(500);

    // Verify Parser Lab content is visible
    await expect(page.locator('.parser-lab')).toBeVisible();

    // No JavaScript errors (except expected backend unavailable)
    const unexpectedErrors = errors.filter(
      e => !e.includes('Backend not available')
    );
    expect(unexpectedErrors).toHaveLength(0);
  });

  test('Parser Lab shows action buttons', async ({ page }) => {
    await page.click('button:has-text("PARSER LAB")');
    await page.waitForTimeout(500);

    // Check action buttons are visible
    await expect(page.locator('button:has-text("Open File")')).toBeVisible();
    await expect(page.locator('button:has-text("Load Parser Code")')).toBeVisible();
    await expect(page.locator('button:has-text("Load Sample")')).toBeVisible();
  });

  test('Parser Lab shows empty state with helpful text', async ({ page }) => {
    await page.click('button:has-text("PARSER LAB")');
    await page.waitForTimeout(500);

    // Check for empty state message
    await expect(page.locator('.parser-lab')).toContainText('Recent');
    await expect(page.locator('.parser-lab')).toContainText('Open a file');
  });
});

test.describe('Parser Lab - Backend Integration', () => {
  // These tests require Tauri or Bridge mode
  // Skip in pure Vite dev mode
  test.skip(skipBackendTests, 'Backend tests skipped - set SKIP_BACKEND_TESTS=false to run');

  test.beforeEach(async ({ page }) => {
    // Enable bridge mode for backend tests
    await page.addInitScript(() => {
      (window as any).__CASPARIAN_BRIDGE__ = true;
    });

    await page.goto('/');
    await expect(page.locator('.logo-text')).toContainText('CASPARIAN');
  });

  test('Load Sample creates a parser with code', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.waitForTimeout(500);

    // Click Load Sample button
    await page.click('button:has-text("Load Sample")');

    // Wait for editor to open
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Verify Monaco editor is visible (code was loaded)
    await expect(page.locator('.monaco-editor')).toBeVisible({ timeout: 5000 });

    // Verify no errors during load
    expect(errors).toHaveLength(0);
  });

  test('Parser editor shows data preview', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.waitForTimeout(500);

    // Load sample to get into editor
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Check data preview panel is visible
    await expect(page.locator('.data-panel')).toBeVisible();

    // Check preview content exists (data was loaded)
    await expect(page.locator('.preview-content')).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('Data preview panel scrolls without truncation', async ({ page }) => {
    await page.click('button:has-text("PARSER LAB")');
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Get preview content
    const preview = page.locator('.preview-content');
    await expect(preview).toBeVisible();

    // Check if content can scroll (overflow is not hidden)
    const overflowStyle = await preview.evaluate(el => {
      const computed = getComputedStyle(el);
      return computed.overflowY;
    });

    // Should allow scrolling (auto or scroll, not hidden)
    expect(['auto', 'scroll']).toContain(overflowStyle);
  });

  test('Test button runs validation', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Click Test button (Ctrl+Enter or button click)
    await page.click('button:has-text("Test")');

    // Wait for validation to complete (may take time due to Python execution)
    // Either we get a valid badge, invalid badge, or an error
    await expect(
      page.locator('.badge.valid, .badge.invalid, .error-text')
    ).toBeVisible({ timeout: 60000 });

    expect(errors).toHaveLength(0);
  });

  test('Can navigate back from parser editor', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Click back button
    await page.click('button:has-text("Back")');

    // Should be back at parser list
    await expect(page.locator('.parser-lab')).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('Parser persists after navigation', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    // Load sample parser
    await page.click('button:has-text("PARSER LAB")');
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Click Save to ensure parser is persisted
    await page.click('button:has-text("Save")');
    await page.waitForTimeout(500);

    // Click Back to go to parser list
    await page.click('button:has-text("Back")');
    await expect(page.locator('.parser-lab')).toBeVisible({ timeout: 5000 });

    // Verify parser appears in the list (Sample Parser should be visible)
    await expect(page.locator('text=Sample Parser')).toBeVisible({ timeout: 5000 });

    expect(errors).toHaveLength(0);
  });

  test('AI Assistant panel has Analyze Structure button', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Wait for data to load (needed for Analyze Structure button to appear)
    await expect(page.locator('.preview-content pre')).toBeVisible({ timeout: 5000 });

    // Check AI chat panel is visible
    await expect(page.locator('.parser-chat')).toBeVisible();

    // Check Analyze Structure button exists
    await expect(page.locator('.analyze-btn')).toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('Output section shows correct badge for single-output parser', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Run validation
    await page.click('button:has-text("Test")');

    // Wait for validation to complete
    await expect(page.locator('.badge.valid')).toBeVisible({ timeout: 60000 });

    // For single-output parser, there should be NO "N tables" badge
    await expect(page.locator('.badge.multi')).not.toBeVisible();

    // Output should show plain pre content, not collapsible tables
    await expect(page.locator('.output-content pre')).toBeVisible();
    await expect(page.locator('.multi-output')).not.toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('Sink config visible for single-output parser', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Click Output tab to see sink config
    await page.click('button:has-text("Output")');
    await page.waitForTimeout(300);

    // For single output parser, standard sink config should be visible
    // Use first() since there are multiple selects (Sink Type and Compression)
    await expect(page.locator('.single-output-config select').first()).toBeVisible();

    // Multi-sink config should NOT be visible
    await expect(page.locator('.multi-sink-config')).not.toBeVisible();

    expect(errors).toHaveLength(0);
  });

  test('Validation output section exists and is scrollable', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', err => errors.push(err.message));

    await page.click('button:has-text("PARSER LAB")');
    await page.click('button:has-text("Load Sample")');
    await expect(page.locator('.file-editor')).toBeVisible({ timeout: 10000 });

    // Run validation to populate output
    await page.click('button:has-text("Test")');
    await expect(page.locator('.badge.valid, .badge.invalid')).toBeVisible({ timeout: 60000 });

    // Check output content section can scroll
    const outputContent = page.locator('.output-content');
    await expect(outputContent).toBeVisible();

    const overflowStyle = await outputContent.evaluate(el => {
      const computed = getComputedStyle(el);
      return computed.overflowY;
    });

    expect(['auto', 'scroll']).toContain(overflowStyle);

    expect(errors).toHaveLength(0);
  });
});
