import { chromium, expect, request, type FullConfig } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const useDevBypass = (process.env.PW_DEV_BYPASS ?? '').trim() === '1';

const backendBaseUrl = 'http://localhost:8080';
const uiBaseUrl = 'http://localhost:8080';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..', '..');
const storageStatePath = path.resolve(repoRoot, 'var/playwright/storageState.json');
const debugDir = path.resolve(repoRoot, 'var/playwright/debug');
const heartbeatPath = path.resolve(repoRoot, 'var/playwright/heartbeat.json');

function writeHeartbeat(payload: Record<string, unknown>): void {
  fs.mkdirSync(path.dirname(heartbeatPath), { recursive: true });
  const tmpPath = `${heartbeatPath}.tmp`;
  fs.writeFileSync(tmpPath, JSON.stringify({ ts: Date.now(), ...payload }) + '\n');
  fs.renameSync(tmpPath, heartbeatPath);
}

function beat(stage: string, extra: Record<string, unknown> = {}): void {
  writeHeartbeat({ event: 'global_setup', stage, ...extra });
}

async function waitForOk(url: string, timeoutMs = 60_000): Promise<void> {
  const api = await request.newContext();
  const start = Date.now();
  let lastTick = 0;
  while (Date.now() - start < timeoutMs) {
    if (Date.now() - lastTick >= 10_000) {
      // Emit a heartbeat tick so watchdogs can distinguish "still polling" from a hang.
      writeHeartbeat({ event: 'wait_for_ok', url, elapsedMs: Date.now() - start });
      lastTick = Date.now();
    }
    try {
      const resp = await api.get(url, { timeout: 5_000 });
      if (resp.ok()) {
        await api.dispose();
        return;
      }
    } catch {
      // Retry until timeout.
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  await api.dispose();
  throw new Error(`Timed out waiting for ${url}`);
}

async function seedBackend(): Promise<void> {
  beat('seed_backend:start', { backendBaseUrl });
  await waitForOk(`${backendBaseUrl}/healthz`);
  beat('seed_backend:healthz_ok');
  // /readyz can 503 when worker/model checks fail (expected for UI-only E2E).
  // For deterministic seeding we only need the DB online + migrations applied.
  beat('seed_backend:healthz_db_wait');
  await waitForOk(`${backendBaseUrl}/healthz/db`, 180_000);
  beat('seed_backend:healthz_db_ok');
  const api = await request.newContext({ baseURL: backendBaseUrl });

  const post = async (path: string, body?: Record<string, unknown>) => {
    const resp = await api.post(path, { data: body ?? {}, timeout: 30_000 });
    if (!resp.ok()) {
      const text = await resp.text();
      throw new Error(`Seed failed ${path}: ${resp.status()} ${text}`);
    }
  };

  const reuseExistingServer = (process.env.PW_REUSE_EXISTING_SERVER ?? '').trim() === '1';

  // The webServer command already starts from a fresh DB when not reusing an existing server.
  // Avoid extra reset calls to reduce flake from concurrent local dev traffic.
  const maxSeedAttempts = 3;
  for (let attempt = 1; attempt <= maxSeedAttempts; attempt += 1) {
    try {
      if (reuseExistingServer || attempt > 1) {
        beat('seed_backend:testkit_reset', { attempt });
        await post('/testkit/reset');
      } else {
        beat('seed_backend:testkit_reset_skip', { attempt, reason: 'fresh_db' });
      }

      beat('seed_backend:seed_minimal', { attempt });
      await post('/testkit/seed_minimal');
      break;
    } catch (err) {
      beat('seed_backend:seed_minimal_failed', { attempt, error: String(err) });
      if (attempt === maxSeedAttempts) throw err;
      await new Promise((r) => setTimeout(r, 500 * attempt));
    }
  }
  beat('seed_backend:create_document_fixture');
  await post('/testkit/create_document_fixture', {
    document_id: 'doc-fixture',
    status: 'ready',
    name: 'Fixture Document',
  });
  beat('seed_backend:create_collection_fixture');
  await post('/testkit/create_collection_fixture', {
    collection_id: 'collection-test',
    document_id: 'doc-fixture',
    name: 'Test Collection',
  });
  beat('seed_backend:create_dataset_fixture');
  await post('/testkit/create_dataset_fixture', {
    dataset_id: 'dataset-test',
    name: 'Test Dataset',
  });
  beat('seed_backend:create_worker_fixture');
  await post('/testkit/create_worker_fixture', {
    worker_id: 'worker-test',
  });
  beat('seed_backend:create_repo');
  await post('/testkit/create_repo', {
    repo_id: 'repo-e2e',
    base_model_id: 'model-qwen-test',
  });
  // Note: create_adapter_version skipped - no tests currently use adapterVersionId
  // and the endpoint has FK constraint issues that need backend investigation
  beat('seed_backend:create_training_job_stub');
  await post('/testkit/create_training_job_stub', { repo_id: 'repo-e2e' });
  beat('seed_backend:create_trace_fixture');
  await post('/testkit/create_trace_fixture', { token_count: 150 });
  beat('seed_backend:create_diag_run_fixture');
  await post('/testkit/create_diag_run_fixture');
  beat('seed_backend:create_evidence_fixture');
  await post('/testkit/create_evidence_fixture');

  await api.dispose();
  beat('seed_backend:done');
}

async function loginAndStoreState(): Promise<void> {
  if (useDevBypass) {
    // Dev bypass means we don't need cookies for auth; keep storageState minimal.
    fs.mkdirSync(path.dirname(storageStatePath), { recursive: true });
    fs.writeFileSync(
      storageStatePath,
      JSON.stringify({ cookies: [], origins: [] }, null, 2) + '\n'
    );
    beat('login:skipped_dev_bypass');
    return;
  }

  beat('login:start', { uiBaseUrl });
  fs.mkdirSync(path.dirname(storageStatePath), { recursive: true });
  fs.mkdirSync(debugDir, { recursive: true });
  await waitForOk(`${uiBaseUrl}/style-audit`, 120_000);
  beat('login:style_audit_ok');
  const api = await request.newContext({ baseURL: backendBaseUrl });
  const resp = await api.post('/v1/auth/login', {
    data: { username: 'test@example.com', password: 'password' },
    timeout: 30_000,
  });
  if (!resp.ok()) {
    const text = await resp.text();
    throw new Error(`Login failed: ${resp.status()} ${text}`);
  }
  beat('login:api_ok');
  await api.storageState({ path: storageStatePath });
  await api.dispose();

  const uiApi = await request.newContext({
    baseURL: uiBaseUrl,
    storageState: storageStatePath,
  });
  const meResp = await uiApi.get('/v1/auth/me', { timeout: 30_000 });
  if (!meResp.ok()) {
    const text = await meResp.text();
    await uiApi.dispose();
    throw new Error(`Auth cookie rejected by UI proxy: ${meResp.status()} ${text}`);
  }
  await uiApi.dispose();
  beat('login:cookie_ok');

  const browser = await chromium.launch();
  const context = await browser.newContext({
    storageState: storageStatePath,
  });
  const page = await context.newPage();
  const requestFailures: string[] = [];
  const consoleErrors: string[] = [];
  page.on('requestfailed', (req) => {
    const failure = req.failure();
    requestFailures.push(
      `${req.method()} ${req.url()} -> ${failure?.errorText ?? 'unknown'}`
    );
  });
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      consoleErrors.push(msg.text());
    }
  });
  await page.goto(`${uiBaseUrl}/dashboard`, { waitUntil: 'domcontentloaded' });
  beat('login:dashboard_navigated');
  await page.waitForFunction(
    () => {
      const boot = (window as any).aosBoot;
      const progress = document.getElementById('aos-boot-progress');
      if (boot?.stages?.mount?.status === 'done') {
        return true;
      }
      if (progress && progress.classList.contains('hidden')) {
        return true;
      }
      return false;
    },
    { timeout: 60_000 }
  );
  beat('login:boot_done');
  const dashboard = page.getByRole('heading', { name: 'Dashboard' });
  try {
    await expect(dashboard).toBeVisible({ timeout: 60_000 });
    beat('login:dashboard_visible');
  } catch (err) {
    const login = page.getByRole('heading', { name: 'Login' });
    const authError = page.getByRole('heading', { name: 'Authentication Error' });
    const authTimeout = page.getByRole('heading', { name: 'Authentication Timeout' });
    const loading = page.getByText('Signing you in');
    const onLogin = await login.isVisible().catch(() => false);
    const onAuthError = await authError.isVisible().catch(() => false);
    const onAuthTimeout = await authTimeout.isVisible().catch(() => false);
    const onLoading = await loading.isVisible().catch(() => false);
    const html = await page.content();
    const scriptSrcs = await page.evaluate(() =>
      Array.from(document.querySelectorAll('script[src]')).map(
        (el) => (el as HTMLScriptElement).src
      )
    );
    await page.screenshot({
      path: path.join(debugDir, 'global-setup-dashboard.png'),
      fullPage: true,
    });
    const origin = await page.evaluate(() => window.location.origin);
    await browser.close();
    beat('login:dashboard_not_visible', {
      url: page.url(),
      origin,
      requestFailures: requestFailures.length,
      consoleErrors: consoleErrors.length,
    });
    throw new Error(
      `Dashboard not visible after 60s (url=${page.url()}, origin=${origin}, loading=${onLoading}, login=${onLogin}, auth_error=${onAuthError}, auth_timeout=${onAuthTimeout}).\n` +
        `Request failures: ${requestFailures.join(' | ') || 'none'}\n` +
        `Console errors: ${consoleErrors.join(' | ') || 'none'}\n` +
        `Script srcs: ${scriptSrcs.join(', ') || 'none'}\n` +
        `HTML snapshot (first 500 chars): ${html.slice(0, 500).replace(/\n/g, ' ')}\n` +
        `Screenshot: ${path.join(debugDir, 'global-setup-dashboard.png')}`
    );
  }
  await browser.close();
  beat('login:done');
}

export default async function globalSetup(_config: FullConfig) {
  beat('global_setup:start');
  await seedBackend();
  await loginAndStoreState();
  beat('global_setup:done');
}
