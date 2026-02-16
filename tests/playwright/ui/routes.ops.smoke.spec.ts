import { test } from '@playwright/test';
import { runRouteCheck, seeded, type RouteCheck } from './utils';

const opsRoutes: RouteCheck[] = [
  { path: '/admin', heading: 'Administration' },
  { path: '/audit', heading: 'Audit Log' },
  { path: '/runs', heading: 'Flight Recorder' },
  { path: `/runs/${seeded.runId}`, heading: 'Run Detail', headingLevel: 2 },
  { path: '/flight-recorder', heading: 'Flight Recorder' },
  { path: `/flight-recorder/${seeded.runId}`, heading: 'Run Detail', headingLevel: 2 },
  { path: '/diff', heading: 'Run Diff' },
  { path: '/workers', testId: 'workers-page-heading' },
  {
    path: `/workers/${seeded.workerId}`,
    testIdsAny: ['worker-detail-heading', 'worker-detail-error-state'],
  },
  {
    path: '/workers/worker-missing',
    testId: 'worker-detail-error-state',
  },
  { path: '/monitoring', heading: 'Monitoring' },
  { path: '/errors', heading: 'Incidents' },
  { path: '/routing', heading: 'Routing Debug' },
  { path: '/reviews', heading: 'Reviews' },
  { path: '/agents', heading: 'Agent Orchestration' },
  { path: '/files', heading: 'Files' },
  { path: '/welcome', heading: 'Welcome' },
  { path: '/safe', testId: 'safe-page' },
  { path: '/style-audit', testId: 'style-audit-heading' },
];

for (const route of opsRoutes) {
  test(`route smoke coverage (ops): ${route.path}`, { tag: ['@smoke'] }, async ({ page }) => {
    test.setTimeout(90_000);
    await runRouteCheck(page, route);
  });
}
