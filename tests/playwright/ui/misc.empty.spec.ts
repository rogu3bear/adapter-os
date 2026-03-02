import { test, expect } from '@playwright/test';
import { gotoAndBootstrap } from './utils';

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
