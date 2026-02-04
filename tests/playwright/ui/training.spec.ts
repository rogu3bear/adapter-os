import { test, expect } from '@playwright/test';
import { ensureLoggedIn, seeded, waitForAppReady } from './utils';

test('training list shows seeded job', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/training', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Training Jobs', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText(seeded.adapterName)).toBeVisible();
});

test('training detail shows job details and metrics', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await page.goto('/training', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await page
    .getByRole('row', { name: new RegExp(seeded.adapterName) })
    .click();
  await expect(
    page.getByRole('heading', { name: seeded.trainingJobId, level: 2, exact: true })
  ).toBeVisible({ timeout: 20_000 });
  await expect(page.getByText('Job Details')).toBeVisible();
  await expect(
    page.getByRole('tab', { name: 'Overview', exact: true })
  ).toBeVisible();
});
