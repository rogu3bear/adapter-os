import { test, expect } from '@playwright/test';
import {
  ensureActiveChatSession,
  gotoChatEntryAndResolve,
} from './utils';

test('chat streaming stub renders assistant response', { tag: ['@flow'] }, async ({ page }) => {
  test.setTimeout(90_000);

  await page.route('**/v1/system/status', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        schema_version: '1.0',
        timestamp: new Date().toISOString(),
        integrity: {
          mode: 'best_effort',
          is_federated: false,
          strict_mode: false,
          pf_deny_ok: true,
          drift: { level: 'ok' },
        },
        readiness: {
          overall: 'ready',
          checks: {
            db: { status: 'ready' },
            migrations: { status: 'ready' },
            workers: { status: 'ready' },
            models: { status: 'ready' },
          },
        },
        inference_ready: 'true',
        inference_blockers: [],
      }),
    });
  });

  await page.route('**/v1/infer/stream', async (route) => {
    const body = [
      'event: aos.run_envelope',
      'data: {"run_id":"trace-fixture","schema_version":"1.0","workspace_id":"ws-fixture","actor":{"subject":"dev-bypass"},"reasoning_mode":false,"determinism_version":"1.0","created_at":"2025-01-01T00:00:00Z"}',
      '',
      'data: {"event":"Token","text":"Hello from stub"}',
      '',
      'data: {"event":"Done","total_tokens":8,"latency_ms":50}',
      '',
    ].join('\n');
    await route.fulfill({
      status: 200,
      headers: {
        'content-type': 'text/event-stream',
      },
      body,
    });
  });

  const entry = await gotoChatEntryAndResolve(page, { mode: 'ui-only', timeoutMs: 30_000 });
  if (entry.state === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
    await expect(page.getByTestId('chat-unavailable-reason')).toBeVisible();
    return;
  }
  await ensureActiveChatSession(page);
  const input = page.getByTestId('chat-input');
  await expect(input).toBeVisible();
  await input.fill('hello');
  await page.getByTestId('chat-send').click();
  await expect(
    page.getByLabel('Chat messages').getByText('Hello from stub', { exact: true })
  ).toBeVisible();
  await expect(page.getByTestId('chat-run-link')).toBeVisible();
  await expect(page.getByTestId('chat-receipt-link')).toBeVisible();
});
