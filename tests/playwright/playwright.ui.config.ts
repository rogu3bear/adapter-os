import { defineConfig } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function sanitizeRunId(value: string): string {
  const cleaned = value.trim().replace(/[^A-Za-z0-9._-]/g, '_');
  return cleaned || 'default';
}

function parseServerPort(value: string | undefined): number {
  const parsed = Number.parseInt((value ?? '8080').trim(), 10);
  if (!Number.isFinite(parsed) || parsed < 1 || parsed > 65535) {
    return 8080;
  }
  return parsed;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..');
const runId = sanitizeRunId(process.env.PW_RUN_ID ?? 'default');
const runRootDir = path.resolve(repoRoot, 'var/playwright/runs', runId);
const runRootRel = `var/playwright/runs/${runId}`;
const serverPort = parseServerPort(process.env.PW_SERVER_PORT);
const browserPath = path.resolve(repoRoot, 'var/playwright/browsers');
const tmpDir = path.resolve(runRootDir, 'tmp');
const outputDir = path.resolve(runRootDir, 'test-results');
const reportDir = path.resolve(runRootDir, 'report');
const storageStatePath = path.resolve(runRootDir, 'storageState.json');
const repoTmpDir = path.resolve(repoRoot, 'var/tmp');

const baseURL = `http://localhost:${serverPort}`;
const reuseExistingServer = (process.env.PW_REUSE_EXISTING_SERVER ?? '').trim() === '1';
const watchdogEnabled = (process.env.PW_WATCHDOG ?? '').trim() === '1';
const devBypass = (process.env.PW_DEV_BYPASS ?? '').trim() === '1';

// IMPORTANT: System temp paths may be blocked in this environment; keep all temp files repo-local.
process.env.TMPDIR = repoTmpDir;
process.env.PW_DEV_BYPASS = devBypass ? '1' : '0';
process.env.PW_RUN_ID = runId;
process.env.PW_SERVER_PORT = String(serverPort);

const backendCommand = `bash -lc "set -euo pipefail; cd ${repoRoot} && mkdir -p var/tmp var/playwright/models/mistral-7b-instruct-v0.3-4bit var/playwright/locks ${runRootRel}/run && if [ ! -f crates/adapteros-server/static/index.html ]; then echo '[pw] build ui assets' && if command -v lockf >/dev/null 2>&1; then lockf var/playwright/locks/ui-build.lock ./scripts/build-ui.sh; else ./scripts/build-ui.sh; fi; else echo '[pw] ui assets present; skipping build'; fi && if [ ! -x target/debug/aos-server ]; then echo '[pw] build aos-server' && if command -v lockf >/dev/null 2>&1; then lockf var/playwright/locks/aos-server-build.lock cargo build -p adapteros-server; else cargo build -p adapteros-server; fi; fi && echo '[pw] start aos-server' && rm -f ${runRootRel}/aos-cp.sqlite3 ${runRootRel}/aos-kv.redb ${runRootRel}/run/aos-server.pid ${runRootRel}/run/aos-cp-single-writer.pid && rm -rf ${runRootRel}/aos-kv-index && echo $$ > ${runRootRel}/run/aos-server.pid && TMPDIR=${repoRoot}/var/tmp AOS_SERVER_PORT=${serverPort} AOS_MANIFEST_PATH=${repoRoot}/manifests/mistral7b-4bit-mlx.yaml E2E_MODE=1 AOS_DEV_NO_AUTH=${devBypass ? '1' : '0'} AOS_STORAGE_MODE=sql_only AOS_DATABASE_URL=sqlite://${runRootRel}/aos-cp.sqlite3 AOS_KV_PATH=${runRootRel}/aos-kv.redb AOS_KV_TANTIVY_PATH=${runRootRel}/aos-kv-index AOS_MODEL_CACHE_DIR=var/playwright/models AOS_BASE_MODEL_ID=mistral-7b-instruct-v0.3-4bit AOS_DEV_JWT_SECRET=dev-secret AOS_SKIP_PREFLIGHT=1 AOS_MIGRATION_TIMEOUT_SECS=600 AOS_RATE_LIMITS_REQUESTS_PER_MINUTE=10000 AOS_RATE_LIMITS_BURST_SIZE=2000 AOS_RATE_LIMITS_INFERENCE_PER_MINUTE=10000 exec target/debug/aos-server --config configs/cp.toml --pid-file ${runRootRel}/run/aos-cp-single-writer.pid"`;

process.env.PLAYWRIGHT_BROWSERS_PATH ??= browserPath;
process.env.PW_TEST_TMPDIR ??= tmpDir;

export default defineConfig({
  testDir: 'ui',
  outputDir,
  // Default to 1 worker for safety; use --workers=N for parallelism
  // Most UI tests are read-only and can run in parallel
  workers: process.env.CI ? 2 : 1,
  // Allow parallel execution within spec files for independent tests
  fullyParallel: false,
  timeout: 60_000,
  expect: {
    timeout: 10_000,
  },
  reporter: watchdogEnabled
    ? [
        ['line'],
        ['html', { outputFolder: reportDir, open: 'never' }],
        ['./reporters/heartbeat.js'],
      ]
    : [['html', { outputFolder: reportDir, open: 'never' }]],
  use: {
    baseURL,
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
  globalTimeout: 15 * 60_000,
  webServer: {
    command: backendCommand,
    url: `${baseURL}/healthz`,
    reuseExistingServer,
    timeout: 900_000,
  },
});
