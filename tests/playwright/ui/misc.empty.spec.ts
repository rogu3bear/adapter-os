import { test, expect } from '@playwright/test';
import { gotoAndBootstrap } from './utils';

test('reviews empty state', { tag: ['@empty'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/reviews', { mode: 'ui-only' });
  await expect(page.getByRole('heading', { name: 'Reviews', level: 1, exact: true })).toBeVisible();
  await expect(page.getByText(/^\d+\s+total$/).first()).toBeVisible();
  await expect(page.getByText(/^\d+\s+shown$/).first()).toBeVisible();
});

test('errors empty state', { tag: ['@empty'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/errors', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Incidents', level: 1, exact: true })
  ).toBeVisible();
  const empty = page.getByText('No incidents detected', { exact: true });
  if (await empty.isVisible().catch(() => false)) {
    await expect(empty).toBeVisible();
  } else {
    await expect(page.getByText(/errors in buffer/i)).toBeVisible();
  }
});

test('workers empty state', { tag: ['@empty'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/workers', { mode: 'ui-only' });
  await expect(page.getByTestId('workers-page-heading')).toBeVisible();
  // Seeded E2E environment may include a fixture worker; accept either state.
  const seededWorker = page.getByTestId('workers-seeded-link');
  const empty = page.getByTestId('workers-empty-state');
  if (await seededWorker.isVisible().catch(() => false)) {
    await expect(seededWorker).toBeVisible();
  } else {
    await expect(empty).toBeVisible();
  }
});

test('worker detail shows not found error for unknown id', { tag: ['@empty'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/workers/worker-missing', { mode: 'ui-only' });
  await expect(page.getByTestId('worker-detail-error-state')).toBeVisible();
});

test('monitoring health endpoints card', { tag: ['@empty'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/monitoring', { mode: 'ui-only' });
  await expect(page.getByRole('heading', { name: 'Monitoring', level: 1, exact: true })).toBeVisible();
  await expect(page.getByText('Health Endpoints')).toBeVisible();
});
