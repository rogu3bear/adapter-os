import { test, expect, type Page } from '@playwright/test';
import type { DemoContext, DemoOperationMeta } from '../types';
import { createDemoContext, pacingFromEnv } from '../harness';
import { installMocks } from '../mocks';
import {
  ensureActiveChatSession,
  gotoAndBootstrap,
  resolveChatEntryState,
  waitForAppReady,
} from '../../ui/utils';

export const meta: DemoOperationMeta = {
  id: 'replay-signed-verify',
  title: 'Replay And Signed Log Verification',
  mocks: ['system-ready', 'infer-stream', 'trace-detail', 'replay'],
  tags: ['demo', 'replay', 'proof'],
};

async function dismissStatusCenter(page: Page): Promise<void> {
  const statusCenter = page.getByRole('dialog', { name: 'Status Center' });
  if (!(await statusCenter.isVisible().catch(() => false))) {
    return;
  }
  await page.keyboard.press('Escape').catch(() => {});
  if (await statusCenter.isVisible().catch(() => false)) {
    const closeButton = statusCenter.getByRole('button', { name: 'Close' }).first();
    if (await closeButton.isVisible().catch(() => false)) {
      await closeButton.scrollIntoViewIfNeeded().catch(() => {});
      await closeButton.click({ timeout: 1_500 }).catch(async () => {
        await closeButton.click({ force: true, timeout: 1_000 }).catch(() => {});
      });
    }
  }
  if (await statusCenter.isVisible().catch(() => false)) {
    await page.keyboard.press('Escape').catch(() => {});
  }
  await statusCenter.waitFor({ state: 'hidden', timeout: 2_500 }).catch(() => {});
}

export async function run(demo: DemoContext): Promise<void> {
  const { page } = demo;
  const messages = page.getByRole('log', { name: 'Chat messages' });

  await demo.narrate('Generate a run, then open receipt, replay, and verify signed logs.');
  const input = page.getByTestId('chat-input');
  await input.click();
  await input.fill('Generate a replayable answer');
  await page.keyboard.press('Enter');
  await expect(messages.getByText('Hello from AdapterOS demo.')).toBeVisible();
  await expect(page.getByTestId('chat-trace-links')).toBeVisible({ timeout: 60_000 });

  await page.getByTestId('chat-receipt-link').last().click();
  await page.waitForURL(/\/runs\/[^/?#]+/, { timeout: 60_000 });
  await waitForAppReady(page);
  await dismissStatusCenter(page);

  const switchToFullProfile = page.locator('button[title*="switch to Full"]').first();
  if (await switchToFullProfile.isVisible().catch(() => false)) {
    await switchToFullProfile.click();
  }

  const tabNav = page.getByRole('navigation').filter({ hasText: 'Overview' });
  const replayTab = tabNav.getByRole('button', { name: /^(System Execution Records|Replay)$/ });
  await expect(replayTab).toBeVisible({ timeout: 30_000 });
  await replayTab.click();

  const replayRequest = page.waitForResponse(
    (resp) =>
      resp.request().method() === 'POST' &&
      /\/v1\/replay\/sessions\/[^/]+\/execute/.test(new URL(resp.url()).pathname),
    { timeout: 30_000 }
  );
  const replayActionButton = page
    .getByRole('button', { name: /^(Replay Exactly|Execute Replay)$/ })
    .first();
  await expect(replayActionButton).toBeVisible({ timeout: 30_000 });
  await replayActionButton.click();

  const replayDialog = page.getByRole('dialog').filter({ hasText: /Replay/i }).first();
  if (await replayDialog.isVisible().catch(() => false)) {
    const dialogReplayButton = replayDialog
      .getByRole('button', { name: /^(Replay|Execute)$/ })
      .first();
    await expect(dialogReplayButton).toBeVisible({ timeout: 10_000 });
    await dialogReplayButton.click();
  }
  const replayResponse = await replayRequest;
  expect(replayResponse.ok()).toBeTruthy();

  const receiptTab = tabNav.getByRole('button', { name: /^(Receipt|Signed System Logs)$/i });
  if (await receiptTab.isVisible().catch(() => false)) {
    await receiptTab.click();
  }
  await expect(
    page
      .getByText(/^(Signed Logs & Fingerprints|Signed Log Summary|Verify Signed Log Bundle)$/)
      .first()
  ).toBeVisible({ timeout: 30_000 });

  const verify = page.getByRole('button', { name: 'Verify on server' });
  if (await verify.isVisible().catch(() => false)) {
    await verify.click();
  }
  await expect(page.getByText(/Verified|Signed log verified/)).toBeVisible({
    timeout: 60_000,
  });
}

test(meta.id, { tag: ['@demo'] }, async ({ page }) => {
  await installMocks(page, meta.mocks);
  const demo = createDemoContext(page, pacingFromEnv());

  await gotoAndBootstrap(page, '/chat', { mode: 'ui-only' });
  await waitForAppReady(page);

  const chatEntryState = await resolveChatEntryState(page);
  test.skip(chatEntryState === 'unavailable', 'Skipping replay flow because inference is unavailable.');
  await ensureActiveChatSession(page);

  await run(demo);
});
