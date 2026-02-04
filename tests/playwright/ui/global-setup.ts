import { chromium, expect, request, type FullConfig } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const backendBaseUrl = 'http://localhost:8080';
const uiBaseUrl = 'http://localhost:8080';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..', '..');
const storageStatePath = path.resolve(repoRoot, 'var/playwright/storageState.json');
const debugDir = path.resolve(repoRoot, 'var/playwright/debug');

async function waitForOk(url: string, timeoutMs = 60_000): Promise<void> {
  const api = await request.newContext();
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const resp = await api.get(url);
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
  await waitForOk(`${backendBaseUrl}/healthz`);
  try {
    await waitForOk(`${backendBaseUrl}/readyz`, 120_000);
  } catch (err) {
    // Ready checks can be stricter than needed for UI smoke in local E2E.
    // Continue once /healthz is up so tests can run against the API surface.
    console.warn(`[playwright] /readyz not OK yet: ${String(err)}`);
  }
  const api = await request.newContext({ baseURL: backendBaseUrl });

  const post = async (path: string, body?: Record<string, unknown>) => {
    const resp = await api.post(path, { data: body ?? {} });
    if (!resp.ok()) {
      const text = await resp.text();
      throw new Error(`Seed failed ${path}: ${resp.status()} ${text}`);
    }
  };

  await post('/testkit/reset');
  await post('/testkit/seed_minimal');
  await post('/testkit/create_repo', {
    repo_id: 'repo-e2e',
    base_model_id: 'model-qwen-test',
  });
  await post('/testkit/create_training_job_stub', { repo_id: 'repo-e2e' });
  await post('/testkit/create_trace_fixture', { token_count: 150 });
  await post('/testkit/create_diag_run_fixture');
  await post('/testkit/create_evidence_fixture');

  await api.dispose();
}

async function loginAndStoreState(): Promise<void> {
  fs.mkdirSync(path.dirname(storageStatePath), { recursive: true });
  fs.mkdirSync(debugDir, { recursive: true });
  await waitForOk(`${uiBaseUrl}/style-audit`, 120_000);
  const api = await request.newContext({ baseURL: backendBaseUrl });
  const resp = await api.post('/v1/auth/login', {
    data: { username: 'test@example.com', password: 'password' },
  });
  if (!resp.ok()) {
    const text = await resp.text();
    throw new Error(`Login failed: ${resp.status()} ${text}`);
  }
  await api.storageState({ path: storageStatePath });
  await api.dispose();

  const uiApi = await request.newContext({
    baseURL: uiBaseUrl,
    storageState: storageStatePath,
  });
  const meResp = await uiApi.get('/v1/auth/me');
  if (!meResp.ok()) {
    const text = await meResp.text();
    await uiApi.dispose();
    throw new Error(`Auth cookie rejected by UI proxy: ${meResp.status()} ${text}`);
  }
  await uiApi.dispose();

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
  const dashboard = page.getByRole('heading', { name: 'Dashboard' });
  try {
    await expect(dashboard).toBeVisible({ timeout: 60_000 });
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
}

export default async function globalSetup(_config: FullConfig) {
  await seedBackend();
  await loginAndStoreState();
}
