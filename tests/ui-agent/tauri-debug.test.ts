/**
 * Tauri Detection Debug Test
 *
 * This test does NOT use bridge mode - it checks what the UI sees
 * when running against a real backend (or falling back to mock).
 */

import { test, expect } from '@playwright/test';

const APP_URL = 'http://localhost:1420';

test.describe('Tauri Detection Debug', () => {
  test('should detect environment and show correct metrics', async ({ page }) => {
    // Collect console logs
    const consoleLogs: string[] = [];
    page.on('console', msg => {
      const text = msg.text();
      consoleLogs.push(`[${msg.type()}] ${text}`);
      if (text.includes('Tauri')) {
        console.log('>>> TAURI LOG:', text);
      }
    });

    // Navigate WITHOUT bridge mode
    await page.goto(APP_URL);
    await page.waitForLoadState('networkidle');

    // Wait for metrics to load
    await page.waitForTimeout(2000);

    // Check what's in the window
    const windowInfo = await page.evaluate(() => {
      return {
        hasTauri: '__TAURI__' in window,
        hasTauriInternals: '__TAURI_INTERNALS__' in window,
        hasBridge: '__CASPARIAN_BRIDGE__' in window,
        tauriKeys: Object.keys(window).filter(k => k.toLowerCase().includes('tauri')),
        tauriValue: (window as any).__TAURI__ ? 'object' : 'undefined',
      };
    });

    console.log('\n=== WINDOW STATE ===');
    console.log('Has __TAURI__:', windowInfo.hasTauri);
    console.log('Has __TAURI_INTERNALS__:', windowInfo.hasTauriInternals);
    console.log('Has __CASPARIAN_BRIDGE__:', windowInfo.hasBridge);
    console.log('Tauri-related keys:', windowInfo.tauriKeys);
    console.log('__TAURI__ value type:', windowInfo.tauriValue);

    // Get displayed metrics from the UI
    const metrics = await page.evaluate(() => {
      const getText = (selector: string) => {
        const el = document.querySelector(selector);
        return el?.textContent?.trim() || 'NOT_FOUND';
      };

      // Try to find metric values
      const allText = document.body.innerText;

      // Extract numbers near labels
      const msgSentMatch = allText.match(/(\d[\d,]*)\s*MSG SENT/);
      const msgRecvMatch = allText.match(/(\d[\d,]*)\s*MSG RECV/);
      const completedMatch = allText.match(/(\d[\d,]*)\s*COMPLETED/);
      const dispatchedMatch = allText.match(/(\d[\d,]*)\s*DISPATCHED/);

      return {
        msgSent: msgSentMatch?.[1] || 'NOT_FOUND',
        msgRecv: msgRecvMatch?.[1] || 'NOT_FOUND',
        completed: completedMatch?.[1] || 'NOT_FOUND',
        dispatched: dispatchedMatch?.[1] || 'NOT_FOUND',
        fullText: allText.substring(0, 500),
      };
    });

    console.log('\n=== DISPLAYED METRICS ===');
    console.log('MSG SENT:', metrics.msgSent);
    console.log('MSG RECV:', metrics.msgRecv);
    console.log('COMPLETED:', metrics.completed);
    console.log('DISPATCHED:', metrics.dispatched);

    // Check if using mock (mock starts at 1000/998/150/155)
    const msgSentNum = parseInt(metrics.msgSent.replace(/,/g, ''), 10);
    const isMockData = msgSentNum >= 1000;

    console.log('\n=== DIAGNOSIS ===');
    console.log('Using MOCK data:', isMockData);
    console.log('MSG SENT value:', msgSentNum);

    // Print relevant console logs
    console.log('\n=== TAURI-RELATED CONSOLE LOGS ===');
    consoleLogs
      .filter(l => l.toLowerCase().includes('tauri'))
      .forEach(l => console.log(l));

    // The test should show us what's happening
    // For now, just verify page loaded
    await expect(page.locator('body')).toBeVisible();
  });

  test('should check footer bind address', async ({ page }) => {
    await page.goto(APP_URL);
    await page.waitForLoadState('networkidle');
    await page.waitForTimeout(1500);

    // Get the footer content
    const footerText = await page.evaluate(() => {
      const footer = document.querySelector('footer');
      return footer?.textContent || 'NO_FOOTER';
    });

    console.log('\n=== FOOTER CONTENT ===');
    console.log(footerText);

    // Check for mock vs real socket path
    const isMockSocket = footerText.includes('mock://') || footerText.includes('localhost:5555');
    const isRealSocket = footerText.includes('ipc://') && !footerText.includes('mock');

    console.log('Is mock socket:', isMockSocket);
    console.log('Is real socket:', isRealSocket);
  });
});
