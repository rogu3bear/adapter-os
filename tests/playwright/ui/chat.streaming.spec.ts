import { test, expect } from '@playwright/test';
import { ensureLoggedIn, waitForAppReady } from './utils';

test('chat streaming stub renders assistant response', async ({ page }) => {
  await page.route('**/v1/infer/stream', async (route) => {
    const body = [
      'event: aos.run_envelope',
      'data: {"run_id":"trace-fixture"}',
      '',
      'data: {"event":"Token","text":"Hello from stub"}',
      '',
      'data: {"event":"Done","total_tokens":8,"trace_id":"trace-fixture"}',
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
  await page.getByLabel('Chat message input').fill('hello');
  await page.getByRole('button', { name: 'Send message' }).click();
  await expect(
    page.getByLabel('Chat messages').getByText('Hello from stub', { exact: true })
  ).toBeVisible();
  await expect(page.getByRole('link', { name: 'Run' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Receipt' })).toBeVisible();
});
