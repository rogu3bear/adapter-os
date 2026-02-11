import { test, expect } from '@playwright/test';
import {
  ensureLoggedIn,
  runRouteCheck,
  seeded,
  waitForAppReady,
  type RouteCheck,
} from './utils';

const coreRoutes: RouteCheck[] = [
  { path: '/login', heading: 'Login' },
  { path: '/', heading: 'Dashboard' },
  { path: '/dashboard', heading: 'Dashboard' },
  { path: '/adapters', heading: 'Adapters' },
  { path: `/adapters/${seeded.adapterId}`, heading: 'Adapter Details' },
  // /chat uses an sr-only H1; assert on a visible, chat-specific surface instead.
  { path: '/chat', text: 'Sessions' },
  { path: '/system', heading: 'Infrastructure' },
  { path: '/settings', heading: 'Settings' },
  { path: '/user', heading: 'Settings' },
  { path: '/models', heading: 'Models' },
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
  await page.goto('/chat', { waitUntil: 'domcontentloaded' });
  await waitForAppReady(page);
  await ensureLoggedIn(page);
  await expect(page.getByText('Sessions', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'New Chat' }).click();
  await expect(page).toHaveURL(/\/chat\/.+/);
  // Headings in the chat page use custom classes, not always semantic role=heading.
  // Assert on text presence to keep this smoke check resilient.
  await expect(page.getByText('Chat Session', { exact: true })).toBeVisible();
});
