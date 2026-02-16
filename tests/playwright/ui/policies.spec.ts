import { test, expect } from '@playwright/test';
import { gotoAndBootstrap } from './utils';

test('policies create card toggles', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/policies', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Policy Packs', level: 1, exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'New Policy Pack' }).click();
  await expect(page.getByText('Create Policy Pack')).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByText('Create Policy Pack')).toHaveCount(0);
});
