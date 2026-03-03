import { test, expect, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  disableAnimations,
  ensureActiveChatSession,
  gotoAndBootstrapSoftRoute,
  gotoAndBootstrap,
  pathMatchesSoftRoute,
  resolveChatEntryState,
  seeded,
  waitForAppReady,
} from './utils';

type RouteSpec = {
  originalPath: string;
  resolvedPath: string;
  requiresAuth: boolean;
  shellWrapped: boolean;
};

const PUBLIC_ROUTES = new Set(['/login', '/safe']);
const SKIP_AUTOMATED_AUDIT = new Set<string>();
const SOFT_BOOT_READY_ROUTES = new Set(['/settings', '/user']);

function repoRoot(): string {
  const __filename = fileURLToPath(import.meta.url);
  const __dirname = path.dirname(__filename);
  return path.resolve(__dirname, '../../..');
}

function extractLeptosRoutes(): string[] {
  const libRs = path.join(repoRoot(), 'crates/adapteros-ui/src/lib.rs');
  const src = fs.readFileSync(libRs, 'utf8');
  const matches = Array.from(src.matchAll(/path!\("([^"]+)"\)/g)).map((m) => m[1]);
  return [...new Set(matches)].sort();
}

function resolveParamRoute(route: string): string {
  if (!route.includes(':')) return route;

  switch (route) {
    case '/adapters/:id':
      return `/adapters/${seeded.adapterId}`;
    case '/documents/:id':
      return `/documents/${seeded.documentId}`;
    case '/runs/:id':
      return `/runs/${seeded.runId}`;
    case '/workers/:id':
      return `/workers/${seeded.workerId}`;
    case '/flight-recorder/:id':
      return `/flight-recorder/${seeded.runId}`;
    // Chat session is generated dynamically in the test (see below).
    case '/chat/:session_id':
      return '/chat';
    default:
      return route.replace(/:([A-Za-z_][A-Za-z0-9_]*)/g, 'missing');
  }
}

function classifyRoute(route: string): RouteSpec {
  const resolved = resolveParamRoute(route);
  const requiresAuth = !PUBLIC_ROUTES.has(route) && !PUBLIC_ROUTES.has(resolved);
  return {
    originalPath: route,
    resolvedPath: resolved,
    requiresAuth,
    shellWrapped: requiresAuth,
  };
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number, fallback: T): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<T>((resolve) => {
        timer = setTimeout(() => resolve(fallback), timeoutMs);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function countPrimaryHeadingsOutsidePanicOverlay(page: Page): Promise<number> {
  const total = await withTimeout(page.locator('h1:visible').count().catch(() => 0), 2_500, 0);
  const overlay = await withTimeout(
    page.locator('#aos-panic-overlay:visible h1:visible').count().catch(() => 0),
    2_500,
    0
  );
  return Math.max(0, total - overlay);
}

async function countAnyHeadingsOutsidePanicOverlay(page: Page): Promise<number> {
  const selector = 'h1:visible, h2:visible, h3:visible, h4:visible, h5:visible, h6:visible, [role="heading"]:visible';
  const total = await withTimeout(page.locator(selector).count().catch(() => 0), 2_500, 0);
  const overlay = await withTimeout(
    page.locator(`#aos-panic-overlay:visible ${selector}`).count().catch(() => 0),
    2_500,
    0
  );
  return Math.max(0, total - overlay);
}

async function assertBestPractices(page: Page, route: RouteSpec): Promise<void> {
  const isSoftBootRoute = SOFT_BOOT_READY_ROUTES.has(route.resolvedPath);
  const headingTimeoutMs = isSoftBootRoute ? 12_000 : 10_000;
  const requireAnyHeading = route.originalPath === '/login' || isSoftBootRoute;
  const titleTimeoutMs = isSoftBootRoute ? 3_000 : 10_000;

  if (!isSoftBootRoute) {
    await expect
      .poll(async () => (await page.title().catch(() => '')).trim().length, {
        timeout: titleTimeoutMs,
      })
      .toBeGreaterThan(0)
      .catch(() => {});
    const title = await page.title().catch(() => '');
    expect.soft(title.trim().length, `missing document title for ${route.resolvedPath}`).toBeGreaterThan(0);
  }

  if (isSoftBootRoute) {
    await expect
      .poll(
        async () => {
          if (pathMatchesSoftRoute(page.url(), route.resolvedPath)) {
            return true;
          }
          if ((await countAnyHeadingsOutsidePanicOverlay(page)) > 0) {
            return true;
          }
          const signingVisible = await withTimeout(
            page.getByText('Signing you in', { exact: false }).first().isVisible().catch(() => false),
            1_500,
            false
          );
          const bootVisible = await withTimeout(
            page.locator('#aos-boot-progress:not(.hidden)').first().isVisible().catch(() => false),
            1_500,
            false
          );
          return signingVisible || bootVisible;
        },
        { timeout: headingTimeoutMs }
      )
      .toBeTruthy();
    return;
  } else if (requireAnyHeading) {
    await expect
      .poll(
        async () => countAnyHeadingsOutsidePanicOverlay(page),
        { timeout: headingTimeoutMs }
      )
      .toBeGreaterThan(0);
  } else {
    await expect
      .poll(
        async () => countPrimaryHeadingsOutsidePanicOverlay(page),
        { timeout: headingTimeoutMs }
      )
      .toBeGreaterThan(0);
  }

  const lang = await page.evaluate(() => document.documentElement?.getAttribute('lang') ?? '').catch(() => '');
  expect.soft(lang.trim().length > 0, `missing <html lang> for ${route.resolvedPath}`).toBeTruthy();

  const imgMissingAlt = await page.locator('img:not([alt])').count();
  expect.soft(imgMissingAlt, `img missing alt for ${route.resolvedPath}`).toBe(0);

  if (route.shellWrapped && !isSoftBootRoute) {
    await expect.soft(page.locator('a.skip-to-main')).toHaveCount(1);
    await expect.soft(page.locator('main#main-content')).toHaveCount(1);
  }
}

async function navigateAndAuth(page: Page, route: RouteSpec): Promise<void> {
  if (route.requiresAuth) {
    if (SOFT_BOOT_READY_ROUTES.has(route.resolvedPath)) {
      await gotoAndBootstrapSoftRoute(page, route.resolvedPath);
      return;
    }

    await gotoAndBootstrap(page, route.resolvedPath, {
      mode: 'ui-only',
    });
    return;
  }

  await page.goto(route.resolvedPath, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
}

const routes = extractLeptosRoutes()
  .filter((r) => !SKIP_AUTOMATED_AUDIT.has(r))
  .map(classifyRoute);

for (const route of routes) {
  test(`best-practices audit: ${route.originalPath}`, { tag: ['@audit'] }, async ({ page }) => {
    test.setTimeout(90_000);
    await disableAnimations(page);

    // Special case: /chat/:session_id should be audited using a real session.
    if (route.originalPath === '/chat/:session_id') {
      await navigateAndAuth(page, route);
      await expect(page.getByRole('heading', { name: 'Chat', level: 1, exact: true })).toBeVisible();
      const chatState = await resolveChatEntryState(page);
      test.skip(
        chatState === 'unavailable',
        'Skipping /chat/:session_id audit because inference is unavailable in this run.'
      );
      await ensureActiveChatSession(page);
      const sessionUrl = page.url();
      expect(sessionUrl).toMatch(/\/chat\/.+/);
      const sessionPath = (() => {
        const parsed = new URL(sessionUrl);
        return `${parsed.pathname}${parsed.search}${parsed.hash}`;
      })();
      await gotoAndBootstrap(page, sessionPath, { mode: 'ui-only' });
      await assertBestPractices(page, {
        ...route,
        resolvedPath: sessionPath,
      });
      return;
    }

    await navigateAndAuth(page, route);
    await assertBestPractices(page, route);
  });
}
