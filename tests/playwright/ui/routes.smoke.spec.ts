import { test, expect, type Page } from '@playwright/test';
import { ensureLoggedIn, seeded, waitForAppReady } from './utils';

async function gotoWithRetry(page: Page, path: string): Promise<void> {
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

test('route smoke coverage', async ({ page }) => {
  test.setTimeout(180_000);
  const routes: Array<{
    path: string;
    heading?: string;
    text?: string | RegExp;
    headingLevel?: number;
  }> = [
    { path: '/login', heading: 'Login' },
    { path: '/', heading: 'Dashboard' },
    { path: '/dashboard', heading: 'Dashboard' },
    { path: '/adapters', heading: 'Adapters' },
    { path: `/adapters/${seeded.adapterId}`, heading: 'Adapter Details' },
    { path: '/chat', heading: 'Chat' },
    { path: '/system', heading: 'Infrastructure' },
    { path: '/settings', heading: 'Settings' },
    { path: '/user', heading: 'Settings' },
    { path: '/models', heading: 'Models' },
    { path: '/policies', heading: 'Policy Packs' },
    { path: '/training', heading: 'Training Jobs' },
    { path: '/stacks', heading: 'Runtime Stacks' },
    { path: `/stacks/${seeded.stackId}`, heading: 'Stack Details' },
    { path: '/collections', heading: 'Collections' },
    { path: '/collections/collection-missing', heading: 'Collection Details' },
    { path: '/documents', heading: 'Documents' },
    { path: `/documents/${seeded.documentId}`, heading: 'Document Details' },
    { path: '/datasets', heading: 'Datasets' },
    { path: '/datasets/dataset-missing', text: /dataset not found/i },
    { path: '/admin', heading: 'Administration' },
    { path: '/audit', heading: 'Audit Log' },
    { path: '/runs', heading: 'Runs' },
    { path: `/runs/${seeded.runId}`, heading: 'Run Detail', headingLevel: 2 },
    { path: '/flight-recorder', heading: 'Runs' },
    { path: `/flight-recorder/${seeded.runId}`, heading: 'Run Detail', headingLevel: 2 },
    { path: '/diff', heading: 'Run Diff' },
    { path: '/workers', heading: 'Workers' },
    { path: '/workers/worker-missing', text: /not found/i },
    { path: '/monitoring', heading: 'Metrics' },
    { path: '/errors', heading: 'Incidents' },
    { path: '/routing', heading: 'Routing Debug' },
    { path: '/repositories', heading: 'Repositories' },
    { path: `/repositories/${seeded.repoId}`, heading: 'Repository Details' },
    { path: '/reviews', heading: 'Human Review' },
    { path: '/agents', heading: 'Agent Orchestration' },
    { path: '/safe', text: 'Safety Mode' },
    { path: '/style-audit', heading: 'Style Audit' },
  ];

  for (const route of routes) {
    await test.step(route.path, async () => {
      await gotoWithRetry(page, route.path);
      await waitForAppReady(page);
      await ensureLoggedIn(page);

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
              page.getByRole('heading', {
                name: 'Dashboard',
                level: 1,
                exact: true,
              })
            ).toBeVisible({ timeout: 20_000 });
          }
        } else {
          await expect(heading).toBeVisible({ timeout: 20_000 });
        }
      } else if (route.text) {
        await expect(page.getByText(route.text, { exact: false })).toBeVisible({
          timeout: 20_000,
        });
      }
    });
  }
});

test('chat session deep route loads', async ({ page }) => {
  await page.goto('/chat', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Chat', level: 1, exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'New Session' }).click();
  await expect(
    page.getByRole('heading', { name: 'Chat Session', level: 1, exact: true })
  ).toBeVisible();
  await expect(page).toHaveURL(/\/chat\/.+/);
});
