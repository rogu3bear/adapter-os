import { test } from '@playwright/test';
import { runRouteCheck, seeded, type RouteCheck } from './utils';

const opsRoutes: RouteCheck[] = [
  { path: '/admin', heading: 'Administration' },
  { path: '/audit', heading: 'Audit Log' },
  { path: '/runs', heading: 'System Execution Records' },
  { path: `/runs/${seeded.runId}`, heading: 'Execution Record Detail', headingLevel: 2 },
  { path: '/flight-recorder', heading: 'System Execution Records' },
  { path: `/flight-recorder/${seeded.runId}`, heading: 'Execution Record Detail', headingLevel: 2 },
  { path: '/workers', testId: 'workers-page-heading' },
  {
    path: `/workers/${seeded.workerId}`,
    testIdsAny: ['worker-detail-heading', 'worker-detail-error-state'],
  },
  {
    path: '/workers/worker-missing',
    testId: 'worker-detail-error-state',
  },
  { path: '/welcome', heading: 'Welcome' },
  { path: '/safe', testId: 'safe-page' },
];

for (const route of opsRoutes) {
  test(`route smoke coverage (ops): ${route.path}`, { tag: ['@smoke'] }, async ({ page }) => {
    test.setTimeout(90_000);
    await runRouteCheck(page, route);
  });
}
