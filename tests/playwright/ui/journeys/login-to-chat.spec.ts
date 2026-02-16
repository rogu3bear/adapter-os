/**
 * Journey: Login -> land on /chat workspace.
 *
 * Starts unauthenticated, attempts a quick UI login, falls back to API login,
 * and verifies we land on either chat or the dashboard fallback.
 */

import { test, expect } from '@playwright/test';
import { gotoChatEntryAndResolve } from '../utils';

// Clear storageState so this journey always exercises login/bootstrap behavior.
test.use({ storageState: { cookies: [], origins: [] } });

test.setTimeout(240_000);

test('login then land on chat workspace', { tag: ['@flow'] }, async ({ page }) => {
  const contract = await gotoChatEntryAndResolve(page, {
    mode: 'ui-then-api',
    requireUiAttempt: true,
    maxUiAttempts: 2,
    postAuthPath: '/chat',
    expectedPostAuthPath: /\/(chat|dashboard)(\/|$)/,
    timeoutMs: 60_000,
  });

  const path = new URL(page.url()).pathname;
  if (/^\/chat(\/|$)/.test(path)) {
    if (contract.state === 'unavailable') {
      await expect(page.getByTestId('chat-unavailable-state')).toBeVisible({ timeout: 20_000 });
      await expect(page.getByTestId('chat-unavailable-reason')).toBeVisible({ timeout: 20_000 });
      return;
    }
    if (contract.state === 'empty') {
      await expect(page.getByTestId(contract.anchor)).toBeVisible({ timeout: 20_000 });
      return;
    }
    await expect(page.getByTestId('chat-input')).toBeVisible({ timeout: 20_000 });
    return;
  }

  await expect(page.getByRole('link', { name: 'Dashboard' })).toBeVisible();
  expect(path).toMatch(/^\/(?:dashboard)?$/);
});
