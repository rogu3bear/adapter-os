import { defineConfig } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..');

const browserPath = path.resolve(repoRoot, 'var/playwright/browsers');
const tmpDir = path.resolve(repoRoot, 'var/playwright/tmp');
const outputDir = path.resolve(repoRoot, 'var/playwright/local-test-results');
const reportDir = path.resolve(repoRoot, 'var/playwright/local-report');

process.env.PLAYWRIGHT_BROWSERS_PATH ??= browserPath;
process.env.PW_TEST_TMPDIR ??= tmpDir;
process.env.PW_EXPECT_FIXTURES ??= '0';

// Local suite: assumes you already have adapterOS running (e.g. AOS_DEV_NO_AUTH=1 ./start backend).
// No testkit seeding, no webServer wrapper, no stored auth state by default.
const baseURLRaw = (process.env.PW_BASE_URL ?? '').trim();
const serverPortRaw = (process.env.AOS_SERVER_PORT ?? '').trim();
const baseURL =
  baseURLRaw ||
  `http://127.0.0.1:${serverPortRaw || '18080'}`;

export default defineConfig({
  testDir: 'ui',
  outputDir,
  workers: 1,
  fullyParallel: false,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [['html', { outputFolder: reportDir, open: 'never' }]],
  use: {
    baseURL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { browserName: 'chromium' } }],
});
