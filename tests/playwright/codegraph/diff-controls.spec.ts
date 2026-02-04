import { test, expect } from '@playwright/test';

test('diff controls open with shortcut', async ({ page }) => {
  await page.goto('/?testData=1', { waitUntil: 'domcontentloaded' });
  await expect(page.getByPlaceholder('Search symbols... (Cmd+F)')).toBeVisible();
  await page.getByTitle('Toggle Diff Mode (Cmd+D)').click();
  await expect(page.getByText('Base Commit (A)')).toBeVisible();
  await expect(page.getByText('Compare Commit (B)')).toBeVisible();
});
