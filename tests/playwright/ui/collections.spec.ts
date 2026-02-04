import { test, expect } from '@playwright/test';
import { expectErrorState, waitForAppReady } from './utils';

test('collections empty state and create dialog', async ({ page }) => {
  await page.goto('/collections', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Collections', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('No collections yet')).toBeVisible();

  await page.getByRole('button', { name: 'New Collection' }).click();
  await expect(
    page.getByRole('heading', { name: 'Create Collection', exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(
    page.getByRole('heading', { name: 'Create Collection', exact: true })
  ).toHaveCount(0);
});

test('collection detail shows not found error for unknown id', async ({ page }) => {
  await page.goto('/collections/collection-missing', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Collection Details', level: 1, exact: true })
  ).toBeVisible();
  await expectErrorState(page);
});
