import { test, expect } from '@playwright/test';
import { resolveChatEntryContract, waitForAppReady } from './utils';

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
  await page
    .waitForURL(/\/($|dashboard(\/|$)|chat(\/|$))/, { timeout: 20_000 })
    .catch(() => {});

  // Post-login can land on /chat or dashboard fallback depending on readiness.
  const path = new URL(page.url()).pathname;
  expect(path).toMatch(/^\/($|dashboard(\/|$)|chat(\/|$))/);
  await expect(page.getByRole('link', { name: 'Dashboard' })).toBeVisible();
  if (/^\/chat(\/|$)/.test(path)) {
    const contract = await resolveChatEntryContract(page);
    if (contract.state === 'unavailable') {
      await expect(page.getByTestId('chat-unavailable-reason')).toBeVisible();
    }
  }
});
