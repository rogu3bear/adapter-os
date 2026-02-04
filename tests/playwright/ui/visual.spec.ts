import { test, expect } from '@playwright/test';
import { disableAnimations, ensureLoggedIn, seeded, waitForAppReady } from './utils';

test.describe('visual baselines', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await disableAnimations(page);
  });

  test('style audit', { tag: ['@visual'] }, async ({ page }) => {
    await page.goto('/style-audit', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);
    await expect(
      page.getByRole('heading', { name: 'Style Audit', level: 1, exact: true })
    ).toBeVisible();
    await expect(page).toHaveScreenshot('style-audit.png', {
      fullPage: true,
      maxDiffPixels: 50,
    });
  });

  test('training detail', { tag: ['@visual'] }, async ({ page }) => {
    await page.goto('/training', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);
    await page
      .getByRole('row', { name: new RegExp(seeded.adapterName) })
      .click();
    await expect(
      page.getByRole('heading', { name: seeded.trainingJobId, level: 2, exact: true })
    ).toBeVisible();
    const main = page.locator('#main-content');
    const masks = [
      page.getByText('Created', { exact: true }).locator('..'),
      page.getByText('Started', { exact: true }).locator('..'),
      page.getByText('Completed', { exact: true }).locator('..'),
    ];
    const visibleMasks = [];
    for (const locator of masks) {
      if ((await locator.count()) > 0) {
        visibleMasks.push(locator);
      }
    }
    await expect(main).toHaveScreenshot('training-detail.png', {
      maxDiffPixels: 10000,
      maxDiffPixelRatio: 0.03,
      mask: visibleMasks,
    });
  });

  test('routing debug', { tag: ['@visual'] }, async ({ page }) => {
    await page.goto('/routing', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);
    await expect(
      page.getByRole('heading', { name: 'Routing Debug', level: 1, exact: true })
    ).toBeVisible();
    const main = page.locator('#main-content');
    await expect(main).toHaveScreenshot('routing.png', {
      maxDiffPixels: 50,
    });
  });

  test('adapters list', { tag: ['@visual'] }, async ({ page }) => {
    await page.goto('/adapters', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);
    await expect(
      page.getByRole('heading', { name: 'Adapters', level: 1, exact: true })
    ).toBeVisible();
    const main = page.locator('#main-content');
    await expect(main).toHaveScreenshot('adapters.png', {
      maxDiffPixels: 50,
    });
  });

  test('repository detail', { tag: ['@visual'] }, async ({ page }) => {
    await page.goto(`/repositories/${seeded.repoId}`, { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);
    await expect(
      page.getByRole('heading', { name: 'Repository Details', level: 1, exact: true })
    ).toBeVisible();
    const main = page.locator('#main-content');
    const masks = [
      page.getByText('Created', { exact: true }).locator('..'),
      page.getByText('Updated', { exact: true }).locator('..'),
      page.getByText('Latest Scan', { exact: true }).locator('..'),
      page.getByText('Latest Commit', { exact: true }).locator('..'),
    ];
    const visibleMasks = [];
    for (const locator of masks) {
      if ((await locator.count()) > 0) {
        visibleMasks.push(locator);
      }
    }
    await expect(main).toHaveScreenshot('repository-detail.png', {
      maxDiffPixels: 2500,
      maxDiffPixelRatio: 0.02,
      mask: visibleMasks,
    });
  });
});
