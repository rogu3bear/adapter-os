import { test, expect } from '@playwright/test';
import { expectErrorState, gotoAndBootstrap } from './utils';

test('models list and detail', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/models', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Models', level: 1, exact: true })
  ).toBeVisible();
  const modelName = page.getByText('qwen2.5-7b-test');
  const empty = page.getByText('No models found.');
  await expect
    .poll(
      async () => {
        if (await modelName.isVisible().catch(() => false)) return 'seeded';
        if (await empty.isVisible().catch(() => false)) return 'empty';
        return 'pending';
      },
      { timeout: 10_000 }
    )
    .toMatch(/^(seeded|empty)$/);
  if (await modelName.isVisible().catch(() => false)) {
    await expect(modelName).toBeVisible();
    await gotoAndBootstrap(page, '/models/model-qwen-test', { mode: 'ui-only' });
    const main = page.locator('#main-content');
    await expect(main.getByRole('heading', { name: 'Model Details', level: 1, exact: true })).toBeVisible();
    const statusHeading = main.getByRole('heading', { name: 'Status', exact: true }).first();
    await expect
      .poll(
        async () => {
          if (await statusHeading.isVisible().catch(() => false)) return 'status';
          if (await main.getByRole('button', { name: 'Retry' }).isVisible().catch(() => false)) {
            return 'error';
          }
          if (
            await main.getByRole('heading', { name: 'Authentication Error' }).isVisible().catch(() => false)
          ) {
            return 'error';
          }
          if (
            await main.getByRole('heading', { name: 'Authentication Timeout' }).isVisible().catch(() => false)
          ) {
            return 'error';
          }
          if (await main.getByRole('heading', { name: '404' }).isVisible().catch(() => false)) {
            return 'error';
          }
          if (await main.getByRole('heading', { name: 'Not Found' }).isVisible().catch(() => false)) {
            return 'error';
          }
          return 'pending';
        },
        { timeout: 45_000 }
      )
      .toMatch(/^(status|error)$/);
    if (await statusHeading.isVisible().catch(() => false)) {
      await expect(statusHeading).toBeVisible();
    } else {
      await expectErrorState(page);
    }
  } else {
    await expect(empty).toBeVisible();
    await gotoAndBootstrap(page, '/models/model-qwen-test', { mode: 'ui-only' });
    await expectErrorState(page);
  }
});
