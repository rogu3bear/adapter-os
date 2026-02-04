import { defineConfig } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..');
const outputDir = path.resolve(repoRoot, 'var/playwright/test-results');
const reportDir = path.resolve(repoRoot, 'var/playwright/report');
const storageStatePath = path.resolve(repoRoot, 'var/playwright/storageState.json');
const backendCommand = `bash -lc "cd ${repoRoot} && mkdir -p var/playwright/models/mistral-7b-instruct-v0.3-4bit && rm -f var/playwright/aos-cp.sqlite3 var/playwright/aos-kv.redb && rm -rf var/playwright/aos-kv-index && AOS_MANIFEST_PATH=${repoRoot}/manifests/mistral7b-4bit-mlx.yaml E2E_MODE=1 AOS_DEV_NO_AUTH=0 AOS_DATABASE_URL=sqlite://var/playwright/aos-cp.sqlite3 AOS_KV_PATH=var/playwright/aos-kv.redb AOS_KV_TANTIVY_PATH=var/playwright/aos-kv-index AOS_MODEL_CACHE_DIR=var/playwright/models AOS_BASE_MODEL_ID=mistral-7b-instruct-v0.3-4bit AOS_DEV_JWT_SECRET=dev-secret AOS_SKIP_PREFLIGHT=1 target/debug/adapteros-server --config configs/cp.toml"`;

export default defineConfig({
  testDir: 'ui',
  outputDir,
  workers: 1,
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  reporter: [['html', { outputFolder: reportDir, open: 'never' }]],
  use: {
    baseURL: 'http://localhost:8080',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    storageState: storageStatePath,
  },
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' } },
    { name: 'webkit', use: { browserName: 'webkit' } },
  ],
  globalSetup: 'ui/global-setup.ts',
  globalTeardown: 'ui/global-teardown.ts',
  webServer: {
    command: backendCommand,
    url: 'http://localhost:8080/healthz',
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
