import { test, expect, type Locator } from '@playwright/test';
import { disableAnimations, gotoAndBootstrap, seeded } from './utils';

test.describe('visual baselines', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await disableAnimations(page);
  });

  test('style audit', { tag: ['@visual'] }, async ({ page }) => {
    await gotoAndBootstrap(page, '/style-audit', { mode: 'ui-only' });
    await expect(
      page.getByRole('heading', { name: 'Style Audit', level: 1, exact: true })
    ).toBeVisible();
    // Wait for async trace/metrics surfaces to settle before full-page capture.
    await expect(page.getByText('Loading recent traces...')).toBeHidden({ timeout: 15_000 });
    await expect(page.getByText('Loading trace data...')).toBeHidden({ timeout: 15_000 });
    await page.waitForFunction(
      () => {
        const key = '__styleAuditHeights';
        const root = document.documentElement;
        if (!root) return false;
        const current = root.scrollHeight;
        const win = window as unknown as Record<string, unknown>;
        const history = (win[key] as number[] | undefined) ?? [];
        history.push(current);
        if (history.length > 5) history.shift();
        win[key] = history;
        if (history.length < 4) return false;
        return history.every((value) => value === history[0]);
      },
      { timeout: 10_000, polling: 100 }
    );
    await expect(page).toHaveScreenshot('style-audit.png', {
      fullPage: true,
      maxDiffPixels: 2500,
    });
  });

  test('training detail', { tag: ['@visual'] }, async ({ page }) => {
    await gotoAndBootstrap(page, '/training', { mode: 'ui-only' });
    await page
      .getByRole('row', { name: new RegExp(seeded.adapterName) })
      .click();
    await expect(
      page.getByRole('heading', { name: seeded.trainingJobId, level: 2, exact: true })
    ).toBeVisible();
    const trainingDetail = page.getByTestId('training-job-detail');
    await expect(trainingDetail).toBeVisible();
    const createdRow = trainingDetail.getByTestId('training-detail-created-row');
    await expect(createdRow).toBeVisible();
    await trainingDetail.evaluate((el) => {
      const node = el as HTMLElement;
      node.style.width = '436px';
      node.style.maxWidth = '436px';
    });
    const visibleMasks = [createdRow];
    const optionalMasks = [
      trainingDetail.getByTestId('training-detail-started-row'),
      trainingDetail.getByTestId('training-detail-completed-row'),
    ];
    for (const locator of optionalMasks) {
      if ((await locator.count()) > 0) {
        visibleMasks.push(locator);
      }
    }
    await expect(trainingDetail).toHaveScreenshot('training-detail.png', {
      maxDiffPixels: 10000,
      maxDiffPixelRatio: 0.05,
      mask: visibleMasks,
    });
  });

  test('routing debug', { tag: ['@visual'] }, async ({ page }) => {
    await gotoAndBootstrap(page, '/routing', { mode: 'ui-only' });
    await expect(
      page.getByRole('heading', { name: 'Routing Debug', level: 1, exact: true })
    ).toBeVisible();
    const routingPage = page.getByTestId('routing-page');
    await expect(routingPage).toBeVisible();
    await expect(routingPage).toHaveScreenshot('routing.png', {
      maxDiffPixels: 150,
    });
  });

  test('adapters list', { tag: ['@visual'] }, async ({ page }) => {
    await gotoAndBootstrap(page, '/adapters', { mode: 'ui-only' });
    await expect(
      page.getByRole('heading', { name: 'Adapters', level: 1, exact: true })
    ).toBeVisible();
    const adaptersListCard = page.getByTestId('adapters-list-card');
    await expect(adaptersListCard).toBeVisible();
    await expect(adaptersListCard).toHaveScreenshot('adapters.png', {
      maxDiffPixels: 150,
    });
  });

  test('repository detail', { tag: ['@visual'] }, async ({ page }) => {
    await gotoAndBootstrap(page, `/repositories/${seeded.repoId}`, { mode: 'ui-only' });
    await expect(
      page.getByRole('heading', { name: 'Repository Details', level: 1, exact: true })
    ).toBeVisible();
    const statusCard = page.getByTestId('repo-detail-status-card');
    const infoCard = page.getByTestId('repo-detail-info-card');
    const languagesCard = page.getByTestId('repo-detail-languages-card');
    const scanCard = page.getByTestId('repo-detail-scan-card');
    await expect(statusCard).toBeVisible();
    await expect(infoCard).toBeVisible();
    await expect(languagesCard).toBeVisible();
    await expect(scanCard).toBeVisible();
    const masks: Locator[] = [];
    const optionalMasks = [
      infoCard.getByText('Created', { exact: true }).locator('..'),
      infoCard.getByText('Updated', { exact: true }).locator('..'),
      infoCard.getByText('Latest Scan', { exact: true }).locator('..'),
    ];
    for (const locator of optionalMasks) {
      if ((await locator.count()) > 0) {
        masks.push(locator);
      }
    }
    await expect(infoCard).toHaveScreenshot('repository-detail.png', {
      maxDiffPixels: 800,
      maxDiffPixelRatio: 0.03,
      mask: masks,
    });
  });
});
