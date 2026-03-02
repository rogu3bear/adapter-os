import { chromium, expect, request, type FullConfig } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const useDevBypass = (process.env.PW_DEV_BYPASS ?? '').trim() === '1';

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
const repoRoot = path.resolve(__dirname, '..', '..', '..');
const runId = sanitizeRunId(process.env.PW_RUN_ID ?? 'default');
const serverPort = parseServerPort(process.env.PW_SERVER_PORT);
const runRoot = path.resolve(repoRoot, 'var/playwright/runs', runId);
const backendBaseUrl = `http://localhost:${serverPort}`;
const uiBaseUrl = `http://localhost:${serverPort}`;
const storageStatePath = path.resolve(runRoot, 'storageState.json');
const debugDir = path.resolve(runRoot, 'debug');
const heartbeatPath = path.resolve(runRoot, 'heartbeat.json');
const setupLogPath = path.resolve(debugDir, 'global-setup.ndjson');
const setupSummaryPath = path.resolve(debugDir, 'global-setup-summary.json');

type AttemptResult = 'ok' | 'retry' | 'failed';
type RecoveryPathUsed = 'none' | 'user_not_found_reset_seed_minimal';

type LoginAttemptDiagnostic = {
  attempt: number;
  loginStatus: number;
  meStatus?: number;
  meErrorCode?: string | null;
  userNotFound: boolean;
  recoveryPathUsed: RecoveryPathUsed;
  recoveredFromUserNotFound: boolean;
  result: AttemptResult;
  elapsedMs: number;
  loginElapsedMs?: number;
  authMeElapsedMs?: number;
};

type GlobalSetupSummary = {
  runId: string;
  serverPort: number;
  startedAt: string;
  completedAt?: string;
  success: boolean;
  failureMessage?: string;
  recoveredUserNotFound: boolean;
  recoveryAttempts: number;
  loginAttempts: LoginAttemptDiagnostic[];
  eventsWritten: number;
};

const setupSummary: GlobalSetupSummary = {
  runId,
  serverPort,
  startedAt: new Date().toISOString(),
  success: false,
  recoveredUserNotFound: false,
  recoveryAttempts: 0,
  loginAttempts: [],
  eventsWritten: 0,
};
const USER_NOT_FOUND_RECOVERY_PATH: RecoveryPathUsed = 'user_not_found_reset_seed_minimal';
const MAX_USER_NOT_FOUND_RECOVERIES = 1;

process.env.PW_RUN_ID = runId;
process.env.PW_SERVER_PORT = String(serverPort);

function writeHeartbeat(payload: Record<string, unknown>): void {
  fs.mkdirSync(path.dirname(heartbeatPath), { recursive: true });
  const tmpPath = `${heartbeatPath}.tmp`;
  fs.writeFileSync(tmpPath, JSON.stringify({ ts: Date.now(), ...payload }) + '\n');
  fs.renameSync(tmpPath, heartbeatPath);
}

function beat(stage: string, extra: Record<string, unknown> = {}): void {
  writeHeartbeat({ event: 'global_setup', stage, ...extra });
}

function appendSetupDiagnostic(event: string, payload: Record<string, unknown> = {}): void {
  fs.mkdirSync(debugDir, { recursive: true });
  fs.appendFileSync(
    setupLogPath,
    JSON.stringify({
      ts: new Date().toISOString(),
      runId,
      serverPort,
      event,
      ...payload,
    }) + '\n'
  );
  setupSummary.eventsWritten += 1;
}

function persistSetupSummary(): void {
  fs.mkdirSync(debugDir, { recursive: true });
  fs.writeFileSync(setupSummaryPath, JSON.stringify(setupSummary, null, 2) + '\n');
}

function recordLoginAttempt(attemptDiagnostic: LoginAttemptDiagnostic): void {
  setupSummary.loginAttempts.push(attemptDiagnostic);
  appendSetupDiagnostic('login_attempt_result', { ...attemptDiagnostic });
}

function parseAuthErrorCode(payload: string): string | null {
  try {
    const parsed = JSON.parse(payload) as {
      code?: unknown;
      error_code?: unknown;
      error?: { code?: unknown } | unknown;
    };
    if (typeof parsed.code === 'string' && parsed.code.length > 0) return parsed.code;
    if (typeof parsed.error_code === 'string' && parsed.error_code.length > 0) return parsed.error_code;
    if (parsed.error && typeof parsed.error === 'object' && parsed.error !== null) {
      const errorRecord = parsed.error as { code?: unknown };
      if (typeof errorRecord.code === 'string' && errorRecord.code.length > 0) {
        return errorRecord.code;
      }
    }
  } catch {
    // Best-effort JSON parse only.
  }
  return null;
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

function isUserNotFoundError(status: number, payload: string): boolean {
  if (status === 401 && payload.includes('USER_NOT_FOUND')) return true;
  try {
    const parsed = JSON.parse(payload) as {
      code?: unknown;
      error_code?: unknown;
      error?: { code?: unknown } | unknown;
    };
    if (parsed.code === 'USER_NOT_FOUND') return true;
    if (parsed.error_code === 'USER_NOT_FOUND') return true;
    if (parsed.error && typeof parsed.error === 'object' && parsed.error !== null) {
      const errorRecord = parsed.error as { code?: unknown };
      if (errorRecord.code === 'USER_NOT_FOUND') return true;
    }
  } catch {
    // Fall back to string matching only.
  }
  return false;
}

async function recoverFromUserNotFound(): Promise<void> {
  setupSummary.recoveryAttempts += 1;
  appendSetupDiagnostic('login_recovery_start', {
    attempt: setupSummary.recoveryAttempts,
    recoveryPathUsed: USER_NOT_FOUND_RECOVERY_PATH,
    maxRecoveries: MAX_USER_NOT_FOUND_RECOVERIES,
  });
  beat('login:recover_user_not_found:start');
  const api = await request.newContext({ baseURL: backendBaseUrl });
  const post = async (apiPath: string) => {
    const resp = await api.post(apiPath, { data: {}, timeout: 30_000 });
    if (!resp.ok()) {
      const text = await resp.text();
      throw new Error(`Recovery failed ${apiPath}: ${resp.status()} ${text}`);
    }
  };

  try {
    beat('login:recover_user_not_found:reset');
    await post('/testkit/reset');
    beat('login:recover_user_not_found:seed_minimal');
    await post('/testkit/seed_minimal');
    beat('login:recover_user_not_found:done');
    appendSetupDiagnostic('login_recovery_done', {
      attempt: setupSummary.recoveryAttempts,
      recoveryPathUsed: USER_NOT_FOUND_RECOVERY_PATH,
    });
  } finally {
    await api.dispose();
  }
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
    appendSetupDiagnostic('login_skipped_dev_bypass');
    return;
  }

  beat('login:start', { uiBaseUrl, runId, serverPort });
  appendSetupDiagnostic('login_start', { uiBaseUrl, runId, serverPort });
  fs.mkdirSync(path.dirname(storageStatePath), { recursive: true });
  fs.mkdirSync(debugDir, { recursive: true });
  await waitForOk(`${uiBaseUrl}/style-audit`, 120_000);
  beat('login:style_audit_ok');
  appendSetupDiagnostic('login_style_audit_ok');
  let recoveredUserNotFound = false;
  let userNotFoundRecoveriesUsed = 0;
  let cookieValidated = false;
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    const attemptStartedAt = Date.now();
    const attemptDiag: LoginAttemptDiagnostic = {
      attempt,
      loginStatus: 0,
      userNotFound: false,
      recoveryPathUsed: 'none',
      recoveredFromUserNotFound: recoveredUserNotFound,
      result: 'failed',
      elapsedMs: 0,
    };
    const api = await request.newContext({ baseURL: backendBaseUrl });
    try {
      beat('login:api_attempt', { attempt });
      appendSetupDiagnostic('login_attempt_start', { attempt });
      const resp = await api.post('/v1/auth/login', {
        data: { username: 'test@example.com', password: 'password' },
        timeout: 30_000,
      });
      attemptDiag.loginStatus = resp.status();
      attemptDiag.loginElapsedMs = Date.now() - attemptStartedAt;
      appendSetupDiagnostic('login_attempt_login_response', {
        attempt,
        status: resp.status(),
      });
      if (!resp.ok()) {
        const text = await resp.text();
        attemptDiag.meErrorCode = parseAuthErrorCode(text);
        attemptDiag.recoveryPathUsed = recoveredUserNotFound
          ? USER_NOT_FOUND_RECOVERY_PATH
          : 'none';
        attemptDiag.result = 'failed';
        attemptDiag.elapsedMs = Date.now() - attemptStartedAt;
        recordLoginAttempt(attemptDiag);
        appendSetupDiagnostic('login_attempt_failed', {
          ...attemptDiag,
        });
        throw new Error(`Login failed: ${resp.status()} ${text}`);
      }
      beat('login:api_ok', { attempt });
      await api.storageState({ path: storageStatePath });
    } finally {
      await api.dispose();
    }

    const uiApi = await request.newContext({
      baseURL: uiBaseUrl,
      storageState: storageStatePath,
    });
    try {
      const meCheckStartedAt = Date.now();
      const meResp = await uiApi.get('/v1/auth/me', { timeout: 30_000 });
      attemptDiag.authMeElapsedMs = Date.now() - meCheckStartedAt;
      attemptDiag.meStatus = meResp.status();
      if (meResp.ok()) {
        beat('login:cookie_ok', { attempt, recoveredUserNotFound });
        attemptDiag.result = 'ok';
        attemptDiag.userNotFound = false;
        attemptDiag.meErrorCode = null;
        attemptDiag.recoveryPathUsed = recoveredUserNotFound
          ? USER_NOT_FOUND_RECOVERY_PATH
          : 'none';
        attemptDiag.recoveredFromUserNotFound = recoveredUserNotFound;
        attemptDiag.elapsedMs = Date.now() - attemptStartedAt;
        recordLoginAttempt(attemptDiag);
        appendSetupDiagnostic('login_attempt_ok', {
          ...attemptDiag,
        });
        cookieValidated = true;
        break;
      }

      const text = await meResp.text();
      const userNotFound = isUserNotFoundError(meResp.status(), text);
      const errorCode = parseAuthErrorCode(text);
      attemptDiag.userNotFound = userNotFound;
      attemptDiag.meErrorCode = errorCode;
      attemptDiag.recoveryPathUsed = recoveredUserNotFound ? USER_NOT_FOUND_RECOVERY_PATH : 'none';
      attemptDiag.recoveredFromUserNotFound = recoveredUserNotFound;
      beat('login:cookie_rejected', {
        attempt,
        status: meResp.status(),
        userNotFound,
      });
      appendSetupDiagnostic('login_attempt_cookie_rejected', {
        attempt,
        meStatus: meResp.status(),
        userNotFound,
        errorCode,
      });
      if (
        attempt === 1 &&
        userNotFound &&
        userNotFoundRecoveriesUsed < MAX_USER_NOT_FOUND_RECOVERIES
      ) {
        recoveredUserNotFound = true;
        userNotFoundRecoveriesUsed += 1;
        setupSummary.recoveredUserNotFound = true;
        attemptDiag.result = 'retry';
        attemptDiag.recoveryPathUsed = USER_NOT_FOUND_RECOVERY_PATH;
        attemptDiag.elapsedMs = Date.now() - attemptStartedAt;
        recordLoginAttempt(attemptDiag);
        appendSetupDiagnostic('login_attempt_retry_user_not_found', {
          ...attemptDiag,
          recoveriesUsed: userNotFoundRecoveriesUsed,
          maxRecoveries: MAX_USER_NOT_FOUND_RECOVERIES,
        });
        await recoverFromUserNotFound();
        beat('login:retry_after_user_not_found');
        continue;
      }
      attemptDiag.result = 'failed';
      attemptDiag.recoveryPathUsed = recoveredUserNotFound ? USER_NOT_FOUND_RECOVERY_PATH : 'none';
      attemptDiag.elapsedMs = Date.now() - attemptStartedAt;
      recordLoginAttempt(attemptDiag);
      appendSetupDiagnostic('login_attempt_failed', {
        ...attemptDiag,
      });
      throw new Error(`Auth cookie rejected by UI proxy: ${meResp.status()} ${text}`);
    } finally {
      await uiApi.dispose();
    }
  }
  if (!cookieValidated) {
    throw new Error('Auth cookie rejected by UI proxy after bounded recovery');
  }

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
  try {
    beat('global_setup:start');
    appendSetupDiagnostic('global_setup_start');
    await seedBackend();
    await loginAndStoreState();
    setupSummary.success = true;
    beat('global_setup:done');
    appendSetupDiagnostic('global_setup_done');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    setupSummary.success = false;
    setupSummary.failureMessage = message;
    beat('global_setup:failed', { error: message });
    appendSetupDiagnostic('global_setup_failed', { error: message });
    throw err;
  } finally {
    setupSummary.completedAt = new Date().toISOString();
    persistSetupSummary();
  }
}
