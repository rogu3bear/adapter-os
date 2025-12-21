import { defineConfig, devices } from '@playwright/test';

// Port offset strategy: PLAYWRIGHT_BASE_URL=http://localhost:${AOS_UI_PORT}
// Default: 3200, Developer B: 3300 (+100), Developer C: 3400 (+200)
const baseURL = process.env.PLAYWRIGHT_BASE_URL || 'http://localhost:3200';

// Port offset strategy: PLAYWRIGHT_API_BASE_URL=http://localhost:${AOS_SERVER_PORT}
// Default: 8080, Developer B: 8180 (+100), Developer C: 8280 (+200)
// Access in tests via: process.env.PLAYWRIGHT_API_BASE_URL || 'http://localhost:8080'
export const API_BASE_URL = process.env.PLAYWRIGHT_API_BASE_URL || 'http://localhost:8080';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  retries: 0,
  use: {
    baseURL,
    trace: 'retain-on-failure',
    actionTimeout: 10000,
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});

