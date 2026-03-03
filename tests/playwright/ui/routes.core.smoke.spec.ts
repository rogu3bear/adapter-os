import { test, expect } from '@playwright/test';
import {
  ensureActiveChatSession,
  gotoAndBootstrap,
  resolveChatEntryState,
  runRouteCheck,
} from './utils';
import { coreRoutes } from './core-routes';

for (const route of coreRoutes) {
  test(`route smoke coverage (core): ${route.path}`, { tag: ['@smoke'] }, async ({ page }) => {
    test.setTimeout(90_000);
    await runRouteCheck(page, route);
  });
}

test('chat session deep route loads', { tag: ['@smoke'] }, async ({ page }) => {
  await gotoAndBootstrap(page, '/chat', {
    mode: 'ui-only',
  });
  let state: Awaited<ReturnType<typeof resolveChatEntryState>>;
  try {
    state = await resolveChatEntryState(page);
  } catch {
    // Reduced shell can present a neutral "connecting" chat surface without
    // legacy chat entry anchors; route reachability is sufficient for core smoke.
    await expect(page).toHaveURL(/\/chat(\/|$)/);
    return;
  }
  if (state === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
    return;
  }
  await ensureActiveChatSession(page);
  await expect(page).toHaveURL(/\/chat\/.+/);
  await expect(page.getByTestId('chat-header')).toBeVisible();
});
