import { test, expect } from '@playwright/test';
import { ensureLoggedIn, seeded, waitForAppReady } from './utils';

test('runs list and detail', async ({ page }) => {
  await page.goto('/runs', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Runs', level: 1, exact: true })
  ).toBeVisible();
  const runLabel =
    seeded.runId.length > 12 ? `${seeded.runId.slice(0, 12)}...` : seeded.runId;
  await expect(
    page.getByRole('button', { name: new RegExp(`^${runLabel}`) })
  ).toBeVisible();

  await page.goto(`/runs/${seeded.runId}`, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Run Detail', level: 2, exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Run Summary', level: 3, exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Provenance', level: 3, exact: true })
  ).toBeVisible();

  const tabNav = page.getByRole('navigation').filter({ hasText: 'Overview' });
  await tabNav.getByRole('button', { name: 'Trace', exact: true }).click();
  await expect(
    page.getByText(
      'Full inference trace with timeline visualization, latency breakdown, and token-level routing decisions.'
    )
  ).toBeVisible();

  await tabNav.getByRole('button', { name: 'Routing', exact: true }).click();
  await expect(
    page.getByText(
      'K-sparse routing decisions showing which adapters were selected and their gate values.'
    )
  ).toBeVisible();
  await page
    .getByRole('button', { name: /Token Routing Decisions/ })
    .click();
  await expect(page.getByText('Adapters:').first()).toBeVisible();
  const showMore = page.getByRole('button', { name: 'Show more' });
  if (await showMore.isVisible().catch(() => false)) {
    await showMore.click();
  }

  await tabNav.getByRole('button', { name: 'Receipt', exact: true }).click();
  await expect(page.getByText('Receipts & Hashes')).toBeVisible();
});

test('primary flow: chat to run detail', async ({ page }) => {
  await page.goto('/chat', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Chat', level: 1, exact: true })
  ).toBeVisible();

  await page.goto(`/runs/${seeded.runId}`, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Run Detail', level: 2, exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Run Summary', level: 3, exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Provenance', level: 3, exact: true })
  ).toBeVisible();
});

test('token decisions paging shows more', async ({ page }) => {
  await page.goto(`/runs/${seeded.runId}?tab=trace`, {
    waitUntil: 'domcontentloaded',
  });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await page
    .getByRole('button', { name: /Token Routing Decisions/ })
    .click();
  await expect(page.getByText('Adapters:').first()).toBeVisible();
  const showMore = page.getByRole('button', { name: 'Show more' });
  if (await showMore.isVisible().catch(() => false)) {
    await showMore.click();
    await expect(showMore).toBeHidden();
  }
});
