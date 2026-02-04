import { test, expect } from '@playwright/test';

test('diff controls open with shortcut', async ({ page }) => {
  await page.goto('/?testData=1', { waitUntil: 'domcontentloaded' });
  await page.keyboard.press('Meta+D');
  await expect(page.getByText('Base Commit (A)')).toBeVisible();
  await expect(page.getByText('Compare Commit (B)')).toBeVisible();
});
