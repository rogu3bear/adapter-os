import { test, expect } from '@playwright/test';
import { ensureLoggedIn, expectErrorState, waitForAppReady } from './utils';

test('reviews empty state', { tag: ['@empty'] }, async ({ page }) => {
  await page.goto('/reviews', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Human Review', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('No pending reviews')).toBeVisible();
});

test('errors empty state', { tag: ['@empty'] }, async ({ page }) => {
  await page.goto('/errors', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Incidents', level: 1, exact: true })
  ).toBeVisible();
  const empty = page.getByText('No errors found');
  if (await empty.isVisible().catch(() => false)) {
    await expect(empty).toBeVisible();
  } else if (await page.getByText('No errors recorded').isVisible().catch(() => false)) {
    await expect(page.getByText('No errors recorded')).toBeVisible();
  } else {
    await expect(page.getByText('Waiting for errors...', { exact: false })).toBeVisible();
  }
});

test('workers empty state', { tag: ['@empty'] }, async ({ page }) => {
  await page.goto('/workers', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Workers', level: 1, exact: true })
  ).toBeVisible();
  // Seeded E2E environment may include a fixture worker; accept either state.
  const seededWorker = page.getByText('worker-test', { exact: false });
  const empty = page.getByText('No workers yet');
  if (await seededWorker.isVisible().catch(() => false)) {
    await expect(seededWorker).toBeVisible();
  } else {
    await expect(empty).toBeVisible();
  }
});

test('worker detail shows not found error for unknown id', { tag: ['@empty'] }, async ({ page }) => {
  await page.goto('/workers/worker-missing', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expectErrorState(page);
});

test('monitoring health endpoints card', { tag: ['@empty'] }, async ({ page }) => {
  await page.goto('/monitoring', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Metrics', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Health Endpoints')).toBeVisible();
});
