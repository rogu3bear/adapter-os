import { test, expect } from '@playwright/test';
import { seeded, waitForAppReady } from './utils';

test('training list shows seeded job', async ({ page }) => {
  await page.goto('/training', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Training Jobs', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText(seeded.adapterName)).toBeVisible();
});

test('training detail shows job details and metrics', async ({ page }) => {
  await page.goto('/training', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await page.getByText(seeded.adapterName, { exact: true }).click();
  await expect(
    page.getByRole('heading', { name: seeded.trainingJobId, level: 2, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Job Details')).toBeVisible();
  await expect(
    page.getByRole('heading', { name: /Final Metrics|Training Metrics/ })
  ).toBeVisible();
});
