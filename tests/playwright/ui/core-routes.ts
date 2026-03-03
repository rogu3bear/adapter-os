import { seeded, type RouteCheck } from './utils';

export const coreRoutes: RouteCheck[] = [
  // Stabilization sweep: core smoke enforces route reachability + hydrated shell.
  // Route-specific UI anchors are covered in dedicated page specs and may vary by profile.
  { path: '/login' },
  { path: '/' },
  { path: '/dashboard' },
  { path: '/adapters' },
  { path: `/adapters/${seeded.adapterId}` },
  { path: '/chat' },
  { path: '/system' },
  { path: '/models' },
  { path: '/policies' },
  { path: '/training' },
];
