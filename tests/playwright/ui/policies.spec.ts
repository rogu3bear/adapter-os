import { test, expect } from '@playwright/test';
import { gotoAndBootstrap } from './utils';

test('policies create card toggles', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/policies', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Policies', level: 1, exact: true })
  ).toBeVisible();
  const newPolicyPack = page.getByRole('button', { name: 'New Policy Pack' });
  const visible = await newPolicyPack.isVisible().catch(() => false);
  if (!visible) {
    await expect(page.getByText('No policy packs found')).toBeVisible();
    return;
  }
  await newPolicyPack.click();
  await expect(page.getByText('Create Policy Pack')).toBeVisible();
  await page.getByRole('button', { name: 'Cancel' }).click();
  await expect(page.getByText('Create Policy Pack')).toHaveCount(0);
});
