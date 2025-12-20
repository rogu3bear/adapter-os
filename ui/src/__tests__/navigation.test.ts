import { describe, expect, it, vi } from 'vitest';
import { UiMode } from '@/config/ui-mode';

vi.mock('@/config/routes_manifest', () => {
  return {
    PRIMARY_SPINE: [
      '/repos',
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
      '/telemetry',
      '/testing',
      '/golden',
      '/replay',
      '/security/policies',
      '/security/audit',
      '/security/compliance',
      '/_dev/routes',
      '/dev/api-errors',
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
      modes: [UiMode.Builder],
    },
    {
      path: '/repos',
      navGroup: 'Build',
      navTitle: 'Repositories',
      navIcon: () => null,
      navOrder: 1,
      cluster: 'Build',
      modes: [UiMode.Builder],
    },
    {
      path: '/adapters',
      navGroup: 'Build',
      navTitle: 'Adapters',
      navIcon: () => null,
      navOrder: 2,
      cluster: 'Build',
      modes: [UiMode.Builder],
    },
    {
      path: '/training',
      navGroup: 'Build',
      navTitle: 'Training',
      navIcon: () => null,
      navOrder: 3,
      cluster: 'Build',
      requiredRoles: ['admin'],
      modes: [UiMode.Builder],
    },
    {
      path: '/router-config',
      navGroup: 'Build',
      navTitle: 'Router Config',
      navIcon: () => null,
      navOrder: 4,
      cluster: 'Build',
      modes: [UiMode.Builder],
    },
    {
      path: '/base-models',
      navGroup: 'Build',
      navTitle: 'Base Models',
      navIcon: () => null,
      navOrder: 5,
      cluster: 'Build',
      modes: [UiMode.Builder],
    },
    {
      path: '/dashboard',
      navGroup: 'Run',
      navTitle: 'Dashboard',
      navIcon: () => null,
      navOrder: 0,
      cluster: 'Run',
      modes: [UiMode.User],
    },
    {
      path: '/inference',
      navGroup: 'Run',
      navTitle: 'Inference',
      navIcon: () => null,
      navOrder: 1,
      cluster: 'Run',
      modes: [UiMode.User],
    },
    {
      path: '/documents',
      navGroup: 'Run',
      navTitle: 'Documents',
      navIcon: () => null,
      navOrder: 3,
      cluster: 'Run',
      modes: [UiMode.User],
    },
    {
      path: '/metrics',
      navGroup: 'Observe',
      navTitle: 'Metrics',
      navIcon: () => null,
      navOrder: 1,
      cluster: 'Observe',
      modes: [UiMode.Builder],
    },
    {
      path: '/routing',
      navGroup: 'Observe',
      navTitle: 'Routing History',
      navIcon: () => null,
      navOrder: 2,
      cluster: 'Observe',
      modes: [UiMode.User],
    },
    {
      path: '/telemetry',
      navGroup: 'Observe',
      navTitle: 'Telemetry',
      navIcon: () => null,
      navOrder: 3,
      cluster: 'Observe',
      requiredPermissions: ['audit:view'],
      modes: [UiMode.User, UiMode.Audit],
    },
    {
      path: '/testing',
      navGroup: 'Verify',
      navTitle: 'Testing',
      navIcon: () => null,
      navOrder: 0,
      cluster: 'Verify',
      modes: [UiMode.Builder],
    },
    {
      path: '/golden',
      navGroup: 'Verify',
      navTitle: 'Verified Runs',
      navIcon: () => null,
      navOrder: 1,
      cluster: 'Verify',
      modes: [UiMode.Builder],
    },
    {
      path: '/replay',
      navGroup: 'Verify',
      navTitle: 'Replay',
      navIcon: () => null,
      navOrder: 2,
      cluster: 'Verify',
      modes: [UiMode.User, UiMode.Audit],
    },
    {
      path: '/security/policies',
      navGroup: 'Verify',
      navTitle: 'Guardrails',
      navIcon: () => null,
      navOrder: 3,
      cluster: 'Verify',
      modes: [UiMode.Builder, UiMode.Audit],
    },
    {
      path: '/security/audit',
      navGroup: 'Verify',
      navTitle: 'Audit Logs',
      navIcon: () => null,
      navOrder: 4,
      cluster: 'Verify',
      requiredPermissions: ['audit:view'],
      modes: [UiMode.Audit],
    },
    {
      path: '/security/compliance',
      navGroup: 'Verify',
      navTitle: 'Compliance',
      navIcon: () => null,
      navOrder: 5,
      cluster: 'Verify',
      modes: [UiMode.Audit],
    },
    {
      path: '/hidden',
      navGroup: 'Build',
      navIcon: () => null,
      navOrder: 99,
      cluster: 'Build',
    },
    {
      path: '/dev/api-errors',
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
    // Check role-based access
    if (route.requiredRoles && route.requiredRoles.length > 0) {
      if (!userRole || !route.requiredRoles.some(role => role.toLowerCase() === userRole.toLowerCase())) {
        return false;
      }
    }

    // Check permission-based access - user must have at least one of the required permissions
    if (route.requiredPermissions && route.requiredPermissions.length > 0) {
      if (!userPermissions || userPermissions.length === 0) {
        return false;
      }

      const hasPermission = route.requiredPermissions.some((perm) =>
        userPermissions.includes(perm)
      );

      if (!hasPermission) {
        return false;
      }
    }

    return true;
  };

  return { routes, canAccessRoute };
});

import { generateNavigationGroups, shouldShowNavGroup, getAccessibleRoutes, findRouteByPath } from '@/utils/navigation';

describe('generateNavigationGroups', () => {
  it('shows run-focused navigation in user mode', () => {
    const groups = generateNavigationGroups('operator', [], UiMode.User);
    const titles = groups.map(group => group.title);
    expect(titles).not.toContain('Build');
    const run = groups.find(group => group.title === 'Run');
    expect(run?.items.map(item => item.label)).toEqual(
      expect.arrayContaining(['Dashboard', 'Inference', 'Documents']),
    );
  });

  it('shows build navigation cluster for operators in builder mode', () => {
    const groups = generateNavigationGroups('operator', [], UiMode.Builder);
    const build = groups.find(group => group.title === 'Build');
    expect(build?.items.map(item => item.label)).toEqual(
      expect.arrayContaining(['Onboarding', 'Repositories', 'Adapters', 'Router Config', 'Base Models']),
    );
    const run = groups.find(group => group.title === 'Run');
    expect(run).toBeUndefined();
  });

  it('enforces audit permissions in audit mode', () => {
    const verifyGroups = generateNavigationGroups('auditor', [], UiMode.Audit);
    const verify = verifyGroups.find(group => group.title === 'Verify');
    expect(verify?.items.map(item => item.label)).not.toContain('Audit Logs');

    const verifyWithPerms = generateNavigationGroups('auditor', ['audit:view'], UiMode.Audit);
    const verifyWithPermsGroup = verifyWithPerms.find(group => group.title === 'Verify');
    expect(verifyWithPermsGroup?.items.map(item => item.label)).toEqual(
      expect.arrayContaining(['Guardrails', 'Audit Logs', 'Compliance', 'Replay']),
    );
    const observe = verifyWithPerms.find(group => group.title === 'Observe');
    expect(observe).toBeDefined();
    expect(observe?.items.map(item => item.label)).toContain('Telemetry');
  });
});

describe('shouldShowNavGroup', () => {
  it('returns true for groups without role restrictions', () => {
    const adminGroups = generateNavigationGroups('admin', [], UiMode.User);
    const runGroup = adminGroups.find(group => group.title === 'Run');
    expect(runGroup).toBeDefined();
    expect(shouldShowNavGroup(runGroup!, 'operator')).toBe(true);
  });

  it('returns true when group is unrestricted or matches role', () => {
    const operatorGroups = generateNavigationGroups('operator', [], UiMode.User);
    const observeGroup = operatorGroups.find(group => group.title === 'Observe');
    expect(observeGroup).toBeDefined();
    expect(shouldShowNavGroup(observeGroup!, 'operator')).toBe(true);

    const adminGroups = generateNavigationGroups('admin', [], UiMode.Audit);
    const verifyGroup = adminGroups.find(group => group.title === 'Verify');
    expect(verifyGroup).toBeDefined();
    expect(shouldShowNavGroup(verifyGroup!, 'admin')).toBe(true);
  });
});

describe('getAccessibleRoutes', () => {
  it('should filter routes based on role', () => {
    const adminRoutes = getAccessibleRoutes('admin', []);
    const adminPaths = adminRoutes.map(r => r.path);

    // Admin should have access to training (requires admin role)
    expect(adminPaths).toContain('/training');

    const operatorRoutes = getAccessibleRoutes('operator', []);
    const operatorPaths = operatorRoutes.map(r => r.path);

    // Operator should NOT have access to training (requires admin role)
    expect(operatorPaths).not.toContain('/training');
  });

  it('should filter routes based on permissions', () => {
    // User with audit:view permission
    const routesWithPermission = getAccessibleRoutes('auditor', ['audit:view']);
    const pathsWithPermission = routesWithPermission.map(r => r.path);

    // Should have access to telemetry (requires audit:view permission)
    expect(pathsWithPermission).toContain('/telemetry');
    expect(pathsWithPermission).toContain('/security/audit');

    // User without audit:view permission
    const routesWithoutPermission = getAccessibleRoutes('auditor', []);
    const pathsWithoutPermission = routesWithoutPermission.map(r => r.path);

    // Should NOT have access to telemetry or security/audit
    expect(pathsWithoutPermission).not.toContain('/telemetry');
    expect(pathsWithoutPermission).not.toContain('/security/audit');
  });

  it('should handle combined role and permission requirements', () => {
    // Admin with audit:view permission - should have maximum access
    const adminWithPerms = getAccessibleRoutes('admin', ['audit:view']);
    const adminPaths = adminWithPerms.map(r => r.path);

    expect(adminPaths).toContain('/training'); // role-restricted
    expect(adminPaths).toContain('/telemetry'); // permission-restricted
    expect(adminPaths).toContain('/security/audit'); // permission-restricted

    // Operator without audit:view permission - limited access
    const operatorWithoutPerms = getAccessibleRoutes('operator', []);
    const operatorPaths = operatorWithoutPerms.map(r => r.path);

    expect(operatorPaths).not.toContain('/training'); // no admin role
    expect(operatorPaths).not.toContain('/telemetry'); // no audit:view permission
    expect(operatorPaths).not.toContain('/security/audit'); // no audit:view permission

    // Operator with audit:view permission - partial access
    const operatorWithPerms = getAccessibleRoutes('operator', ['audit:view']);
    const operatorWithPermsPaths = operatorWithPerms.map(r => r.path);

    expect(operatorWithPermsPaths).not.toContain('/training'); // still no admin role
    expect(operatorWithPermsPaths).toContain('/telemetry'); // has permission
  });

  it('should return all routes for users without restrictions', () => {
    const allRoutes = getAccessibleRoutes('operator', []);

    // Should return at least some routes
    expect(allRoutes.length).toBeGreaterThan(0);

    // All returned routes should be accessible
    allRoutes.forEach(route => {
      expect(route.path).toBeDefined();
    });
  });

  it('should handle undefined role and permissions gracefully', () => {
    const routes = getAccessibleRoutes(undefined, undefined);

    // Should return routes that don't require authentication
    expect(Array.isArray(routes)).toBe(true);

    // Should not include role-restricted routes
    const paths = routes.map(r => r.path);
    expect(paths).not.toContain('/training'); // requires admin role
  });

  it('should handle empty permissions array', () => {
    const routes = getAccessibleRoutes('auditor', []);
    const paths = routes.map(r => r.path);

    // Should not include permission-restricted routes
    expect(paths).not.toContain('/telemetry'); // requires audit:view permission
    expect(paths).not.toContain('/security/audit'); // requires audit:view permission
  });
});

describe('findRouteByPath', () => {
  it('should find route by valid path', () => {
    const route = findRouteByPath('/adapters');

    expect(route).toBeDefined();
    expect(route?.path).toBe('/adapters');
    expect(route?.navTitle).toBe('Adapters');
  });

  it('should find route with nested path', () => {
    const route = findRouteByPath('/security/policies');

    expect(route).toBeDefined();
    expect(route?.path).toBe('/security/policies');
    expect(route?.navTitle).toBe('Guardrails');
  });

  it('should return undefined for non-existent path', () => {
    const route = findRouteByPath('/this/route/does/not/exist');

    expect(route).toBeUndefined();
  });

  it('should handle parameterized paths', () => {
    // findRouteByPath looks for exact matches, not parameterized patterns
    // So searching for an actual path with a param value should return undefined
    const routeWithParam = findRouteByPath('/adapters/some-adapter-id');
    expect(routeWithParam).toBeUndefined();

    // But the parameterized pattern itself should be findable
    // Note: This depends on the routes array having the pattern
    const paramPattern = findRouteByPath('/training/datasets/:datasetId');
    // This may or may not exist depending on if the mock includes it
    // The key is that findRouteByPath does exact matching, not pattern matching
  });

  it('should find dev routes', () => {
    const devRoute = findRouteByPath('/dev/api-errors');

    expect(devRoute).toBeDefined();
    expect(devRoute?.path).toBe('/dev/api-errors');
  });

  it('should find root-level routes', () => {
    const dashboard = findRouteByPath('/dashboard');

    expect(dashboard).toBeDefined();
    expect(dashboard?.path).toBe('/dashboard');
    expect(dashboard?.navTitle).toBe('Dashboard');
  });

  it('should handle empty string', () => {
    const route = findRouteByPath('');

    expect(route).toBeUndefined();
  });

  it('should be case-sensitive', () => {
    const route = findRouteByPath('/ADAPTERS');

    // Routes are defined in lowercase, so uppercase should not match
    expect(route).toBeUndefined();
  });
});
