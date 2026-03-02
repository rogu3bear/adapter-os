import { test, expect } from '@playwright/test';
import { gotoAndBootstrap, resolveChatEntryState, seeded } from './utils';

test('runs list and detail', { tag: ['@smoke', '@detail'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/runs', { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Flight Recorder', level: 1, exact: true })
  ).toBeVisible();
  const runLabel =
    seeded.runId.length > 12 ? `${seeded.runId.slice(0, 12)}...` : seeded.runId;
  const runButton = page.getByRole('button', { name: new RegExp(`^${runLabel}`) });
  const runLink = page.getByRole('link', { name: new RegExp(`^${runLabel}`) });
  await expect
    .poll(
      async () =>
        (await runButton.isVisible().catch(() => false)) ||
        (await runLink.isVisible().catch(() => false)),
      { timeout: 10_000 }
    )
    .toBeTruthy();

  await gotoAndBootstrap(page, `/runs/${seeded.runId}`, { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Run Detail', level: 2, exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Run Summary', level: 3, exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Provenance', level: 3, exact: true })
  ).toBeVisible();

  const tabNav = page.getByRole('navigation').filter({ hasText: 'Overview' });
  await tabNav.getByRole('button', { name: 'Trace', exact: true }).click();
  await expect(
    page.getByText(
      'Full inference trace with timeline visualization, latency breakdown, and token-level routing decisions.'
    )
  ).toBeVisible();

  await tabNav.getByRole('button', { name: 'Routing', exact: true }).click();
  await expect(
    page.getByText(
      'K-sparse routing decisions showing which adapters were selected and their gate values.'
    )
  ).toBeVisible();
  await tabNav.getByRole('button', { name: 'Receipt', exact: true }).click();
  await expect(page.getByText('Receipts & Hashes')).toBeVisible();
});

test('primary flow: chat to run detail', { tag: ['@flow'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/chat', { mode: 'ui-only' });
  const chatState = await resolveChatEntryState(page);
  if (chatState === 'active') {
    await expect(page.getByTestId('chat-header')).toBeVisible();
  } else if (chatState === 'empty') {
    await expect(
      page.getByRole('button', { name: /New (Chat|Session)/, exact: false }).first()
    ).toBeVisible();
  } else {
    await expect(page.getByText('Chat unavailable', { exact: false }).first()).toBeVisible();
  }

  await gotoAndBootstrap(page, `/runs/${seeded.runId}`, { mode: 'ui-only' });
  await expect(
    page.getByRole('heading', { name: 'Run Detail', level: 2, exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Run Summary', level: 3, exact: true })
  ).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'Provenance', level: 3, exact: true })
  ).toBeVisible();
});
