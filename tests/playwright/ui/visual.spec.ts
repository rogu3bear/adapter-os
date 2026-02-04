import { test, expect } from '@playwright/test';
import { disableAnimations, seeded, waitForAppReady } from './utils';

test.describe('visual baselines', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await disableAnimations(page);
  });

  test('style audit', async ({ page }) => {
    await page.goto('/style-audit', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await expect(
      page.getByRole('heading', { name: 'Style Audit', level: 1, exact: true })
    ).toBeVisible();
    await expect(page).toHaveScreenshot('style-audit.png', { fullPage: true });
  });

  test('training detail', async ({ page }) => {
    await page.goto('/training', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await page.getByText(seeded.adapterName, { exact: true }).click();
    await expect(
      page.getByRole('heading', { name: seeded.trainingJobId, level: 2, exact: true })
    ).toBeVisible();
    await expect(page).toHaveScreenshot('training-detail.png', { fullPage: true });
  });

  test('routing debug', async ({ page }) => {
    await page.goto('/routing', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await expect(
      page.getByRole('heading', { name: 'Routing Debug', level: 1, exact: true })
    ).toBeVisible();
    await expect(page).toHaveScreenshot('routing.png', { fullPage: true });
  });

  test('adapters list', async ({ page }) => {
    await page.goto('/adapters', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await expect(
      page.getByRole('heading', { name: 'Adapters', level: 1, exact: true })
    ).toBeVisible();
    await expect(page).toHaveScreenshot('adapters.png', { fullPage: true });
  });

  test('repository detail', async ({ page }) => {
    await page.goto(`/repositories/${seeded.repoId}`, { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await expect(
      page.getByRole('heading', { name: 'Repository Details', level: 1, exact: true })
    ).toBeVisible();
    await expect(page).toHaveScreenshot('repository-detail.png', { fullPage: true });
  });
});
