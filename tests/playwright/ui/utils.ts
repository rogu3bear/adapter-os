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

async function waitForBoot(page: Page, timeoutMs = 90_000): Promise<void> {
  const bootProgress = page.locator('#aos-boot-progress');
  const count = await bootProgress.count().catch(() => 0);
  if (count === 0) return;

  try {
    // Prefer DOM truth over Playwright's visibility heuristics.
    // We have seen cases where the overlay is actually hidden (class/style),
    // but Playwright continues to report it as "visible" and times out.
    await page.waitForFunction(
      () => {
        const progress = document.getElementById('aos-boot-progress');
        if (!progress) return true;
        if (progress.classList.contains('hidden')) return true;
        const style = window.getComputedStyle(progress);
        if (style.display === 'none') return true;
        if (style.visibility === 'hidden') return true;
        if (style.opacity === '0') return true;
        const rect = progress.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) return true;
        return false;
      },
      { timeout: timeoutMs }
    );
  } catch (err) {
    // Make flakes actionable: dump DOM + boot-stage diagnostics.
    const diag = await page
      .evaluate(() => {
        const nodes = Array.from(document.querySelectorAll('#aos-boot-progress'));
        const el = nodes[0] as HTMLElement | undefined;
        const boot = (window as any).aosBoot;
        const mountStatus = boot?.stages?.mount?.status ?? null;
        if (!el) {
          return { count: nodes.length, mountStatus, note: 'missing' };
        }
        const style = window.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return {
          count: nodes.length,
          className: el.className,
          display: style.display,
          visibility: style.visibility,
          opacity: style.opacity,
          rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
          mountStatus,
        };
      })
      .catch(() => null);

    console.error('[playwright] waitForBoot timeout diagnostics:', diag);
    throw err;
  }
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
  const shellLink = page.getByRole('link', { name: /^Dashboard$/ }).first();
  const shellNav = page.getByRole('navigation', { name: 'Main navigation' });
  const accountButton = page
    .getByRole('button', { name: new RegExp(AUTH_TEST_USERNAME, 'i') })
    .first();
  const shellVisible =
    (await shellLink.isVisible().catch(() => false)) ||
    (await shellNav.isVisible().catch(() => false)) ||
    (await accountButton.isVisible().catch(() => false));
  const url = (() => {
    try {
      return page.url();
    } catch {
      return 'about:blank';
    }
  })();
  return {
    url,
    onLogin: await loginHeading.isVisible().catch(() => false),
    onAuthError: await authError.isVisible().catch(() => false),
    onAuthTimeout: await authTimeout.isVisible().catch(() => false),
    signingInVisible: await signingIn.isVisible().catch(() => false),
    shellVisible,
  };
}

function formatAuthSurfaceState(state: AuthSurfaceState): string {
  return `url=${state.url}, login=${state.onLogin}, auth_error=${state.onAuthError}, auth_timeout=${state.onAuthTimeout}, signing_in=${state.signingInVisible}, shell=${state.shellVisible}`;
}

async function settleAfterAuthAction(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded', { timeout: 4_000 }).catch(() => {});
  const signingIn = page.getByText('Signing you in', { exact: false });
  if (await signingIn.isVisible().catch(() => false)) {
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

  for (let attempt = 0; attempt < attempts; attempt += 1) {
    let state = await readAuthSurfaceState(page);
    let statePathname = (() => {
      try {
        return new URL(state.url).pathname;
      } catch {
        return '';
      }
    })();

    // Already authenticated and outside auth error surfaces.
    if (
      !state.onLogin &&
      !state.onAuthError &&
      !state.onAuthTimeout &&
      (state.shellVisible || (statePathname.length > 0 && !authSurfacePath.test(statePathname)))
    ) {
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
      statePathname = (() => {
        try {
          return new URL(state.url).pathname;
        } catch {
          return '';
        }
      })();
    }

    if (!state.onLogin && !state.onAuthError && !state.onAuthTimeout) {
      // Unknown transitional state; force a deterministic entrypoint.
      if (statePathname.length > 0 && !authSurfacePath.test(statePathname)) {
        return { resolved: true, uiAttempted, finalState: state };
      }
      await gotoWithRetry(page, '/login');
      await settleAfterAuthAction(page);
      state = await readAuthSurfaceState(page);
      statePathname = (() => {
        try {
          return new URL(state.url).pathname;
        } catch {
          return '';
        }
      })();
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
        continue;
      }

      await usernameInput.fill(AUTH_TEST_USERNAME, { timeout: 5_000 }).catch(() => {});
      await passwordInput.fill(AUTH_TEST_PASSWORD, { timeout: 5_000 }).catch(() => {});
      const enabled = await loginButton.isEnabled().catch(() => false);
      if (enabled) {
        await loginButton.click().catch(() => {});
      }
      await settleAfterAuthAction(page);

      state = await readAuthSurfaceState(page);
      if (!state.onLogin && !state.onAuthError && !state.onAuthTimeout && state.shellVisible) {
        return { resolved: true, uiAttempted, finalState: state };
      }

      const serviceUnavailableVisible = await page
        .getByText(/service unavailable/i)
        .isVisible()
        .catch(() => false);
      if (serviceUnavailableVisible && attempt < attempts - 1) {
        await page.waitForTimeout(500);
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
    await page.waitForTimeout(500);
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
  await waitForAppReady(page);

  const meResp = await page.request.get('/v1/auth/me', { timeout: 8_000 }).catch(() => null);
  if (!meResp || !meResp.ok()) {
    throw new Error(
      `bootstrapAuth API fallback failed for /v1/auth/me: ${meResp?.status?.() ?? 'request-failed'}`
    );
  }

  const path = new URL(page.url()).pathname;
  if (!expectedPostAuthPath.test(path)) {
    await gotoWithRetry(page, postAuthPath);
    await waitForAppReady(page);
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
    const shellLink = page.getByRole('link', { name: 'Dashboard' });
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
  const finalState = await readAuthSurfaceState(page);
  if (finalState.onLogin || finalState.onAuthError || finalState.onAuthTimeout) {
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

export async function gotoAndBootstrap(
  page: Page,
  path: string,
  options: AuthBootstrapOptions = {}
): Promise<void> {
  await gotoWithRetry(page, path);
  await waitForAppReady(page);
  await bootstrapAuth(page, options);

  const mode = options.mode ?? 'ui-only';
  if (mode === 'none') {
    return;
  }

  if (!matchesRequestedPath(page.url(), path)) {
    await gotoWithRetry(page, path);
    await waitForAppReady(page);
    await bootstrapAuth(page, options);
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
  const heading = page.getByRole('heading', { name: 'Chat unavailable', exact: true }).first();
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
  while (Date.now() - startedAt < timeoutMs) {
    const path = currentPath(page);
    if (await page.getByTestId('chat-header').isVisible().catch(() => false)) {
      return { state: 'active', anchor: 'chat-header', path };
    }
    if (await page.getByTestId('chat-unavailable-state').isVisible().catch(() => false)) {
      return {
        state: 'unavailable',
        anchor: 'chat-unavailable-state',
        path,
        unavailableReason: await readUnavailableReason(page),
      };
    }
    if (await page.getByTestId('chat-empty-new-chat').isVisible().catch(() => false)) {
      return { state: 'empty', anchor: 'chat-empty-new-chat', path };
    }
    if (await page.getByTestId('chat-sidebar-new-session').isVisible().catch(() => false)) {
      return { state: 'empty', anchor: 'chat-sidebar-new-session', path };
    }
    if (
      await page
        .getByRole('heading', { name: 'Chat unavailable', level: 2, exact: true })
        .isVisible()
        .catch(() => false)
    ) {
      return {
        state: 'unavailable',
        anchor: 'chat-unavailable-state',
        path,
        unavailableReason: await readUnavailableReason(page),
      };
    }
    if (await page.getByText('Chat unavailable', { exact: false }).isVisible().catch(() => false)) {
      return {
        state: 'unavailable',
        anchor: 'chat-unavailable-state',
        path,
        unavailableReason: await readUnavailableReason(page),
      };
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

    const onDashboard = /^\/($|dashboard(\/|$))/.test(path);
    if (onDashboard) {
      const chatUnavailableBanner = page.getByText('Chat unavailable', { exact: false });
      if (await chatUnavailableBanner.isVisible().catch(() => false)) {
        const bannerText = (await chatUnavailableBanner.first().textContent().catch(() => null))?.trim();
        return {
          state: 'unavailable',
          anchor: 'chat-unavailable-state',
          path,
          unavailableReason: bannerText || (await readUnavailableReason(page)),
        };
      }
    }

    await page.waitForTimeout(250);
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
          page.getByRole('link', { name: 'Dashboard' })
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
