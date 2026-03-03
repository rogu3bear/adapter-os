/**
 * Journey: Deep-linked chat session confirmation states.
 *
 * Covers:
 * 1. Pending -> confirmed (backend probe success)
 * 2. Pending -> not found (backend probe 404)
 * 3. Pending -> transient -> retry -> confirmed
 */

import { test, expect } from '@playwright/test';
import { gotoAndBootstrap } from '../utils';
import { stubChatSessionTags, stubSystemStatus } from '../helpers/sse';

const ISO_TS = '2026-01-01T00:00:00Z';

function uniqueSessionId(prefix: string) {
  return `${prefix}-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

function backendSession(id: string) {
  return {
    id,
    name: `session-${id}`,
    title: 'New Conversation',
    status: 'active',
    created_at: ISO_TS,
    updated_at: ISO_TS,
  };
}

test(
  'deep link: local draft transitions to confirmed when backend session exists',
  { tag: ['@flow'] },
  async ({ page }) => {
    test.setTimeout(90_000);

    const sessionId = uniqueSessionId('ses-confirm-existing');
    let getSessionCalls = 0;

    await stubSystemStatus(page, { ready: true });
    await stubChatSessionTags(page);

    await page.route('**/v1/chat/sessions?limit=50', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    await page.route(`**/v1/chat/sessions/${sessionId}`, async (route) => {
      getSessionCalls += 1;
      await new Promise((resolve) => setTimeout(resolve, 250));
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(backendSession(sessionId)),
      });
    });

    await page.route(`**/v1/chat/sessions/${sessionId}/messages`, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    await gotoAndBootstrap(page, `/chat/${sessionId}`, { mode: 'ui-only' });
    await expect(page.getByTestId('chat-header')).toBeVisible({ timeout: 20_000 });

    await expect(page.getByTestId('chat-session-state-pending')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('chat-session-state-pending')).not.toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByTestId('chat-session-state-not-found')).not.toBeVisible();
    await expect(page.getByTestId('chat-session-state-transient')).not.toBeVisible();

    expect(getSessionCalls).toBeGreaterThan(0);
  }
);

test(
  'deep link: local draft transitions to not-found when backend session is missing',
  { tag: ['@flow'] },
  async ({ page }) => {
    test.setTimeout(90_000);

    const sessionId = uniqueSessionId('ses-missing-404');

    await stubSystemStatus(page, { ready: true });
    await stubChatSessionTags(page);

    await page.route('**/v1/chat/sessions?limit=50', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    await page.route(`**/v1/chat/sessions/${sessionId}`, async (route) => {
      await route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({
          message: 'Session not found',
          code: 'NOT_FOUND',
        }),
      });
    });

    await gotoAndBootstrap(page, `/chat/${sessionId}`, { mode: 'ui-only' });
    await expect(page.getByTestId('chat-header')).toBeVisible({ timeout: 20_000 });

    await expect(page.getByTestId('chat-session-state-pending')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('chat-session-state-not-found')).toBeVisible({
      timeout: 20_000,
    });
    await expect(page.getByTestId('chat-session-confirm-retry')).not.toBeVisible();
    await expect(page.getByTestId('chat-session-error-link')).toBeVisible();
  }
);

test(
  'deep link: transient confirmation error exposes retry and recovers',
  { tag: ['@flow'] },
  async ({ page }) => {
    test.setTimeout(120_000);

    const sessionId = uniqueSessionId('ses-transient-retry');
    let getSessionCalls = 0;

    await stubSystemStatus(page, { ready: true });
    await stubChatSessionTags(page);

    await page.route('**/v1/chat/sessions?limit=50', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    await page.route(`**/v1/chat/sessions/${sessionId}`, async (route) => {
      getSessionCalls += 1;
      if (getSessionCalls === 1) {
        await route.fulfill({
          status: 503,
          contentType: 'application/json',
          body: JSON.stringify({
            message: 'temporary upstream outage',
            code: 'SERVER_UNAVAILABLE',
          }),
        });
        return;
      }

      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(backendSession(sessionId)),
      });
    });

    await page.route(`**/v1/chat/sessions/${sessionId}/messages`, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      });
    });

    await gotoAndBootstrap(page, `/chat/${sessionId}`, { mode: 'ui-only' });
    await expect(page.getByTestId('chat-header')).toBeVisible({ timeout: 20_000 });

    await expect(page.getByTestId('chat-session-state-transient')).toBeVisible({
      timeout: 20_000,
    });
    await expect(page.getByTestId('chat-session-confirm-retry')).toBeVisible();

    await page.getByTestId('chat-session-confirm-retry').click();
    await expect(page.getByTestId('chat-session-state-pending')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByTestId('chat-session-state-pending')).not.toBeVisible({
      timeout: 20_000,
    });
    await expect(page.getByTestId('chat-session-state-transient')).not.toBeVisible();
    expect(getSessionCalls).toBeGreaterThanOrEqual(2);
  }
);
