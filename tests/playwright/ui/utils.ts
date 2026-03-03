import { test, expect, Page } from '@playwright/test';

export const seeded = {
  adapterId: 'adapter-test',
  adapterName: 'Test Adapter',
  repoId: 'repo-e2e',
  adapterVersionId: 'adapter-version-e2e',
  trainingJobId: 'job-stub',
  traceId: 'trace-fixture',
  runId: 'trace-fixture',
  documentId: 'doc-fixture',
  documentChunkId: 'chunk-fixture',
  evidenceId: 'evidence-fixture',
  stackId: 'stack-test',
  collectionId: 'collection-test',
  datasetId: 'dataset-test',
  workerId: 'worker-test',
};

export async function disableAnimations(page: Page): Promise<void> {
  await page.addStyleTag({
    content: `
      *, *::before, *::after {
        animation-duration: 0s !important;
        animation-delay: 0s !important;
        transition-duration: 0s !important;
        scroll-behavior: auto !important;
      }
    `,
  });
}

type BootStageSnapshot = {
  id: string;
  status: string;
  detail: string;
};

type BootSnapshot = {
  exists: boolean;
  hidden: boolean;
  className: string;
  display: string;
  visibility: string;
  opacity: string;
  rect: { width: number; height: number };
  stages: BootStageSnapshot[];
  path: string;
  title: string;
  signingInVisible: boolean;
  hasShell: boolean;
};

const BOOT_SNAPSHOT_EVAL_TIMEOUT = Symbol('boot-snapshot-eval-timeout');

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function readBootSnapshot(page: Page): Promise<BootSnapshot | null> {
  return await page
    .evaluate(() => {
      const progress = document.getElementById('aos-boot-progress');
      const path = window.location.pathname;
      const title = document.title ?? '';
      const bodyText = document.body?.innerText ?? '';
      const hasShell =
        Boolean(document.querySelector('a.skip-to-main')) ||
        Boolean(document.querySelector('main#main-content')) ||
        Boolean(document.querySelector('nav[aria-label="Main navigation"]'));

      if (!progress) {
        return {
          exists: false,
          hidden: true,
          className: '',
          display: '',
          visibility: '',
          opacity: '',
          rect: { width: 0, height: 0 },
          stages: [],
          path,
          title,
          signingInVisible: bodyText.includes('Signing you in'),
          hasShell,
        };
      }

      const style = window.getComputedStyle(progress);
      const rect = progress.getBoundingClientRect();
      const hidden =
        progress.classList.contains('hidden') ||
        style.display === 'none' ||
        style.visibility === 'hidden' ||
        style.opacity === '0' ||
        rect.width === 0 ||
        rect.height === 0;
      const stages = Array.from(progress.querySelectorAll('.stage')).map((node) => {
        const detailNode = node.querySelector('.detail');
        return {
          id: node.id || '',
          status: node.getAttribute('data-status') ?? '',
          detail: (detailNode?.textContent ?? '').trim(),
        };
      });

      return {
        exists: true,
        hidden,
        className: progress.className,
        display: style.display,
        visibility: style.visibility,
        opacity: style.opacity,
        rect: { width: rect.width, height: rect.height },
        stages,
        path,
        title,
        signingInVisible: bodyText.includes('Signing you in'),
        hasShell,
      };
    })
    .catch(() => null);
}

async function readBootSnapshotWithTimeout(
  page: Page,
  timeoutMs: number
): Promise<BootSnapshot | null | typeof BOOT_SNAPSHOT_EVAL_TIMEOUT> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      readBootSnapshot(page),
      new Promise<typeof BOOT_SNAPSHOT_EVAL_TIMEOUT>((resolve) => {
        timer = setTimeout(() => resolve(BOOT_SNAPSHOT_EVAL_TIMEOUT), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

function formatBootSnapshot(snapshot: BootSnapshot | null): string {
  if (!snapshot) {
    return 'snapshot=unavailable';
  }
  const stageSummary = snapshot.stages
    .slice(0, 7)
    .map((stage) => `${stage.id || 'stage'}:${stage.status || 'unknown'}:${stage.detail || '-'}`)
    .join('|');
  return `path=${snapshot.path}, hidden=${snapshot.hidden}, shell=${snapshot.hasShell}, signing_in=${snapshot.signingInVisible}, title="${snapshot.title}", stages=${stageSummary || 'none'}`;
}

async function waitForBoot(page: Page, timeoutMs = 90_000): Promise<void> {
  if (page.isClosed()) {
    return;
  }

  const startedAt = Date.now();
  const deadline = startedAt + timeoutMs;
  const pollIntervalMs = 250;
  const evaluateTimeoutMs = Math.max(1_000, Math.min(8_000, Math.floor(timeoutMs / 4)));
  const pageErrors: string[] = [];
  const onPageError = (error: Error) => {
    if (pageErrors.length < 4) {
      pageErrors.push(error.message);
    }
  };
  page.on('pageerror', onPageError);

  let lastSnapshot: BootSnapshot | null = null;
  let evaluateTimedOut = false;
  let evaluateTimeoutCount = 0;
  try {
    while (Date.now() < deadline) {
      if (page.isClosed()) {
        return;
      }
      const remainingMs = Math.max(1, deadline - Date.now());
      const snapshotOrTimeout = await readBootSnapshotWithTimeout(
        page,
        Math.min(evaluateTimeoutMs, remainingMs)
      );
      if (snapshotOrTimeout === BOOT_SNAPSHOT_EVAL_TIMEOUT) {
        evaluateTimedOut = true;
        evaluateTimeoutCount += 1;
        const sleepMs = Math.min(pollIntervalMs, Math.max(25, deadline - Date.now()));
        await sleep(sleepMs);
        continue;
      }

      if (!snapshotOrTimeout) {
        const sleepMs = Math.min(pollIntervalMs, Math.max(25, deadline - Date.now()));
        await sleep(sleepMs);
        continue;
      }

      lastSnapshot = snapshotOrTimeout;
      if (snapshotOrTimeout.hidden) {
        return;
      }

      const sleepMs = Math.min(pollIntervalMs, Math.max(25, deadline - Date.now()));
      await sleep(sleepMs);
    }
  } finally {
    page.off('pageerror', onPageError);
  }

  if (page.isClosed()) {
    return;
  }

  const elapsedMs = Date.now() - startedAt;
  const timeoutType = evaluateTimedOut ? 'evaluate-timeout' : 'wait-timeout';
  const diag = {
    timeoutType,
    timeoutMs,
    elapsedMs,
    evaluateTimeoutCount,
    pageErrors,
    snapshot: lastSnapshot,
  };
  console.error('[playwright] waitForBoot timeout diagnostics:', diag);
  throw new Error(
    `[playwright] waitForBoot ${timeoutType} after ${elapsedMs}ms; eval_timeouts=${evaluateTimeoutCount}; ${formatBootSnapshot(lastSnapshot)}; page_errors=${pageErrors.join(' || ') || 'none'}`
  );
}

export async function waitForAppReady(
  page: Page,
  options: { timeoutMs?: number } = {}
): Promise<void> {
  await waitForBoot(page, options.timeoutMs ?? 90_000);
}

const AUTH_TEST_USERNAME = 'test@example.com';
const AUTH_TEST_PASSWORD = 'password';
const DEFAULT_POST_AUTH_PATH = '/dashboard';
const DEFAULT_EXPECTED_POST_AUTH_PATH = /\/(dashboard|chat)(\/|$)/;

type AuthSurfaceState = {
  url: string;
  onLogin: boolean;
  onAuthError: boolean;
  onAuthTimeout: boolean;
  signingInVisible: boolean;
  shellVisible: boolean;
};

async function safeIsVisible(
  probe: () => Promise<boolean>,
  timeoutMs = 1500
): Promise<boolean> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      probe().catch(() => false),
      new Promise<boolean>((resolve) => {
        timer = setTimeout(() => resolve(false), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export type AuthBootstrapMode = 'none' | 'ui-only' | 'ui-then-api';

export type AuthBootstrapOptions = {
  mode?: AuthBootstrapMode;
  maxUiAttempts?: number;
  requireUiAttempt?: boolean;
  postAuthPath?: string;
  expectedPostAuthPath?: RegExp;
};

async function readAuthSurfaceState(page: Page): Promise<AuthSurfaceState> {
  if (page.isClosed()) {
    return {
      url: 'about:blank',
      onLogin: false,
      onAuthError: false,
      onAuthTimeout: false,
      signingInVisible: false,
      shellVisible: false,
    };
  }
  const authError = page.getByRole('heading', { name: 'Authentication Error' });
  const authTimeout = page.getByRole('heading', { name: 'Authentication Timeout' });
  const loginHeading = page.getByRole('heading', { name: 'Login', exact: true });
  const signingIn = page.getByText('Signing you in', { exact: false });
  const shellLink = page.getByRole('link', { name: /^(Home|Dashboard)$/ }).first();
  const shellNav = page.getByRole('navigation', { name: 'Main navigation' });
  const accountButton = page
    .getByRole('button', { name: new RegExp(AUTH_TEST_USERNAME, 'i') })
    .first();
  const shellVisible =
    (await safeIsVisible(() => shellLink.isVisible())) ||
    (await safeIsVisible(() => shellNav.isVisible())) ||
    (await safeIsVisible(() => accountButton.isVisible()));
  const url = (() => {
    try {
      return page.url();
    } catch {
      return 'about:blank';
    }
  })();
  return {
    url,
    onLogin: await safeIsVisible(() => loginHeading.isVisible()),
    onAuthError: await safeIsVisible(() => authError.isVisible()),
    onAuthTimeout: await safeIsVisible(() => authTimeout.isVisible()),
    signingInVisible: await safeIsVisible(() => signingIn.isVisible()),
    shellVisible,
  };
}

function formatAuthSurfaceState(state: AuthSurfaceState): string {
  return `url=${state.url}, login=${state.onLogin}, auth_error=${state.onAuthError}, auth_timeout=${state.onAuthTimeout}, signing_in=${state.signingInVisible}, shell=${state.shellVisible}`;
}

async function settleAfterAuthAction(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded', { timeout: 4_000 }).catch(() => {});
  const signingIn = page.getByText('Signing you in', { exact: false });
  if (await safeIsVisible(() => signingIn.isVisible())) {
    await signingIn.waitFor({ state: 'hidden', timeout: 4_000 }).catch(() => {});
  }
  await waitForAppReady(page, { timeoutMs: 20_000 });
}

async function attemptUiLogin(
  page: Page,
  maxUiAttempts: number
): Promise<{ resolved: boolean; uiAttempted: boolean; finalState: AuthSurfaceState }> {
  const attempts = Math.max(1, maxUiAttempts);
  let uiAttempted = false;
  const authSurfacePath = /^\/(?:login|auth(?:entication)?-(?:error|timeout))(?:\/|$)/i;
  const readPathname = (url: string): string => {
    try {
      return new URL(url).pathname;
    } catch {
      return '';
    }
  };
  const isResolvedState = (state: AuthSurfaceState, statePathname: string): boolean =>
    !state.onLogin &&
    !state.onAuthError &&
    !state.onAuthTimeout &&
    !state.signingInVisible &&
    (state.shellVisible || (statePathname.length > 0 && !authSurfacePath.test(statePathname)));
  const recoverFromAuthSurfaceTransition = async (): Promise<{
    resolved: boolean;
    finalState: AuthSurfaceState;
  }> => {
    const finalState = await readAuthSurfaceState(page);
    const finalPathname = readPathname(finalState.url);
    return {
      resolved: isResolvedState(finalState, finalPathname),
      finalState,
    };
  };

  for (let attempt = 0; attempt < attempts; attempt += 1) {
    let state = await readAuthSurfaceState(page);
    let statePathname = readPathname(state.url);

    // Already authenticated and outside auth error surfaces.
    if (isResolvedState(state, statePathname)) {
      return { resolved: true, uiAttempted, finalState: state };
    }

    if (state.onAuthError || state.onAuthTimeout) {
      const goToLogin = page.getByRole('button', { name: /Go to Login/i });
      if (await goToLogin.isVisible().catch(() => false)) {
        await goToLogin.click();
      } else {
        await gotoWithRetry(page, '/login');
      }
      await settleAfterAuthAction(page);
      state = await readAuthSurfaceState(page);
      statePathname = readPathname(state.url);
    }

    if (!state.onLogin && !state.onAuthError && !state.onAuthTimeout) {
      // Unknown transitional state; force a deterministic entrypoint.
      if (
        !state.signingInVisible &&
        statePathname.length > 0 &&
        !authSurfacePath.test(statePathname)
      ) {
        return { resolved: true, uiAttempted, finalState: state };
      }
      await gotoWithRetry(page, '/login');
      await settleAfterAuthAction(page);
      state = await readAuthSurfaceState(page);
      statePathname = readPathname(state.url);
      if (!state.onLogin && !state.onAuthError && !state.onAuthTimeout && state.shellVisible) {
        return { resolved: true, uiAttempted, finalState: state };
      }
    }

    if (state.onLogin) {
      uiAttempted = true;
      const usernameInput = page.getByLabel('Username');
      const passwordInput = page.getByLabel('Password');
      const loginButton = page.getByRole('button', { name: 'Log in' });
      if (
        !(await usernameInput.isVisible().catch(() => false)) ||
        !(await passwordInput.isVisible().catch(() => false))
      ) {
        const recovery = await recoverFromAuthSurfaceTransition();
        if (recovery.resolved) {
          return { resolved: true, uiAttempted, finalState: recovery.finalState };
        }
        continue;
      }

      const usernameFilled = await usernameInput
        .fill(AUTH_TEST_USERNAME, { timeout: 5_000 })
        .then(() => true)
        .catch(() => false);
      if (!usernameFilled) {
        const recovery = await recoverFromAuthSurfaceTransition();
        if (recovery.resolved) {
          return { resolved: true, uiAttempted, finalState: recovery.finalState };
        }
        continue;
      }

      const passwordFilled = await passwordInput
        .fill(AUTH_TEST_PASSWORD, { timeout: 5_000 })
        .then(() => true)
        .catch(() => false);
      if (!passwordFilled) {
        const recovery = await recoverFromAuthSurfaceTransition();
        if (recovery.resolved) {
          return { resolved: true, uiAttempted, finalState: recovery.finalState };
        }
        continue;
      }

      const enabled = await loginButton.isEnabled({ timeout: 2_000 }).catch(async () => {
        const recovery = await recoverFromAuthSurfaceTransition();
        if (recovery.resolved) {
          state = recovery.finalState;
        }
        return false;
      });
      if (isResolvedState(state, readPathname(state.url))) {
        return { resolved: true, uiAttempted, finalState: state };
      }
      if (enabled) {
        const clicked = await loginButton
          .click({ timeout: 2_000 })
          .then(() => true)
          .catch(() => false);
        if (!clicked) {
          const recovery = await recoverFromAuthSurfaceTransition();
          if (recovery.resolved) {
            return { resolved: true, uiAttempted, finalState: recovery.finalState };
          }
        }
      }
      await settleAfterAuthAction(page);

      state = await readAuthSurfaceState(page);
      statePathname = readPathname(state.url);
      if (isResolvedState(state, statePathname)) {
        return { resolved: true, uiAttempted, finalState: state };
      }

      const serviceUnavailableVisible = await page
        .getByText(/service unavailable/i)
        .isVisible()
        .catch(() => false);
      if (serviceUnavailableVisible && attempt < attempts - 1) {
        await sleep(500);
      }
    }
  }

  return {
    resolved: false,
    uiAttempted,
    finalState: await readAuthSurfaceState(page),
  };
}

async function apiLoginFallback(
  page: Page,
  postAuthPath: string,
  expectedPostAuthPath: RegExp
): Promise<void> {
  let loginResp = await page.request
    .post('/v1/auth/login', {
      data: { username: AUTH_TEST_USERNAME, password: AUTH_TEST_PASSWORD },
      timeout: 8_000,
    })
    .catch(() => null);
  if (loginResp && !loginResp.ok() && loginResp.status() === 503) {
    await sleep(500);
    loginResp = await page.request
      .post('/v1/auth/login', {
        data: { username: AUTH_TEST_USERNAME, password: AUTH_TEST_PASSWORD },
        timeout: 8_000,
      })
      .catch(() => null);
  }
  if (!loginResp || !loginResp.ok()) {
    throw new Error(
      `bootstrapAuth API fallback failed for /v1/auth/login: ${loginResp?.status?.() ?? 'request-failed'}`
    );
  }

  await gotoWithRetry(page, postAuthPath);

  const meResp = await page.request.get('/v1/auth/me', { timeout: 8_000 }).catch(() => null);
  if (!meResp || !meResp.ok()) {
    throw new Error(
      `bootstrapAuth API fallback failed for /v1/auth/me: ${meResp?.status?.() ?? 'request-failed'}`
    );
  }

  const path = new URL(page.url()).pathname;
  if (!expectedPostAuthPath.test(path)) {
    await gotoWithRetry(page, postAuthPath);
  }
}

export async function bootstrapAuth(
  page: Page,
  options: AuthBootstrapOptions = {}
): Promise<void> {
  const mode = options.mode ?? 'ui-only';
  if (mode === 'none') {
    return;
  }

  const maxUiAttempts = options.maxUiAttempts ?? 2;
  const requireUiAttempt = options.requireUiAttempt ?? false;
  const postAuthPath = options.postAuthPath ?? DEFAULT_POST_AUTH_PATH;
  const expectedPostAuthPath = options.expectedPostAuthPath ?? DEFAULT_EXPECTED_POST_AUTH_PATH;
  const devBypass = (process.env.PW_DEV_BYPASS ?? '').trim() === '1';

  // In dev-bypass mode the backend should serve protected routes without a login redirect,
  // but the UI can still briefly show a "Signing you in" overlay while it boots/auth-hydrates.
  if (devBypass) {
    const signingIn = page.getByText('Signing you in', { exact: false });
    if (await signingIn.isVisible().catch(() => false)) {
      await signingIn.waitFor({ state: 'hidden', timeout: 90_000 }).catch(() => {});
    }

    // If the Shell is visible, we consider auth "good enough" for E2E flows.
    const shellLink = page.getByRole('link', { name: /^(Home|Dashboard)$/ });
    if (await shellLink.isVisible().catch(() => false)) {
      return;
    }
  }

  const uiResult = await attemptUiLogin(page, maxUiAttempts);
  if (uiResult.resolved) {
    return;
  }

  if (mode === 'ui-only') {
    throw new Error(
      `bootstrapAuth exhausted UI attempts (mode=${mode}, maxUiAttempts=${maxUiAttempts}). ${formatAuthSurfaceState(uiResult.finalState)}`
    );
  }

  if (requireUiAttempt && !uiResult.uiAttempted) {
    throw new Error(
      `bootstrapAuth refused API fallback because no UI login attempt occurred (mode=${mode}, maxUiAttempts=${maxUiAttempts}). ${formatAuthSurfaceState(uiResult.finalState)}`
    );
  }

  await apiLoginFallback(page, postAuthPath, expectedPostAuthPath);
  await settleAfterAuthAction(page).catch(() => {});
  let finalState = await readAuthSurfaceState(page);
  if (finalState.signingInVisible && !finalState.shellVisible) {
    await gotoWithRetry(page, postAuthPath);
    await settleAfterAuthAction(page).catch(() => {});
    finalState = await readAuthSurfaceState(page);
  }
  if (
    finalState.onLogin ||
    finalState.onAuthError ||
    finalState.onAuthTimeout ||
    finalState.signingInVisible
  ) {
    throw new Error(
      `bootstrapAuth API fallback completed but auth surface remained unresolved (mode=${mode}). ${formatAuthSurfaceState(finalState)}`
    );
  }
}

export async function ensureLoggedIn(page: Page): Promise<void> {
  await bootstrapAuth(page, { mode: 'ui-only' });
}

function matchesRequestedPath(currentUrl: string, requestedPath: string): boolean {
  const currentPath = new URL(currentUrl).pathname;
  const requested = requestedPath.split(/[?#]/)[0] || '/';
  if (requested === '/') {
    return currentPath === '/';
  }
  return currentPath === requested || currentPath.startsWith(`${requested}/`);
}

const SOFT_ROUTE_REDIRECTS = new Map<string, string>([['/user', '/settings']]);
const SOFT_SETTINGS_ROUTE_SET = new Set(['/settings', '/user']);

export function canonicalSoftRoutePath(routePath: string): string {
  return SOFT_ROUTE_REDIRECTS.get(routePath) ?? routePath;
}

function acceptedSoftRoutePaths(routePath: string): string[] {
  const canonical = canonicalSoftRoutePath(routePath);
  return canonical === routePath ? [routePath] : [routePath, canonical];
}

export function pathMatchesSoftRoute(currentUrl: string, routePath: string): boolean {
  try {
    const currentPath = new URL(currentUrl).pathname;
    return acceptedSoftRoutePaths(routePath).some(
      (acceptedPath) => currentPath === acceptedPath || currentPath.startsWith(`${acceptedPath}/`)
    );
  } catch {
    return false;
  }
}

async function waitForSoftRoutePath(page: Page, routePath: string, timeoutMs: number): Promise<boolean> {
  return await expect
    .poll(() => pathMatchesSoftRoute(page.url(), routePath), { timeout: timeoutMs })
    .toBeTruthy()
    .then(() => true)
    .catch(() => false);
}

async function navigateSettingsViaShortcut(page: Page): Promise<boolean> {
  const shortcuts = ['Meta+,', 'Control+,'];
  for (const shortcut of shortcuts) {
    await page.keyboard.press(shortcut).catch(() => {});
    if (await waitForSoftRoutePath(page, '/settings', 4_000)) {
      return true;
    }
  }
  return false;
}

async function switchToFullProfileIfAvailable(page: Page): Promise<void> {
  const switchToFull = page.locator('button[title*="switch to Full"]').first();
  if (await safeIsVisible(() => switchToFull.isVisible())) {
    await switchToFull.click({ timeout: 5_000 }).catch(() => {});
    await page
      .locator('button[title*="switch to Primary"]')
      .first()
      .waitFor({ state: 'visible', timeout: 10_000 })
      .catch(() => {});
  }
}

async function openCommandDeck(page: Page): Promise<ReturnType<Page['locator']> | null> {
  const input = page
    .locator('.command-palette-panel input[aria-label*="Search pages"]')
    .first();
  if (await safeIsVisible(() => input.isVisible())) {
    return input;
  }

  const openShortcuts = ['Control+k', 'Meta+k'];
  for (const shortcut of openShortcuts) {
    await page.keyboard.press(shortcut).catch(() => {});
    if (await safeIsVisible(() => input.isVisible())) {
      return input;
    }
  }
  return null;
}

async function navigateSettingsViaCommandDeck(page: Page): Promise<boolean> {
  const input = await openCommandDeck(page);
  if (!input) {
    return false;
  }

  await input.fill('settings').catch(() => {});
  const settingsResult = page
    .locator('.command-palette-panel .search-result-title')
    .filter({ hasText: /^Settings$/ })
    .first();

  let resultVisible = false;
  const resultDeadline = Date.now() + 12_000;
  while (Date.now() < resultDeadline) {
    if (await safeIsVisible(() => settingsResult.isVisible())) {
      resultVisible = true;
      break;
    }
    await sleep(200);
  }

  if (!resultVisible) {
    return false;
  }

  const clicked = await settingsResult
    .click({ timeout: 5_000, force: true })
    .then(() => true)
    .catch(() => false);
  if (!clicked) {
    return false;
  }

  return await waitForSoftRoutePath(page, '/settings', 8_000);
}

async function navigateSettingsViaHistoryApi(page: Page): Promise<boolean> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const switched = await Promise.race([
    page
      .evaluate(() => {
        window.history.pushState({}, '', '/settings');
        window.dispatchEvent(new PopStateEvent('popstate'));
        return true;
      })
      .then(() => true)
      .catch(() => false),
    new Promise<boolean>((resolve) => {
      timer = setTimeout(() => resolve(false), 1_500);
    }),
  ]).finally(() => {
    if (timer) clearTimeout(timer);
  });

  if (!switched) {
    return false;
  }
  return await waitForSoftRoutePath(page, '/settings', 6_000);
}

export async function gotoAndBootstrapSoftRoute(page: Page, routePath: string): Promise<void> {
  if (!SOFT_SETTINGS_ROUTE_SET.has(routePath)) {
    throw new Error(`Unsupported soft route "${routePath}"`);
  }

  await gotoAndBootstrap(page, '/dashboard', {
    mode: 'ui-then-api',
    requireUiAttempt: false,
    postAuthPath: '/dashboard',
    expectedPostAuthPath: /\/(dashboard|chat)(\/|$)/,
  });

  if (pathMatchesSoftRoute(page.url(), routePath)) {
    return;
  }

  await switchToFullProfileIfAvailable(page);

  const navigatedByShortcut = await navigateSettingsViaShortcut(page);
  const navigatedByCommandDeck = navigatedByShortcut ? true : await navigateSettingsViaCommandDeck(page);
  if (!navigatedByCommandDeck && !navigatedByShortcut) {
    await navigateSettingsViaHistoryApi(page);
  }

  await expect
    .poll(() => pathMatchesSoftRoute(page.url(), routePath), { timeout: 25_000 })
    .toBeTruthy()
    .catch(() => {
      throw new Error(`Unable to soft-navigate to ${routePath}; current_url=${page.url()}`);
    });
}

export async function gotoAndBootstrap(
  page: Page,
  path: string,
  options: AuthBootstrapOptions = {}
): Promise<void> {
  const mode = options.mode ?? 'ui-only';
  await gotoWithRetry(page, path);
  if (mode === 'none') {
    await waitForAppReady(page);
    return;
  }

  // Some protected routes can remain on transitional boot/auth overlays until a login
  // attempt runs. Don't let pre-auth readiness block the auth bootstrap.
  await waitForAppReady(page).catch(() => {});
  await bootstrapAuth(page, options);
  await waitForAppReady(page);

  if (!matchesRequestedPath(page.url(), path)) {
    await gotoWithRetry(page, path);
    await waitForAppReady(page).catch(() => {});
    await bootstrapAuth(page, options);
    await waitForAppReady(page);
  }
}

export async function gotoAndExpectHeading(
  page: Page,
  path: string,
  heading: string
): Promise<void> {
  await page.goto(path, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: heading, level: 1, exact: true })
  ).toBeVisible();
}

export async function expectEmptyState(
  page: Page,
  text: string
): Promise<void> {
  await expect(page.getByText(text)).toBeVisible();
}

export async function expectErrorState(page: Page): Promise<void> {
  const candidates = [
    page.getByRole('button', { name: 'Retry' }),
    page.getByText('Error', { exact: true }),
    page.getByRole('heading', { name: 'Authentication Error' }),
    page.getByRole('heading', { name: 'Authentication Timeout' }),
    page.getByRole('heading', { name: '404' }),
    page.getByRole('heading', { name: 'Not Found' }),
    page.getByText(/Not Found/i),
    page.locator('#aos-panic-overlay'),
    page.locator('#aos-panic-message'),
    page.locator('.boot-error'),
    page.locator('.border-destructive'),
  ];

  for (const locator of candidates) {
    if (await locator.isVisible().catch(() => false)) {
      await expect(locator).toBeVisible();
      return;
    }
  }

  await expect(page.getByText('Error', { exact: true })).toBeVisible();
}

export type ChatEntryState = 'unavailable' | 'empty' | 'active';

export type ChatEntryAnchor =
  | 'chat-header'
  | 'chat-unavailable-state'
  | 'chat-empty-new-chat'
  | 'chat-sidebar-new-session';

export type ChatEntryContract = {
  state: ChatEntryState;
  anchor: ChatEntryAnchor;
  path: string;
  unavailableReason?: string;
};

function currentPath(page: Page): string {
  try {
    return new URL(page.url()).pathname;
  } catch {
    return page.url();
  }
}

async function readUnavailableReason(page: Page): Promise<string | undefined> {
  const reasonLocator = page.getByTestId('chat-unavailable-reason').first();
  if (await reasonLocator.isVisible().catch(() => false)) {
    const text = (await reasonLocator.textContent())?.trim();
    if (text) return text;
  }
  const heading = page
    .getByRole('heading', { name: /^(Chat|Conversation) unavailable$/i })
    .first();
  if (await heading.isVisible().catch(() => false)) {
    const text = (await heading.textContent())?.trim();
    if (text) return text;
  }
  return undefined;
}

export async function resolveChatEntryContract(
  page: Page,
  options: { timeoutMs?: number } = {}
): Promise<ChatEntryContract> {
  const timeoutMs = options.timeoutMs ?? 20_000;
  const startedAt = Date.now();
  let authRecoveryAttempted = false;
  while (Date.now() - startedAt < timeoutMs) {
    const path = currentPath(page);
    const authErrorVisible = await page
      .getByRole('heading', { name: 'Authentication Error' })
      .isVisible()
      .catch(() => false);
    const authTimeoutVisible = await page
      .getByRole('heading', { name: 'Authentication Timeout' })
      .isVisible()
      .catch(() => false);
    const loginVisible = await page
      .getByRole('heading', { name: 'Login', exact: true })
      .isVisible()
      .catch(() => false);
    if (authErrorVisible || authTimeoutVisible || loginVisible) {
      if (authRecoveryAttempted) {
        throw new Error(
          `Unable to resolve /chat entry contract: authentication surface persisted after recovery. url=${page.url()}`
        );
      }
      authRecoveryAttempted = true;
      const goToLogin = page.getByRole('button', { name: /Go to Login/i });
      if (await goToLogin.isVisible().catch(() => false)) {
        await goToLogin.click().catch(() => {});
      }
      await bootstrapAuth(page, {
        mode: 'ui-then-api',
        maxUiAttempts: 2,
        postAuthPath: '/chat',
        expectedPostAuthPath: /\/(chat|dashboard)(\/|$)/,
      });
      await gotoWithRetry(page, '/chat');
      await waitForAppReady(page, { timeoutMs: 20_000 });
      continue;
    }
    // Check active state first — this is the highest-priority anchor.
    if (await page.getByTestId('chat-header').isVisible().catch(() => false)) {
      return { state: 'active', anchor: 'chat-header', path };
    }

    // Check empty states before unavailable. Empty is actionable (click New Chat),
    // while unavailable might be a transient flash before status response arrives.
    if (await page.getByTestId('chat-empty-new-chat').isVisible().catch(() => false)) {
      return { state: 'empty', anchor: 'chat-empty-new-chat', path };
    }
    if (await page.getByTestId('chat-sidebar-new-session').isVisible().catch(() => false)) {
      return { state: 'empty', anchor: 'chat-sidebar-new-session', path };
    }
    if (await page.getByRole('button', { name: 'New Chat', exact: true }).isVisible().catch(() => false)) {
      return { state: 'empty', anchor: 'chat-empty-new-chat', path };
    }
    if (
      await page.getByRole('button', { name: 'New Session', exact: true }).isVisible().catch(() => false)
    ) {
      return { state: 'empty', anchor: 'chat-sidebar-new-session', path };
    }
    if (
      await page
        .getByRole('heading', { name: 'Sessions', level: 2, exact: true })
        .isVisible()
        .catch(() => false)
    ) {
      return { state: 'empty', anchor: 'chat-sidebar-new-session', path };
    }

    // Unavailable checks: the WASM app may flash "unavailable" before the status
    // API response arrives and triggers a re-render. To avoid false negatives when
    // the system status is stubbed as ready, do a short stabilization wait and
    // re-check for the active anchor before committing to `unavailable`.
    const unavailableTestId = await page.getByTestId('chat-unavailable-state').isVisible().catch(() => false);
    const unavailableHeading = !unavailableTestId && await page
      .getByRole('heading', { name: /^(Chat|Conversation) unavailable$/i, level: 2 })
      .isVisible()
      .catch(() => false);
    const unavailableText = !unavailableTestId && !unavailableHeading
      && await page.getByText(/(Chat|Conversation) unavailable/i, { exact: false }).isVisible().catch(() => false);

    if (unavailableTestId || unavailableHeading || unavailableText) {
      // Give the app a moment to process a pending status response that might
      // transition the UI from unavailable → active/empty.
      await sleep(500);

      // Re-check: if the active or empty anchor appeared, prefer that.
      if (await page.getByTestId('chat-header').isVisible().catch(() => false)) {
        return { state: 'active', anchor: 'chat-header', path };
      }
      if (await page.getByTestId('chat-empty-new-chat').isVisible().catch(() => false)) {
        return { state: 'empty', anchor: 'chat-empty-new-chat', path };
      }
      if (await page.getByTestId('chat-sidebar-new-session').isVisible().catch(() => false)) {
        return { state: 'empty', anchor: 'chat-sidebar-new-session', path };
      }

      // Confirmed unavailable — the status response didn't change the state.
      return {
        state: 'unavailable',
        anchor: 'chat-unavailable-state',
        path,
        unavailableReason: await readUnavailableReason(page),
      };
    }

    const onDashboard = /^\/($|dashboard(\/|$))/.test(path);
    if (onDashboard) {
        const chatUnavailableBanner = page.getByText(/(Chat|Conversation) unavailable/i, { exact: false });
      if (await chatUnavailableBanner.isVisible().catch(() => false)) {
        // Same stabilization: wait and re-check before committing to unavailable.
        await sleep(500);
        if (await page.getByTestId('chat-header').isVisible().catch(() => false)) {
          return { state: 'active', anchor: 'chat-header', path };
        }
        const bannerText = (await chatUnavailableBanner.first().textContent().catch(() => null))?.trim();
        return {
          state: 'unavailable',
          anchor: 'chat-unavailable-state',
          path,
          unavailableReason: bannerText || (await readUnavailableReason(page)),
        };
      }
    }

    await sleep(250);
  }

  throw new Error(
    `Unable to resolve /chat entry contract within ${timeoutMs}ms (expected anchors: chat-header, chat-unavailable-state, chat-empty-new-chat, chat-sidebar-new-session). url=${page.url()}`
  );
}

export async function resolveChatEntryState(
  page: Page,
  options: { timeoutMs?: number } = {}
): Promise<ChatEntryState> {
  const contract = await resolveChatEntryContract(page, options);
  return contract.state;
}

export async function gotoChatEntryAndResolve(
  page: Page,
  options: AuthBootstrapOptions & { timeoutMs?: number } = {}
): Promise<ChatEntryContract> {
  const { timeoutMs, ...bootstrapOptions } = options;
  await gotoAndBootstrap(page, '/chat', {
    mode: 'ui-only',
    ...bootstrapOptions,
  });
  return await resolveChatEntryContract(page, { timeoutMs });
}

export async function ensureActiveChatSession(page: Page): Promise<void> {
  const path = currentPath(page);
  if (!/^\/chat(\/|$)/.test(path)) {
    await gotoWithRetry(page, '/chat');
    await waitForAppReady(page);
  }

  const contract = await resolveChatEntryContract(page);
  if (contract.state === 'active') {
    return;
  }
  if (contract.state === 'unavailable') {
    throw new Error(
      'Cannot ensure an active chat session: chat is unavailable (inference not ready).'
    );
  }

  if (contract.anchor === 'chat-empty-new-chat') {
    await page.getByTestId('chat-empty-new-chat').click();
  } else if (contract.anchor === 'chat-sidebar-new-session') {
    await page.getByTestId('chat-sidebar-new-session').click();
  } else {
    const emptyNewChat = page.getByTestId('chat-empty-new-chat');
    if (await emptyNewChat.isVisible().catch(() => false)) {
      await emptyNewChat.click();
    } else {
      const sidebarNewSession = page.getByTestId('chat-sidebar-new-session');
      if (await sidebarNewSession.isVisible().catch(() => false)) {
        await sidebarNewSession.click();
      } else {
        const newChatByRole = page.getByRole('button', { name: 'New Chat', exact: true });
        const newSessionByRole = page.getByRole('button', { name: 'New Session', exact: true });
        if (await newChatByRole.isVisible().catch(() => false)) {
          await newChatByRole.click();
        } else if (await newSessionByRole.isVisible().catch(() => false)) {
          await newSessionByRole.click();
        } else {
          throw new Error(
            `Cannot ensure an active chat session from /chat entry contract: state=${contract.state}, anchor=${contract.anchor}.`
          );
        }
      }
    }
  }
  await expect(page.getByTestId('chat-header')).toBeVisible({ timeout: 20_000 });
}

// Route smoke test helpers (extracted from smoke specs)
export type RouteCheck = {
  path: string;
  testId?: string;
  testIdsAny?: string[];
  heading?: string;
  text?: string | RegExp;
  headingLevel?: number;
};

export async function gotoWithRetry(page: Page, path: string): Promise<void> {
  try {
    await page.goto(path, { waitUntil: 'domcontentloaded' });
  } catch (err) {
    if (String(err).includes('net::ERR_ABORTED')) {
      await page.goto(path, { waitUntil: 'domcontentloaded' });
      return;
    }
    throw err;
  }
}

export async function runRouteCheck(page: Page, route: RouteCheck): Promise<void> {
  await gotoAndBootstrap(page, route.path, {
    mode: route.path === '/login' ? 'none' : 'ui-only',
  });
  if (route.path !== '/login') {
    const currentPath = new URL(page.url()).pathname;
    if (!currentPath.startsWith(route.path)) {
      await gotoWithRetry(page, route.path);
      await waitForAppReady(page);
    }
  }

  let testIdError: unknown;
  const testIds = route.testIdsAny?.length
    ? route.testIdsAny
    : route.testId
      ? [route.testId]
      : [];

  if (testIds.length > 0) {
    for (const testId of testIds) {
      try {
        await expect(page.getByTestId(testId).first()).toBeVisible({
          timeout: 20_000,
        });
        return;
      } catch (err) {
        testIdError = err;
      }
    }
  }

  if (route.heading) {
    const heading = page.getByRole('heading', {
      name: route.heading,
      level: route.headingLevel ?? 1,
      exact: true,
    });
    if (route.path === '/login') {
      const loginVisible = await heading.isVisible().catch(() => false);
      if (loginVisible) {
        await expect(heading).toBeVisible({ timeout: 20_000 });
      } else {
        await expect(
          // /login redirects to /chat when already authenticated; assert on the Shell sidebar.
          page.getByRole('link', { name: /^(Home|Dashboard)$/ })
        ).toBeVisible({ timeout: 20_000 });
      }
    } else {
      await expect(heading).toBeVisible({ timeout: 20_000 });
    }
  } else if (route.text) {
    await expect(page.getByText(route.text, { exact: false }).first()).toBeVisible(
      {
        timeout: 20_000,
      }
    );
  } else if (testIdError) {
    throw testIdError;
  }
}

export async function runRouteChecks(page: Page, routes: RouteCheck[]): Promise<void> {
  for (const route of routes) {
    await runRouteCheck(page, route);
  }
}

export async function firstDocumentId(page: Page): Promise<string | null> {
  const link = page.locator('a[href^="/documents/"]').first();
  if ((await link.count().catch(() => 0)) === 0) {
    return null;
  }
  const href = await link.getAttribute('href');
  if (!href) return null;
  const parts = href.split('/').filter(Boolean);
  // Expected: ["documents", "<id>"]
  if (parts.length < 2) return null;
  if (parts[0] !== 'documents') return null;
  return parts[1] ?? null;
}
