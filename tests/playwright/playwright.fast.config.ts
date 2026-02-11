/**
 * Fast Playwright config for local development.
 *
 * Differences from ui config:
 * - Chromium only (no webkit)
 * - 4 parallel workers
 * - Shorter timeouts
 * - No video/trace capture
 *
 * Usage: npx playwright test -c playwright.fast.config.ts
 */
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
const storageStatePath = path.resolve(repoRoot, 'var/playwright/storageState.json');
const repoTmpDir = path.resolve(repoRoot, 'var/tmp');

const baseURL = 'http://localhost:8080';
const reuseExistingServer = (process.env.PW_REUSE_EXISTING_SERVER ?? '').trim() === '1';
const watchdogEnabled = (process.env.PW_WATCHDOG ?? '').trim() === '1';
const devBypass = (process.env.PW_DEV_BYPASS ?? '').trim() === '1';

// IMPORTANT: System temp paths may be blocked in this environment; keep all temp files repo-local.
process.env.TMPDIR = repoTmpDir;
process.env.PW_DEV_BYPASS = devBypass ? '1' : '0';

const backendCommand = `bash -lc "set -euo pipefail; cd ${repoRoot} && echo '[pw] ensure aos-server is built' && test -x target/debug/aos-server || cargo build -p adapteros-server >/dev/null && echo '[pw] build ui assets' && ./scripts/build-ui.sh && echo '[pw] start aos-server' && mkdir -p var/tmp var/playwright/models/mistral-7b-instruct-v0.3-4bit var/playwright/run && rm -f var/playwright/aos-cp.sqlite3 var/playwright/aos-kv.redb var/playwright/run/aos-server.pid && rm -rf var/playwright/aos-kv-index && echo $$ > var/playwright/run/aos-server.pid && TMPDIR=${repoRoot}/var/tmp AOS_SERVER_PORT=8080 AOS_MANIFEST_PATH=${repoRoot}/manifests/mistral7b-4bit-mlx.yaml E2E_MODE=1 AOS_DEV_NO_AUTH=${devBypass ? '1' : '0'} AOS_STORAGE_MODE=sql_only AOS_DATABASE_URL=sqlite://var/playwright/aos-cp.sqlite3 AOS_KV_PATH=var/playwright/aos-kv.redb AOS_KV_TANTIVY_PATH=var/playwright/aos-kv-index AOS_MODEL_CACHE_DIR=var/playwright/models AOS_BASE_MODEL_ID=mistral-7b-instruct-v0.3-4bit AOS_DEV_JWT_SECRET=dev-secret AOS_SKIP_PREFLIGHT=1 AOS_RATE_LIMITS_REQUESTS_PER_MINUTE=10000 AOS_RATE_LIMITS_BURST_SIZE=2000 AOS_RATE_LIMITS_INFERENCE_PER_MINUTE=10000 exec target/debug/aos-server --config configs/cp.toml"`;

process.env.PLAYWRIGHT_BROWSERS_PATH ??= browserPath;
process.env.PW_TEST_TMPDIR ??= tmpDir;

export default defineConfig({
  testDir: 'ui',
  outputDir,
  workers: 4,
  fullyParallel: true,
  timeout: 30_000,
  expect: {
    timeout: 5_000,
  },
  retries: 0,
  reporter: watchdogEnabled
    ? [['list'], ['./reporters/heartbeat.js']]
    : [['list']],
  use: {
    baseURL,
    trace: 'off',
    screenshot: 'off',
    video: 'off',
    storageState: storageStatePath,
  },
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
  ],
  globalSetup: 'ui/global-setup.ts',
  globalTeardown: 'ui/global-teardown.ts',
  globalTimeout: 10 * 60_000,
  webServer: {
    command: backendCommand,
    url: `${baseURL}/healthz`,
    reuseExistingServer,
    timeout: 900_000,
  },
});
