import { test, expect } from '@playwright/test';

test('empty state renders', async ({ page }) => {
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.getByText('CodeGraph Viewer')).toBeVisible();
  await expect(page.getByText('Open a CodeGraph database')).toBeVisible();
  await expect(page.getByText('Press')).toBeVisible();
});
