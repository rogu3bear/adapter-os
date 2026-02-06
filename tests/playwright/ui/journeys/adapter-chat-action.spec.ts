/**
 * Journey: Adapters list → click Chat action → verify adapter is pinned in chat.
 *
 * Tests the cross-page flow:
 * 1. Navigate to /adapters list.
 * 2. Click the "Chat" button on a seeded adapter row.
 * 3. Verify navigation to /chat/{session}?adapter={id}.
 * 4. Verify the adapter appears pinned in the chat workspace.
 */

import { test, expect } from '@playwright/test';
import { ensureLoggedIn, seeded, waitForAppReady } from '../utils';
import { stubSystemStatus } from '../helpers/sse';
import { useConsoleCatcher } from '../helpers/console-catcher';

useConsoleCatcher(test);

test('from adapters list, click Chat action, adapter is pinned in chat', { tag: ['@flow'] }, async ({ page }) => {
  await stubSystemStatus(page, { ready: true });

  // Navigate to adapters list.
  await page.goto('/adapters', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);

  await expect(
    page.getByRole('heading', { name: 'Adapters', level: 1, exact: true })
  ).toBeVisible();

  // Find the seeded adapter row and click its Chat action button.
  const adapterRow = page.getByText(seeded.adapterName).locator('..');
  const chatButton = adapterRow.locator('..').getByRole('button', { name: 'Chat', exact: true });
  await expect(chatButton).toBeVisible();
  await chatButton.click();

  // Should navigate to chat with the adapter query param.
  await waitForAppReady(page);
  await ensureLoggedIn(page);

  // Verify we're on a chat session page.
  await expect(
    page.getByRole('heading', { name: 'Chat Session', level: 1, exact: true })
  ).toBeVisible({ timeout: 20_000 });

  // Verify the URL contains the adapter query param.
  expect(page.url()).toContain(`adapter=${seeded.adapterId}`);

  // Verify the pending badge shows (adapter was auto-pinned).
  const pendingBadge = page.locator('[aria-label="Adapter changes pending confirmation"]');
  await expect(pendingBadge).toBeVisible();

  // Verify the message input is ready.
  await expect(page.getByTestId('chat-input')).toBeVisible();
});
