import { defineConfig, devices } from '@playwright/test';

// Check if running in bridge mode (for backend integration tests)
const useBridge = process.env.USE_BRIDGE === 'true';

export default defineConfig({
  testDir: './e2e',
  testMatch: '**/*.spec.ts',
  timeout: 30000,
  retries: 0,
  use: {
    baseURL: 'http://localhost:1420',
    headless: true,
    screenshot: 'only-on-failure',
  },
  webServer: useBridge ? [
    // Bridge server for backend commands
    {
      command: 'bun run scripts/test-bridge.ts',
      url: 'http://localhost:9999/health',
      reuseExistingServer: !process.env.CI,
      timeout: 15000,
    },
    // Vite dev server for frontend
    {
      command: 'bun run dev',
      url: 'http://localhost:1420',
      reuseExistingServer: !process.env.CI,
      timeout: 10000,
    },
  ] : {
    // Just Vite dev server (for UI-only tests)
    command: 'bun run dev',
    url: 'http://localhost:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 10000,
  },
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        // Inject bridge mode flag when using bridge
        ...(useBridge && {
          contextOptions: {
            // This will be set via addInitScript in the test
          },
        }),
      },
    },
  ],
});
