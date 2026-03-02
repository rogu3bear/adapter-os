import { test, expect } from '@playwright/test';
import { expectErrorState, gotoAndBootstrap } from '../ui/utils';

test('datasets empty state and upload dialog', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/datasets', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Datasets', level: 1, exact: true })
  ).toBeVisible();
  // Seeded E2E environment may include a fixture dataset; accept either state.
  const seededDataset = page.getByText('Test Dataset', { exact: true });
  const empty = page.getByText('No datasets');
  const datasetCount = page.getByText(/\d+\s+dataset\(s\)/i);
  await expect
    .poll(
      async () => {
        if (await seededDataset.isVisible().catch(() => false)) return 'seeded';
        if (await empty.isVisible().catch(() => false)) return 'empty';
        if (await datasetCount.isVisible().catch(() => false)) return 'count';
        return 'pending';
      },
      { timeout: 10_000 }
    )
    .toMatch(/^(seeded|empty|count)$/);
  if (await seededDataset.isVisible().catch(() => false)) {
    await expect(seededDataset).toBeVisible();
  } else if (await empty.isVisible().catch(() => false)) {
    await expect(empty).toBeVisible();
  } else {
    await expect(datasetCount).toBeVisible();
  }

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
  await gotoAndBootstrap(page, '/datasets/dataset-missing', { mode: 'ui-only' });
  await expectErrorState(page);
});
