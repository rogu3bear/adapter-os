import { expect, test, type Page } from '@playwright/test';
import { disableAnimations, gotoAndBootstrap, seeded } from './utils';

type AdapterOpsMockState = {
  timelineRequests: number;
};

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

async function installAdapterOpsMocks(page: Page): Promise<AdapterOpsMockState> {
  const state: AdapterOpsMockState = { timelineRequests: 0 };

  const adapter = {
    schema_version: 'v1',
    id: seeded.adapterId,
    adapter_id: seeded.adapterId,
    name: seeded.adapterName,
    hash_b3: 'b3-test-hash',
    rank: 8,
    tier: 'standard',
    languages: ['rust'],
    framework: 'leptos',
    category: 'code',
    scope: 'repo',
    repo_id: seeded.repoId,
    created_at: '2026-02-28T00:00:00Z',
    updated_at: '2026-02-28T00:00:00Z',
    version: 'v1',
    lifecycle_state: 'active',
    runtime_state: 'hot',
    pinned: false,
  };

  const versions = [
    {
      id: 'ver-candidate',
      repo_id: seeded.repoId,
      version: 'v2',
      branch: 'main',
      release_state: 'candidate',
      adapter_trust_state: 'allowed',
      serveable: true,
      serveable_reason: 'Reviewed and ready',
      created_at: '2026-02-28T01:00:00Z',
      display_name: 'v2-candidate',
    },
    {
      id: 'ver-promoted',
      repo_id: seeded.repoId,
      version: 'v1',
      branch: 'main',
      release_state: 'promoted',
      adapter_trust_state: 'allowed',
      serveable: true,
      serveable_reason: 'Live in production',
      created_at: '2026-02-28T00:30:00Z',
      display_name: 'v1-live',
    },
  ];

  await page.route('**/v1/adapter-repositories/**/versions/checkout', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: '{}',
    });
  });

  await page.route('**/v1/adapter-versions/**/promote', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: '{}',
    });
  });

  await page.route('**/v1/repos/**/timeline', async (route) => {
    state.timelineRequests += 1;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          id: `evt-${state.timelineRequests}`,
          timestamp: `2026-02-28T02:0${Math.min(state.timelineRequests, 9)}:00Z`,
          event_type: 'state_change:promoted',
          description: `draft -> promoted (event ${state.timelineRequests})`,
        },
      ]),
    });
  });

  await page.route('**/v1/adapter-repositories/**/versions', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(versions),
    });
  });

  await page.route(`**/v1/adapters/${seeded.adapterId}`, async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(adapter),
    });
  });

  await page.route('**/v1/adapters', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([adapter]),
    });
  });

  return state;
}

async function openCommandDeck(page: Page): Promise<void> {
  const input = page
    .locator('.command-palette-panel input[aria-label*="Search pages"]')
    .first();

  if (await input.isVisible().catch(() => false)) {
    return;
  }

  for (const shortcut of ['Control+k', 'Meta+k']) {
    await page.keyboard.press(shortcut).catch(() => {});
    if (await input.isVisible().catch(() => false)) {
      return;
    }
  }

  throw new Error('Command Deck input did not open');
}

async function runCommandFromDeck(page: Page, query: string, title: string): Promise<void> {
  await openCommandDeck(page);
  const panel = page.locator('.command-palette-panel').first();
  const input = page
    .locator('.command-palette-panel input[aria-label*="Search pages"]')
    .first();
  await input.fill(query);
  const resultTitle = page
    .locator('.command-palette-panel .search-result-title')
    .filter({ hasText: new RegExp(`^${escapeRegExp(title)}$`) })
    .first();
  await expect(resultTitle).toBeVisible({ timeout: 10_000 });
  await page.keyboard.press('Enter');
  await expect(panel).toBeHidden({ timeout: 10_000 });
}

test(
  'command deck deep-links preserve selected adapter intent for promote/checkout/feed-dataset',
  { tag: ['@flow'] },
  async ({ page }) => {
    test.setTimeout(90_000);
    await disableAnimations(page);
    await gotoAndBootstrap(page, '/dashboard', { mode: 'ui-only' });
    await installAdapterOpsMocks(page);

    await page.goto(`/update-center?adapter_id=${seeded.adapterId}`, {
      waitUntil: 'domcontentloaded',
    });
    await expect(
      page.getByRole('heading', { name: 'Update Center', level: 1, exact: true })
    ).toBeVisible();
    await expect(page.getByText('Repository Command Timeline', { exact: true })).toBeVisible();

    await runCommandFromDeck(page, 'run promote', 'Run Promote');
    await expect(page).toHaveURL(
      new RegExp(
        `/update-center\\?(?:[^#]*&)?adapter_id=${seeded.adapterId}.*command=run-promote|/update-center\\?(?:[^#]*&)?command=run-promote.*adapter_id=${seeded.adapterId}`
      )
    );
    await expect(page.getByText('Intent received: Run Promote.')).toBeVisible();

    await runCommandFromDeck(page, 'run checkout', 'Run Checkout');
    await expect(page).toHaveURL(
      new RegExp(
        `/update-center\\?(?:[^#]*&)?adapter_id=${seeded.adapterId}.*command=run-checkout|/update-center\\?(?:[^#]*&)?command=run-checkout.*adapter_id=${seeded.adapterId}`
      )
    );
    await expect(page.getByText('Intent received: Run Checkout.')).toBeVisible();

    await runCommandFromDeck(page, 'feed dataset', 'Feed Dataset');
    await expect(page).toHaveURL(
      new RegExp(`/training\\?.*open_wizard=1.*repo_id=${seeded.repoId}.*return_to=`)
    );
  }
);

test(
  'repository command timeline refetches after run promote',
  { tag: ['@flow'] },
  async ({ page }) => {
    test.setTimeout(90_000);
    await disableAnimations(page);
    await gotoAndBootstrap(page, '/dashboard', { mode: 'ui-only' });
    const mockState = await installAdapterOpsMocks(page);

    await page.goto(`/update-center?adapter_id=${seeded.adapterId}`, {
      waitUntil: 'domcontentloaded',
    });
    await expect(
      page.getByRole('heading', { name: 'Update Center', level: 1, exact: true })
    ).toBeVisible();
    await expect(page.getByText('Repository Command Timeline', { exact: true })).toBeVisible();
    await expect
      .poll(() => mockState.timelineRequests, { timeout: 10_000 })
      .toBeGreaterThan(0);

    const timelineBefore = mockState.timelineRequests;
    await page.getByRole('button', { name: /Run Promote/ }).first().click();
    const confirmDialog = page.getByRole('dialog', { name: 'Run Promote to Production' });
    await expect(confirmDialog).toBeVisible();
    await confirmDialog.getByRole('button', { name: 'Run Promote', exact: true }).click();

    await expect
      .poll(() => mockState.timelineRequests, { timeout: 10_000 })
      .toBeGreaterThan(timelineBefore);
    await expect(page.getByText(/event [2-9]/)).toBeVisible();
  }
);
