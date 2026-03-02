import { test, expect, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  canonicalSoftRoutePath,
  disableAnimations,
  ensureActiveChatSession,
  gotoAndBootstrapSoftRoute,
  gotoAndBootstrap,
  gotoWithRetry,
  resolveChatEntryState,
  seeded,
  waitForAppReady,
} from './utils';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..', '..');

function sanitizeRunId(value: string): string {
  const cleaned = value.trim().replace(/[^A-Za-z0-9._-]/g, '_');
  return cleaned || 'default';
}

const runId = sanitizeRunId(process.env.PW_RUN_ID ?? 'default');
const auditDir = path.resolve(repoRoot, 'var/playwright/runs', runId, 'audit', 'visual');
const pagesDir = path.resolve(auditDir, 'pages');
const componentsDir = path.resolve(auditDir, 'components');
const manifestPath = path.resolve(auditDir, 'manifest.json');
const routeFilterRaw = (process.env.PW_CAPTURE_ROUTES ?? '').trim();
const routeFilter = new Set(
  routeFilterRaw
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value.length > 0)
);

function ensureAuditDir(): void {
  fs.mkdirSync(pagesDir, { recursive: true });
  fs.mkdirSync(componentsDir, { recursive: true });
}

function slugForRoute(routePath: string): string {
  const raw = routePath === '/' ? 'root' : routePath.replace(/\//g, '_');
  return raw.replace(/[^A-Za-z0-9._-]/g, '_').replace(/^_+/, '');
}

function extractLeptosRoutes(): string[] {
  const libRs = path.resolve(repoRoot, 'crates/adapteros-ui/src/lib.rs');
  const src = fs.readFileSync(libRs, 'utf8');
  const matches = Array.from(src.matchAll(/path!\("([^"]+)"\)/g)).map((m) => m[1]);
  return [...new Set(matches.filter((value) => value.length > 0))].sort();
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
    case '/chat/:session_id':
      return '/chat';
    default:
      return route.replace(/:([A-Za-z_][A-Za-z0-9_]*)/g, 'missing');
  }
}

const PUBLIC_ROUTES = new Set(['/login', '/safe']);
const SOFT_BOOT_READY_ROUTES = new Set(['/settings', '/user']);
const CAPTURE_ROUTE_SKIP = new Set(['/settings', '/user']);
const UNSKIP_SETTINGS_USER_AUDIT = (process.env.PW_UNSKIP_SETTINGS_USER_AUDIT ?? '').trim() === '1';
const MIN_COMPONENT_WIDTH = 16;
const MIN_COMPONENT_HEIGHT = 12;
const MIN_COMPONENT_AREA = 320;

async function capturePageScreenshot(page: Page, fileName: string): Promise<string> {
  ensureAuditDir();
  const outputPath = path.resolve(pagesDir, fileName);
  await page.screenshot({
    path: outputPath,
    fullPage: true,
    animations: 'disabled',
  });
  return outputPath;
}

type CaptureRecord = {
  originalRoute: string;
  resolvedRoute: string;
  pageShot?: string;
  status: 'captured' | 'failed' | 'skipped';
  error?: string;
  componentShots: string[];
  componentIds: string[];
};

async function navigateForCapture(page: Page, route: string): Promise<string> {
  const authSurfacePath = /^\/(?:login|auth(?:entication)?-(?:error|timeout))(?:\/|$)/i;
  const pathMatchesRoute = (currentUrl: string, routePath: string): boolean => {
    try {
      const currentPath = new URL(currentUrl).pathname;
      return currentPath === routePath || currentPath.startsWith(`${routePath}/`);
    } catch {
      return false;
    }
  };

  if (route === '/chat/:session_id') {
    await gotoAndBootstrap(page, '/chat', { mode: 'ui-only' });
    const chatState = await resolveChatEntryState(page);
    if (chatState !== 'unavailable') {
      await ensureActiveChatSession(page);
      const sessionPath = new URL(page.url()).pathname;
      if (/\/chat\/.+/.test(sessionPath)) {
        return sessionPath;
      }
    }
    return '/chat';
  }

  const resolved = resolveParamRoute(route);
  if (PUBLIC_ROUTES.has(route) || PUBLIC_ROUTES.has(resolved)) {
    await gotoWithRetry(page, resolved);
    await page.waitForLoadState('domcontentloaded', { timeout: 6_000 }).catch(() => {});
    await waitForAppReady(page, { timeoutMs: 20_000 }).catch(() => {});
    return resolved;
  }

  if (SOFT_BOOT_READY_ROUTES.has(resolved)) {
    await gotoAndBootstrapSoftRoute(page, resolved);
    return canonicalSoftRoutePath(resolved);
  }

  await gotoAndBootstrap(page, resolved, { mode: 'ui-only' });
  if (!pathMatchesRoute(page.url(), resolved) && !authSurfacePath.test(new URL(page.url()).pathname)) {
    await gotoWithRetry(page, resolved);
    await waitForAppReady(page, { timeoutMs: 20_000 }).catch(() => {});
  }
  return resolved;
}

async function collectVisibleTestIds(page: Page): Promise<string[]> {
  return await page.evaluate(() => {
    const visible = (el: Element) => {
      const h = el as HTMLElement;
      const style = window.getComputedStyle(h);
      if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') {
        return false;
      }
      const rect = h.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    };

    const ids = Array.from(document.querySelectorAll('[data-testid]'))
      .filter(visible)
      .map((el) => el.getAttribute('data-testid')?.trim() ?? '')
      .filter((id) => id.length > 0);
    return [...new Set(ids)].sort();
  });
}

function slugId(value: string): string {
  return value.replace(/[^A-Za-z0-9._-]/g, '_');
}

async function bestVisibleTestIdTarget(
  page: Page,
  testId: string
): Promise<{ target: ReturnType<Page['locator']>; width: number; height: number } | null> {
  const candidates = page.locator(`[data-testid="${testId}"]`);
  const count = await candidates.count().catch(() => 0);
  let best:
    | {
        target: ReturnType<Page['locator']>;
        area: number;
        width: number;
        height: number;
      }
    | null = null;

  for (let index = 0; index < count; index += 1) {
    const candidate = candidates.nth(index);
    if (!(await candidate.isVisible().catch(() => false))) continue;
    const box = await candidate.boundingBox().catch(() => null);
    if (!box) continue;
    const width = Math.round(box.width);
    const height = Math.round(box.height);
    const area = width * height;
    if (!best || area > best.area) {
      best = { target: candidate, area, width, height };
    }
  }

  if (!best) return null;
  return { target: best.target, width: best.width, height: best.height };
}

test.describe('visual audit screenshot capture', () => {
  test('capture all routes and visible testid components', { tag: ['@visual', '@audit'] }, async ({
    page,
  }) => {
    test.setTimeout(12 * 60_000);
    fs.rmSync(auditDir, { recursive: true, force: true });
    ensureAuditDir();
    await page.setViewportSize({ width: 1440, height: 900 });
    page.setDefaultTimeout(8_000);
    page.setDefaultNavigationTimeout(12_000);
    await disableAnimations(page);

    const records: CaptureRecord[] = [];
    const capturedGlobalTestIds = new Set<string>();

    const routes = extractLeptosRoutes().filter((route) =>
      routeFilter.size > 0 ? routeFilter.has(route) : true
    );

    for (const route of routes) {
      const record: CaptureRecord = {
        originalRoute: route,
        resolvedRoute: resolveParamRoute(route),
        status: 'failed',
        componentShots: [],
        componentIds: [],
      };
      records.push(record);
      if (!UNSKIP_SETTINGS_USER_AUDIT && CAPTURE_ROUTE_SKIP.has(record.resolvedRoute)) {
        record.status = 'skipped';
        record.error = `skipped route capture for ${record.resolvedRoute}; set PW_UNSKIP_SETTINGS_USER_AUDIT=1 to include`;
        continue;
      }

      try {
        const resolvedRoute = await navigateForCapture(page, route);
        record.resolvedRoute = resolvedRoute;
        await page.waitForLoadState('domcontentloaded', { timeout: 8_000 }).catch(() => {});
        await waitForAppReady(page, { timeoutMs: 20_000 }).catch(() => {});
        await page.waitForTimeout(250);
        const pageFileName = `${slugForRoute(route)}-desktop.png`;
        record.pageShot = await capturePageScreenshot(page, pageFileName);

        const testIds = await collectVisibleTestIds(page);
        for (const testId of testIds) {
          if (capturedGlobalTestIds.has(testId)) continue;
          const targetMeta = await bestVisibleTestIdTarget(page, testId);
          if (!targetMeta) continue;
          if (
            targetMeta.width < MIN_COMPONENT_WIDTH ||
            targetMeta.height < MIN_COMPONENT_HEIGHT ||
            targetMeta.width * targetMeta.height < MIN_COMPONENT_AREA
          ) {
            continue;
          }

          const target = targetMeta.target;
          await target.scrollIntoViewIfNeeded().catch(() => {});
          const componentPath = path.resolve(componentsDir, `${slugId(testId)}.png`);
          await target.screenshot({ path: componentPath, animations: 'disabled' });
          record.componentShots.push(componentPath);
          record.componentIds.push(testId);
          capturedGlobalTestIds.add(testId);
        }

        record.status = 'captured';
      } catch (error) {
        record.status = 'failed';
        record.error = error instanceof Error ? error.message : String(error);
        const fallbackName = `${slugForRoute(route)}-error-desktop.png`;
        record.pageShot = await capturePageScreenshot(page, fallbackName).catch(() => undefined);
      }
    }

    fs.writeFileSync(
      manifestPath,
      JSON.stringify(
        {
          runId,
          generatedAt: new Date().toISOString(),
          pagesDir,
          componentsDir,
          routeCount: records.length,
          pageScreenshotsCaptured: records.filter((r) => r.status === 'captured').length,
          skippedRoutes: records.filter((r) => r.status === 'skipped').length,
          uniqueComponentsCaptured: Array.from(
            new Set(records.flatMap((record) => record.componentIds))
          ).length,
          records,
        },
        null,
        2
      ),
      'utf8'
    );

    expect(records.length, `no routes selected; PW_CAPTURE_ROUTES="${routeFilterRaw}"`).toBeGreaterThan(0);
    const failedRoutes = records.filter((record) => record.status === 'failed');
    expect(
      failedRoutes,
      `visual audit capture must succeed for all routes; failed routes: ${failedRoutes.map((record) => `${record.originalRoute} => ${record.error ?? 'unknown error'}`).join(' | ')}`
    ).toEqual([]);
  });
});
