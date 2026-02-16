import { test, expect } from '@playwright/test';
import { gotoAndBootstrap, seeded } from './utils';

test('adapters list and detail', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/adapters', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Adapters', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText(seeded.adapterName)).toBeVisible();

  await gotoAndBootstrap(page, `/adapters/${seeded.adapterId}`, { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Adapter Details', level: 1, exact: true })
  ).toBeVisible();
  await expect(page.getByText('Basic Information')).toBeVisible();
});
