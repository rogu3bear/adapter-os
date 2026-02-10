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

async function waitForBoot(page: Page): Promise<void> {
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
      { timeout: 90_000 }
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

export async function waitForAppReady(page: Page): Promise<void> {
  await waitForBoot(page);
}

export async function ensureLoggedIn(page: Page): Promise<void> {
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

  for (let attempt = 0; attempt < 2; attempt += 1) {
    const authError = page.getByRole('heading', { name: 'Authentication Error' });
    const authTimeout = page.getByRole('heading', { name: 'Authentication Timeout' });
    const onAuthError =
      (await authError.isVisible().catch(() => false)) ||
      (await authTimeout.isVisible().catch(() => false));
    if (onAuthError) {
      const goToLogin = page.getByRole('button', { name: /Go to Login/i });
      if (await goToLogin.isVisible().catch(() => false)) {
        await goToLogin.click();
      }
      await waitForBoot(page);
    }

    const loginHeading = page.getByRole('heading', { name: 'Login', exact: true });
    const onLogin = await loginHeading.isVisible().catch(() => false);
    if (onLogin) {
      await page.getByLabel('Username').fill('test@example.com');
      await page.getByLabel('Password').fill('password');
      await page.getByRole('button', { name: 'Log in' }).click();
      await waitForBoot(page);
    }

    const stillAuthError =
      (await authError.isVisible().catch(() => false)) ||
      (await authTimeout.isVisible().catch(() => false));
    const stillLogin = await loginHeading.isVisible().catch(() => false);
    if (!stillAuthError && !stillLogin) {
      return;
    }
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

// Route smoke test helpers (extracted from smoke specs)
export type RouteCheck = {
  path: string;
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
  await gotoWithRetry(page, route.path);
  await waitForAppReady(page);
  if (route.path !== '/login') {
    await ensureLoggedIn(page);
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
