/**
 * Journey: Pin adapter → send message → verify pending clears on SSE update.
 *
 * Tests the adapter pinning lifecycle:
 * 1. Navigate to chat and create a new session.
 * 2. Pin an adapter via the ?adapter= query param (simulating the flow).
 * 3. Verify the "Pending next message" badge appears.
 * 4. Send a message with an SSE stream that includes AdapterStateUpdate.
 * 5. Verify the pending badge disappears after the update.
 */

import { test, expect } from '@playwright/test';
import { gotoAndBootstrap, seeded } from '../utils';
import {
  buildStream,
  stubChatSessionTags,
  stubInferStream,
  stubSystemStatus,
} from '../helpers/sse';
import { useConsoleCatcher } from '../helpers/console-catcher';

useConsoleCatcher(test);

test('pin adapter, send message, pending clears on SSE update', { tag: ['@flow'] }, async ({ page }) => {
  await stubSystemStatus(page, { ready: true });
  await stubChatSessionTags(page);

  // Build a stream that includes an AdapterStateUpdate event.
  await stubInferStream(
    page,
    buildStream({
      runId: 'trace-fixture',
      text: 'Response with adapter active',
      adapters: [{ adapter_id: seeded.adapterId, uses_per_minute: 5, is_active: true }],
      totalTokens: 12,
      latencyMs: 60,
    })
  );

  // Navigate directly to a new session with the adapter pinned via query param.
  // This simulates what happens when clicking "Chat" on the adapters page.
  await gotoAndBootstrap(page, `/chat/ses-pin-test?adapter=${seeded.adapterId}`, {
    mode: 'ui-then-api',
  });

  await expect(page.getByTestId('chat-header')).toBeVisible();

  // The pending badge should be visible after pinning.
  const pendingBadge = page.locator('[aria-label="Adapter changes pending confirmation"]');
  await expect(pendingBadge).toBeVisible();
  await expect(pendingBadge).toHaveText('Pending next message');

  // Send a message — the SSE stream includes AdapterStateUpdate.
  const input = page.getByTestId('chat-input');
  await expect(input).toBeVisible();
  await input.fill('test with pinned adapter');
  await page.getByTestId('chat-send').click();

  // Verify the response rendered.
  const messages = page.getByLabel('Chat messages');
  await expect(messages.getByText('Response with adapter active')).toBeVisible();

  // The pending badge should be gone after AdapterStateUpdate arrived.
  await expect(pendingBadge).not.toBeVisible();
});
