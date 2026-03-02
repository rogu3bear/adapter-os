import { test, expect } from '@playwright/test';
import {
  ensureActiveChatSession,
  gotoAndBootstrap,
  resolveChatEntryState,
  runRouteCheck,
  seeded,
  type RouteCheck,
} from './utils';

const coreRoutes: RouteCheck[] = [
  { path: '/login', heading: 'Login' },
  { path: '/', heading: 'Dashboard' },
  { path: '/dashboard', heading: 'Dashboard' },
  { path: '/adapters', text: /New Adapter|No adapters found/i },
  { path: `/adapters/${seeded.adapterId}`, heading: 'Adapter Details' },
  // /chat uses an sr-only H1; assert on a visible, chat-specific surface instead.
  { path: '/chat', testId: 'chat-status-badge', text: /Sessions|Chat unavailable/i },
  { path: '/system', heading: 'Infrastructure' },
  { path: '/settings', heading: 'Settings' },
  { path: '/user', heading: 'Settings' },
  { path: '/models', text: /Import Model|Base model status requires admin permissions/i },
  { path: '/policies', heading: 'Policy Packs' },
  { path: '/training', heading: 'Training Jobs' },
];

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
  const state = await resolveChatEntryState(page);
  if (state === 'unavailable') {
    await expect(page.getByTestId('chat-unavailable-state')).toBeVisible();
    return;
  }
  await ensureActiveChatSession(page);
  await expect(page).toHaveURL(/\/chat\/.+/);
  await expect(page.getByTestId('chat-header')).toBeVisible();
});
