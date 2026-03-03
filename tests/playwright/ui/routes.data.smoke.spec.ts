import { test } from '@playwright/test';
import { runRouteCheck, seeded, type RouteCheck } from './utils';

const dataRoutes: RouteCheck[] = [
  { path: '/documents', heading: 'Files' },
  { path: `/documents/${seeded.documentId}`, heading: 'File Details' },
];

for (const route of dataRoutes) {
  test(`route smoke coverage (data): ${route.path}`, { tag: ['@smoke'] }, async ({ page }) => {
    test.setTimeout(90_000);
    await runRouteCheck(page, route);
  });
}
