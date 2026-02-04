import { test, expect } from '@playwright/test';

test('graph state renders with fixture', async ({ page }) => {
  await page.goto('/?testData=1', { waitUntil: 'domcontentloaded' });
  await expect(page.getByPlaceholder('Search symbols... (Cmd+F)')).toBeVisible();
  await expect(page.getByText('Symbol Details')).toBeVisible();
  await expect(page.getByText('Diff Mode')).toBeVisible();
  await expect(page.locator('select[title=\"Layout Algorithm\"]')).toBeVisible();
});
