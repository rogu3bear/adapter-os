import { test, expect } from '@playwright/test';
import { seeded, waitForAppReady } from './utils';

test('adapters list and detail', async ({ page }) => {
  await page.goto('/adapters', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Adapters', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText(seeded.adapterName)).toBeVisible();

  await page.goto(`/adapters/${seeded.adapterId}`, { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Adapter Details', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Basic Information')).toBeVisible();
});
