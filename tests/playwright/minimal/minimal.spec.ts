import { test, expect } from '@playwright/test';

test('static minimal page loads', async ({ page }) => {
  await page.goto('/index-minimal.html', { waitUntil: 'domcontentloaded' });
  await expect(page).toHaveTitle('adapterOS - Minimal MVP');
  await expect(page.locator('#root')).toBeVisible();
});
