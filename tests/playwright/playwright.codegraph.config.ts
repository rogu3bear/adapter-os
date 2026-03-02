import { defineConfig } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..');
const browserPath = path.resolve(repoRoot, 'var/playwright/browsers');
const tmpDir = path.resolve(repoRoot, 'var/playwright/tmp');
const outputDir = path.resolve(repoRoot, 'var/playwright/test-results');
const reportDir = path.resolve(repoRoot, 'var/playwright/report');

process.env.PLAYWRIGHT_BROWSERS_PATH ??= browserPath;
process.env.PW_TEST_TMPDIR ??= tmpDir;

export default defineConfig({
  testDir: 'codegraph',
  outputDir,
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  reporter: [['html', { outputFolder: reportDir, open: 'never' }]],
  use: {
    baseURL: 'http://127.0.0.1:5173',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
    { name: 'webkit', use: { browserName: 'webkit' } },
  ],
  webServer: {
    command:
      'cd ../../crates/adapteros-codegraph-viewer/frontend && VITE_CODEGRAPH_TEST_DATA=1 npm run dev -- --host 127.0.0.1 --port 5173',
    url: 'http://127.0.0.1:5173',
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
