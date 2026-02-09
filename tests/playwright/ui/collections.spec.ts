import { test, expect } from '@playwright/test';
import { ensureLoggedIn, expectErrorState, waitForAppReady } from './utils';

test('collections empty state and create dialog', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/collections', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Collections', level: 1, exact: true })
  ).toBeVisible();
  // Seeded E2E environment may include a fixture collection; accept either state.
  const seededCollection = page.getByText('Test Collection', { exact: true });
  const empty = page.getByText('No collections yet');
  if (await seededCollection.isVisible().catch(() => false)) {
    await expect(seededCollection).toBeVisible();
  } else {
    await expect(empty).toBeVisible();
  }

  await page.getByRole('button', { name: 'New Collection' }).click();
  await expect(
    page.getByRole('heading', { name: 'Create Collection', exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(
    page.getByRole('heading', { name: 'Create Collection', exact: true })
  ).toHaveCount(0);
});

test('collection detail shows not found error for unknown id', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/collections/collection-missing', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Collection Details', level: 1, exact: true })
  ).toBeVisible();
  await expectErrorState(page);
});
