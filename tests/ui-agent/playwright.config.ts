/**
 * Playwright Configuration for Ephemeral Backend Testing
 *
 * This configuration:
 * 1. Runs global-setup.ts to create test DB and spawn bridge
 * 2. Starts Vite dev server for the UI
 * 3. Injects bridge mode into the browser
 */

import { defineConfig, devices } from '@playwright/test';
import * as path from 'path';

const uiDir = path.resolve(__dirname, '../../ui');

export default defineConfig({
  testDir: '.',
  testMatch: '**/*.test.ts',

  /* Run tests serially to avoid DB conflicts */
  fullyParallel: false,
  workers: 1,

  /* Fail the build on CI if you accidentally left test.only */
  forbidOnly: !!process.env.CI,

  /* Retry on CI only */
  retries: process.env.CI ? 2 : 0,

  /* Reporter */
  reporter: [
    ['list'],
    ['html', { open: 'never' }],
  ],

  /* Shared settings */
  use: {
    baseURL: 'http://localhost:1420',
    trace: 'on-first-retry',
    screenshot: 'on',
    video: 'on-first-retry',
  },

  /* Global setup/teardown */
  globalSetup: require.resolve('./global-setup.ts'),

  /* Test timeout */
  timeout: 30000,

  /* Projects */
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
      },
    },
  ],

  /* Web server - Vite dev */
  webServer: {
    command: `cd ${uiDir} && bun run dev --port 1420`,
    url: 'http://localhost:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 30000,
    stdout: 'pipe',
    stderr: 'pipe',
  },

  /* Output directory for test artifacts */
  outputDir: './test-results',
});
