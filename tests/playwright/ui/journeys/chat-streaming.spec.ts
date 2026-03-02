/**
 * Journey: Select a session → send message → verify streaming response.
 *
 * Uses the shared SSE helper to stub the inference stream and verifies
 * the assistant message renders with trace links.
 */

import { test, expect } from '@playwright/test';
import {
  ensureActiveChatSession,
  gotoChatEntryAndResolve,
  waitForAppReady,
  gotoWithRetry,
} from '../utils';
import { buildStream, stubInferStream, stubSystemStatus } from '../helpers/sse';

test('new session: send message and verify streaming', { tag: ['@flow'] }, async ({ page }) => {
  test.setTimeout(90_000);

  await stubSystemStatus(page, { ready: true });
  await stubInferStream(
    page,
    buildStream({
      runId: 'trace-fixture',
      tokens: ['Hello', ' from', ' streaming', ' test'],
      totalTokens: 20,
      latencyMs: 80,
    })
  );

  const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
  if (entry.state === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
    await expect(page.getByTestId('chat-unavailable-reason')).toBeVisible();
    return;
  }
  await ensureActiveChatSession(page);

  // Send a message.
  const input = page.getByTestId('chat-input');
  await expect(input).toBeVisible();
  await input.fill('Tell me about adapters');
  await page.getByTestId('chat-send').click();

  // Verify the streamed response renders.
  const messages = page.getByLabel('Chat messages');
  await expect(messages.getByText('Hello from streaming test')).toBeVisible();

  // Verify trace links appear.
  await expect(page.getByTestId('chat-run-link')).toBeVisible();
  await expect(page.getByTestId('chat-receipt-link')).toBeVisible();
});

test('chat session state survives navigation', { tag: ['@flow'] }, async ({ page }) => {
  test.setTimeout(120_000);

  await stubSystemStatus(page, { ready: true });
  await stubInferStream(
    page,
    buildStream({
      runId: 'trace-nav-persist',
      traceId: 'trace-nav-persist',
      tokens: ['Persisted', ' across', ' navigation'],
      totalTokens: 12,
      latencyMs: 60,
    })
  );

  const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
  if (entry.state === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
    return;
  }
  await ensureActiveChatSession(page);

  // Send a message and verify the streamed response renders.
  const input = page.getByTestId('chat-input');
  await expect(input).toBeVisible();
  await input.fill('State persistence check');
  await page.getByTestId('chat-send').click();

  const messages = page.getByLabel('Chat messages');
  await expect(messages.getByText('Persisted across navigation')).toBeVisible();

  // Navigate away: /adapters → /system → back to /chat.
  await gotoWithRetry(page, '/adapters');
  await waitForAppReady(page);
  await gotoWithRetry(page, '/system');
  await waitForAppReady(page);
  await gotoWithRetry(page, '/chat');
  await waitForAppReady(page);

  // The original session message must still be visible — no unintended reset.
  const messagesAfterNav = page.getByLabel('Chat messages');
  await expect(messagesAfterNav.getByText('Persisted across navigation')).toBeVisible({ timeout: 15_000 });
});
