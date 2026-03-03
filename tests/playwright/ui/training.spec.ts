import { test, expect } from '@playwright/test';
import { gotoAndBootstrap, seeded } from './utils';

test('training list shows seeded job', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: /^(Training Jobs|Build)$/, level: 1 })
  ).toBeVisible();
  await expect(page.getByText(seeded.adapterName)).toBeVisible();
});

test('training detail shows job details and metrics', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
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
