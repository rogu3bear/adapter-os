import { test } from '@playwright/test';
import { runRouteCheck, seeded, type RouteCheck } from './utils';

const dataRoutes: RouteCheck[] = [
  { path: '/stacks', heading: 'Runtime Stacks' },
  { path: `/stacks/${seeded.stackId}`, heading: 'Stack Details' },
  { path: '/collections', heading: 'Collections' },
  { path: `/collections/${seeded.collectionId}`, heading: 'Collection Details' },
  { path: '/documents', heading: 'Documents' },
  { path: `/documents/${seeded.documentId}`, heading: 'Document Details' },
  { path: '/datasets', heading: 'Datasets' },
  { path: `/datasets/${seeded.datasetId}`, heading: 'Test Dataset' },
  { path: '/repositories', heading: 'Repositories' },
  { path: `/repositories/${seeded.repoId}`, heading: 'Repository Details' },
];

for (const route of dataRoutes) {
  test(`route smoke coverage (data): ${route.path}`, { tag: ['@smoke'] }, async ({ page }) => {
    test.setTimeout(90_000);
    await runRouteCheck(page, route);
  });
}
