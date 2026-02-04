import { test, expect } from '@playwright/test';
import { waitForAppReady } from './utils';

test.use({ storageState: { cookies: [], origins: [] } });

test('public safe mode renders without auth', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/safe', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Safety Mode', level: 3, exact: true })
  ).toBeVisible();
});

test('public style audit renders without auth', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/style-audit', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Style Audit', level: 1, exact: true })
  ).toBeVisible();
});
