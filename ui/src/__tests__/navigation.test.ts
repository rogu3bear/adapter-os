import { describe, expect, it, vi } from 'vitest';

vi.mock('@/config/routes_manifest', () => {
  return {
    PRIMARY_SPINE: [
      '/workflow',
      '/adapters',
      '/training',
      '/router-config',
      '/base-models',
      '/dashboard',
      '/inference',
      '/chat',
      '/documents',
      '/monitoring',
      '/metrics',
      '/routing',
      '/testing',
      '/golden',
      '/replay',
      '/security/policies',
      '/security/audit',
      '/security/compliance',
      '/_dev/routes',
      '/dev/errors',
    ],
  };
});

vi.mock('@/config/routes', () => {
  const routes = [
    {
      path: '/workflow',
      navGroup: 'Build',
      navTitle: 'Onboarding',
      navIcon: () => null,
      navOrder: 0,
      cluster: 'Build',
    },
    {
      path: '/adapters',
      navGroup: 'Build',
      navTitle: 'Adapters',
      navIcon: () => null,
      navOrder: 1,
      cluster: 'Build',
    },
    {
      path: '/training',
      navGroup: 'Build',
      navTitle: 'Training',
      navIcon: () => null,
      navOrder: 2,
      cluster: 'Build',
      requiredRoles: ['admin'],
    },
    {
      path: '/router-config',
      navGroup: 'Build',
      navTitle: 'Routing Config',
      navIcon: () => null,
      navOrder: 3,
      cluster: 'Build',
    },
    {
      path: '/dashboard',
      navGroup: 'Run',
      navTitle: 'Dashboard',
      navIcon: () => null,
      navOrder: 0,
      cluster: 'Run',
    },
    {
      path: '/inference',
      navGroup: 'Run',
      navTitle: 'Inference',
      navIcon: () => null,
      navOrder: 1,
      cluster: 'Run',
    },
    {
      path: '/documents',
      navGroup: 'Run',
      navTitle: 'Documents',
      navIcon: () => null,
      navOrder: 3,
      cluster: 'Run',
    },
    {
      path: '/metrics',
      navGroup: 'Observe',
      navTitle: 'Metrics',
      navIcon: () => null,
      navOrder: 1,
      cluster: 'Observe',
    },
    {
      path: '/routing',
      navGroup: 'Observe',
      navTitle: 'Routing History',
      navIcon: () => null,
      navOrder: 2,
      cluster: 'Observe',
    },
    {
      path: '/telemetry',
      navGroup: 'Observe',
      navTitle: 'Telemetry',
      navIcon: () => null,
      navOrder: 3,
      cluster: 'Observe',
      requiredPermissions: ['audit.view'],
    },
    {
      path: '/testing',
      navGroup: 'Verify',
      navTitle: 'Testing',
      navIcon: () => null,
      navOrder: 0,
      cluster: 'Verify',
    },
    {
      path: '/golden',
      navGroup: 'Verify',
      navTitle: 'Verified Runs',
      navIcon: () => null,
      navOrder: 1,
      cluster: 'Verify',
    },
    {
      path: '/replay',
      navGroup: 'Verify',
      navTitle: 'Replay',
      navIcon: () => null,
      navOrder: 2,
      cluster: 'Verify',
    },
    {
      path: '/security/policies',
      navGroup: 'Verify',
      navTitle: 'Guardrails',
      navIcon: () => null,
      navOrder: 3,
      cluster: 'Verify',
    },
    {
      path: '/security/audit',
      navGroup: 'Verify',
      navTitle: 'Audit Logs',
      navIcon: () => null,
      navOrder: 4,
      cluster: 'Verify',
      requiredPermissions: ['audit.view'],
    },
    {
      path: '/security/compliance',
      navGroup: 'Verify',
      navTitle: 'Compliance',
      navIcon: () => null,
      navOrder: 5,
      cluster: 'Verify',
    },
    {
      path: '/hidden',
      navGroup: 'Build',
      navIcon: () => null,
      navOrder: 99,
      cluster: 'Build',
    },
    {
      path: '/dev/errors',
      navGroup: 'Verify',
      navTitle: 'Error Inspector',
      navIcon: () => null,
      navOrder: 7,
      cluster: 'Verify',
    },
    {
      path: '/_dev/routes',
      navGroup: 'Verify',
      navTitle: 'Routes Manifest',
      navIcon: () => null,
      navOrder: 8,
      cluster: 'Verify',
    },
  ];

  const canAccessRoute = (
    route: (typeof routes)[number],
    userRole?: string,
    userPermissions?: string[],
  ) => {
    if (route.requiredRoles && route.requiredRoles.length > 0) {
      if (!userRole || !route.requiredRoles.some(role => role.toLowerCase() === userRole.toLowerCase())) {
        return false;
      }
    }

    if (route.requiredPermissions && route.requiredPermissions.length > 0) {
      if (
        !userPermissions ||
        !route.requiredPermissions.every((perm) => userPermissions.includes(perm))
      ) {
        return false;
      }
    }

    return true;
  };

  return { routes, canAccessRoute };
});

import { generateNavigationGroups, shouldShowNavGroup } from '@/utils/navigation';

describe('generateNavigationGroups', () => {
  it('orders navigation groups by primary spine for operators', () => {
    const groups = generateNavigationGroups('operator', []);
    const titles = groups.map(group => group.title);
    expect(titles.slice(0, 4)).toEqual(['Build', 'Run', 'Observe', 'Verify']);
  });

  it('hides routes without navTitle or outside the primary spine', () => {
    const groups = generateNavigationGroups('operator', []);
    const buildItems = groups.find(g => g.title === 'Build')?.items.map(item => item.label);
    expect(buildItems).toEqual(expect.arrayContaining(['Onboarding', 'Adapters']));
    expect(buildItems).not.toEqual(expect.arrayContaining([undefined]));
  });

  it('filters role-restricted routes when role is missing', () => {
    const operatorGroups = generateNavigationGroups('operator', []);
    const build = operatorGroups.find(group => group.title === 'Build');
    expect(build?.items.map(item => item.label)).not.toContain('Training');
  });

  it('includes role-restricted routes for admins', () => {
    const adminGroups = generateNavigationGroups('admin', []);
    const build = adminGroups.find(group => group.title === 'Build');
    expect(build?.items.map(item => item.label)).toEqual(
      expect.arrayContaining(['Onboarding', 'Adapters', 'Training', 'Routing Config']),
    );
  });

  it('filters permission-restricted routes when missing permissions', () => {
    const verifyGroups = generateNavigationGroups('operator', []);
    const verify = verifyGroups.find(group => group.title === 'Verify');
    expect(verify?.items.map(item => item.label)).not.toContain('Audit Logs');
  });

  it('shows permission-restricted routes when permissions are present', () => {
    const verifyGroups = generateNavigationGroups('operator', ['audit.view']);
    const verify = verifyGroups.find(group => group.title === 'Verify');
    expect(verify?.items.map(item => item.label)).toEqual(
      expect.arrayContaining(['Guardrails', 'Audit Logs', 'Compliance']),
    );
  });
});

describe('shouldShowNavGroup', () => {
  it('returns true for groups without role restrictions', () => {
    const adminGroups = generateNavigationGroups('admin', []);
    const runGroup = adminGroups.find(group => group.title === 'Run');
    expect(runGroup).toBeDefined();
    expect(shouldShowNavGroup(runGroup!, 'operator')).toBe(true);
  });

  it('returns true when group is unrestricted or matches role', () => {
    const operatorGroups = generateNavigationGroups('operator', []);
    const observeGroup = operatorGroups.find(group => group.title === 'Observe');
    expect(observeGroup).toBeDefined();
    expect(shouldShowNavGroup(observeGroup!, 'operator')).toBe(true);

    const adminGroups = generateNavigationGroups('admin', []);
    const verifyGroup = adminGroups.find(group => group.title === 'Verify');
    expect(verifyGroup).toBeDefined();
    expect(shouldShowNavGroup(verifyGroup!, 'admin')).toBe(true);
  });
});
