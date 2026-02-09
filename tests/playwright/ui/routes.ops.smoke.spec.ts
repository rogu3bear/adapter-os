import { test } from '@playwright/test';
import { runRouteCheck, seeded, type RouteCheck } from './utils';

const opsRoutes: RouteCheck[] = [
  { path: '/admin', heading: 'Administration' },
  { path: '/audit', heading: 'Audit Log' },
  { path: '/runs', heading: 'Runs' },
  { path: `/runs/${seeded.runId}`, heading: 'Run Detail', headingLevel: 2 },
  { path: '/flight-recorder', heading: 'Runs' },
  { path: `/flight-recorder/${seeded.runId}`, heading: 'Run Detail', headingLevel: 2 },
  { path: '/diff', heading: 'Run Diff' },
  { path: '/workers', heading: 'Workers' },
  { path: `/workers/${seeded.workerId}`, heading: 'Worker Detail' },
  { path: '/workers/worker-missing', text: 'Not found' },
  { path: '/monitoring', heading: 'Metrics' },
  { path: '/errors', heading: 'Incidents' },
  { path: '/routing', heading: 'Routing Debug' },
  { path: '/reviews', heading: 'Human Review' },
  { path: '/agents', heading: 'Agent Orchestration' },
  { path: '/welcome', heading: 'Welcome' },
  { path: '/safe', heading: 'Safety Mode', headingLevel: 3 },
  { path: '/style-audit', heading: 'Style Audit' },
];

for (const route of opsRoutes) {
  test(`route smoke coverage (ops): ${route.path}`, { tag: ['@smoke'] }, async ({ page }) => {
    test.setTimeout(90_000);
    await runRouteCheck(page, route);
  });
}
