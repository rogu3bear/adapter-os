import { test, expect, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  disableAnimations,
  ensureActiveChatSession,
  gotoAndBootstrap,
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

const PUBLIC_ROUTES = new Set(['/login', '/safe', '/style-audit']);
const SKIP_AUTOMATED_AUDIT = new Set<string>([
  // Requires a paused inference fixture; keep this manual-only for now.
  '/reviews/:pause_id',
]);

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
    case '/stacks/:id':
      return `/stacks/${seeded.stackId}`;
    case '/collections/:id':
      return `/collections/${seeded.collectionId}`;
    case '/documents/:id':
      return `/documents/${seeded.documentId}`;
    case '/datasets/:id':
      return `/datasets/${seeded.datasetId}`;
    case '/repositories/:id':
      return `/repositories/${seeded.repoId}`;
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

async function countPrimaryHeadingsOutsidePanicOverlay(page: Page): Promise<number> {
  const total = await page.getByRole('heading', { level: 1 }).count();
  const panicOverlay = page.locator('#aos-panic-overlay');
  const overlayVisible = await panicOverlay.isVisible().catch(() => false);
  if (!overlayVisible) return total;
  const overlayCount = await panicOverlay.getByRole('heading', { level: 1 }).count();
  return Math.max(0, total - overlayCount);
}

async function countAnyHeadingsOutsidePanicOverlay(page: Page): Promise<number> {
  const total = await page.getByRole('heading').count();
  const panicOverlay = page.locator('#aos-panic-overlay');
  const overlayVisible = await panicOverlay.isVisible().catch(() => false);
  if (!overlayVisible) return total;
  const overlayCount = await panicOverlay.getByRole('heading').count();
  return Math.max(0, total - overlayCount);
}

async function assertBestPractices(page: Page, route: RouteSpec): Promise<void> {
  const title = await page.title();
  expect.soft(title.trim().length, `missing document title for ${route.resolvedPath}`).toBeGreaterThan(0);

  const lang = await page.locator('html').getAttribute('lang');
  expect.soft(!!(lang && lang.trim()), `missing <html lang> for ${route.resolvedPath}`).toBeTruthy();

  if (route.originalPath !== '/style-audit') {
    if (route.originalPath === '/login') {
      await expect
        .poll(
          async () => countAnyHeadingsOutsidePanicOverlay(page),
          { timeout: 10_000 }
        )
        .toBeGreaterThan(0);
    } else {
      await expect
        .poll(
          async () => countPrimaryHeadingsOutsidePanicOverlay(page),
          { timeout: 10_000 }
        )
        .toBeGreaterThan(0);
    }
  }

  const imgMissingAlt = await page.locator('img:not([alt])').count();
  expect.soft(imgMissingAlt, `img missing alt for ${route.resolvedPath}`).toBe(0);

  if (route.shellWrapped) {
    await expect.soft(page.locator('a.skip-to-main')).toHaveCount(1);
    await expect.soft(page.locator('main#main-content')).toHaveCount(1);
  }
}

async function navigateAndAuth(page: Page, route: RouteSpec): Promise<void> {
  if (route.requiresAuth) {
    await gotoAndBootstrap(page, route.resolvedPath, {
      mode: 'ui-only',
    });
    await waitForAppReady(page);
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
