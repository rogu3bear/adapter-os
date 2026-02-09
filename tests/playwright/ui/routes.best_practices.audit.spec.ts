import { test, expect, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { useConsoleCatcher } from './helpers/console-catcher';
import { disableAnimations, ensureLoggedIn, seeded, waitForAppReady } from './utils';

useConsoleCatcher(test);

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

async function countH1OutsidePanicOverlay(page: Page): Promise<number> {
  return page.evaluate(() => {
    const hs = Array.from(document.querySelectorAll('h1'));
    return hs.filter((h) => !h.closest('#aos-panic-overlay')).length;
  });
}

async function assertBestPractices(page: Page, route: RouteSpec): Promise<void> {
  const title = await page.title();
  expect.soft(title.trim().length, `missing document title for ${route.resolvedPath}`).toBeGreaterThan(0);

  const lang = await page.locator('html').getAttribute('lang');
  expect.soft(!!(lang && lang.trim()), `missing <html lang> for ${route.resolvedPath}`).toBeTruthy();

  if (route.originalPath !== '/style-audit') {
    const h1Count = await countH1OutsidePanicOverlay(page);
    expect.soft(h1Count, `expected exactly one <h1> for ${route.resolvedPath}`).toBe(1);
  }

  const imgMissingAlt = await page.locator('img:not([alt])').count();
  expect.soft(imgMissingAlt, `img missing alt for ${route.resolvedPath}`).toBe(0);

  if (route.shellWrapped) {
    await expect.soft(page.locator('a.skip-to-main')).toHaveCount(1);
    await expect.soft(page.locator('main#main-content')).toHaveCount(1);
  }
}

async function navigateAndAuth(page: Page, route: RouteSpec): Promise<void> {
  await page.goto(route.resolvedPath, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  if (route.requiresAuth) {
    await ensureLoggedIn(page);
    await waitForAppReady(page);
  }
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
      await page.getByRole('button', { name: 'New Session' }).click();
      await expect(page.getByRole('heading', { name: 'Chat Session', exact: true })).toBeVisible();
      const sessionUrl = page.url();
      expect(sessionUrl).toMatch(/\/chat\/.+/);
      await page.goto(sessionUrl, { waitUntil: 'domcontentloaded' });
      await waitForAppReady(page);
      await assertBestPractices(page, {
        ...route,
        resolvedPath: sessionUrl.replace(page.url().split('/').slice(0, 3).join('/'), ''),
      });
      return;
    }

    await navigateAndAuth(page, route);
    await assertBestPractices(page, route);
  });
}

