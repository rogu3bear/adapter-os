import { test, expect } from '@playwright/test';
import { waitForAppReady } from './utils';

test.use({ storageState: { cookies: [], origins: [] } });

test('login page loads and authenticates', { tag: ['@smoke'] }, async ({ page }) => {
  await page.goto('/login', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Login', level: 3, exact: true })
  ).toBeVisible();
  await page.getByLabel('Username').fill('test@example.com');
  await page.getByLabel('Password').fill('password');
  await page.getByRole('button', { name: 'Log in' }).click();
  await page.waitForLoadState('domcontentloaded');
  await waitForAppReady(page);
  await expect(
    page.getByRole('heading', { name: 'Dashboard', level: 1, exact: true })
  ).toBeVisible();
});
