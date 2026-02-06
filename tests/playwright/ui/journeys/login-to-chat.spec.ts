/**
 * Journey: Login → land on /chat workspace.
 *
 * Starts unauthenticated, logs in via the login form, and verifies the
 * chat workspace loads with a session list and message input.
 */

import { test, expect } from '@playwright/test';
import { waitForAppReady } from '../utils';
import { useConsoleCatcher } from '../helpers/console-catcher';

// Clear storageState so we start unauthenticated.
test.use({ storageState: { cookies: [], origins: [] } });

useConsoleCatcher(test);

test('login then land on chat workspace', { tag: ['@flow'] }, async ({ page }) => {
  await page.goto('/chat', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);

  // Should redirect to login since we cleared storageState.
  await expect(
    page.getByRole('heading', { name: 'Login', exact: true })
  ).toBeVisible({ timeout: 20_000 });

  // Fill credentials and log in.
  await page.getByLabel('Username').fill('test@example.com');
  await page.getByLabel('Password').fill('password');
  await page.getByRole('button', { name: 'Log in' }).click();
  await waitForAppReady(page);

  // After login we should land on the chat workspace.
  // The heading may be "Chat" (landing) or "Chat Session" (if auto-selected).
  const chatHeading = page.getByRole('heading', { level: 1 }).first();
  await expect(chatHeading).toBeVisible({ timeout: 20_000 });

  // Verify the message input is accessible (proves chat workspace loaded).
  const input = page.getByTestId('chat-input');
  await expect(input).toBeVisible();

  // Verify the new session button is available.
  await expect(page.locator('button[title="New chat session"]')).toBeVisible();
});
