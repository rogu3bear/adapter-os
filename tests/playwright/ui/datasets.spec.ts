import { test, expect } from '@playwright/test';
import { ensureLoggedIn, expectErrorState, waitForAppReady } from './utils';

test('datasets empty state and upload dialog', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/datasets', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Datasets', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('No datasets')).toBeVisible();

  await page.getByRole('button', { name: 'Upload Dataset' }).first().click();
  await expect(
    page.getByRole('heading', { name: 'Upload Training Dataset', exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(
    page.getByRole('heading', { name: 'Upload Training Dataset', exact: true })
  ).toHaveCount(0);
});

test('dataset detail shows not found error for unknown id', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/datasets/dataset-missing', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expectErrorState(page);
});
