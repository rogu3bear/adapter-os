import { test, expect } from '@playwright/test';
import { ensureLoggedIn, expectErrorState, waitForAppReady } from './utils';

test('models list and detail', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/models', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(
    page.getByRole('heading', { name: 'Models', level: 1, exact: true })
  ).toBeVisible();
  const modelName = page.getByText('qwen2.5-7b-test');
  if (await modelName.isVisible().catch(() => false)) {
    await expect(modelName).toBeVisible();
    await page.goto('/models/model-qwen-test', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);
    await expect(
      page.getByRole('heading', { name: 'Model Details', level: 1, exact: true })
    ).toBeVisible();
    await expect(page.getByText('Status')).toBeVisible();
  } else {
    await expect(page.getByText('No models found.')).toBeVisible();
    await page.goto('/models/model-qwen-test', { waitUntil: 'domcontentloaded' });
    await waitForAppReady(page);
    await ensureLoggedIn(page);
    await expectErrorState(page);
  }
});
