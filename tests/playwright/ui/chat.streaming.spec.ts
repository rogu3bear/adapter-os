import { test, expect } from '@playwright/test';
import { ensureLoggedIn, waitForAppReady } from './utils';

test('chat streaming stub renders assistant response', { tag: ['@flow'] }, async ({ page }) => {
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

  await page.goto('/chat', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await page.getByRole('button', { name: 'New Session' }).click();
  await expect(
    page.getByRole('heading', { name: 'Chat Session', level: 1, exact: true })
  ).toBeVisible();
  const input = page.getByPlaceholder('Type your message...');
  await expect(input).toBeVisible();
  await input.fill('hello');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await expect(
    page.getByLabel('Chat messages').getByText('Hello from stub', { exact: true })
  ).toBeVisible();
  await expect(page.getByRole('link', { name: 'Run', exact: true })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Receipt', exact: true })).toBeVisible();
});
