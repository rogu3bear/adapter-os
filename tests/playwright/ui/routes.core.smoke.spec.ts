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
  { path: '/chat', heading: 'Chat' },
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
  await expect(
    page.getByRole('heading', { name: 'Chat', level: 1, exact: true })
  ).toBeVisible();
  await page.getByRole('button', { name: 'New Session' }).click();
  await expect(
    page.getByRole('heading', { name: 'Chat Session', level: 1, exact: true })
  ).toBeVisible();
  await expect(page).toHaveURL(/\/chat\/.+/);
});
