import { describe, expect, it, vi } from 'vitest';

vi.mock('@/config/routes_manifest', () => {
  return {
    PRIMARY_SPINE: [
      '/dashboard',
      '/workflow',
      '/trainer',
      '/metrics',
      '/telemetry',
      '/workspaces',
      '/security/policies',
      '/security/audit',
      '/reports',
      '/admin',
      '/admin/tenants',
    ],
  };
});

vi.mock('@/config/routes', () => {
  const routes = [
    {
      path: '/dashboard',
      navGroup: 'Home',
      navTitle: 'Dashboard',
      navIcon: () => null,
      navOrder: 1,
    },
    {
      path: '/workflow',
      navGroup: 'Home',
      navTitle: 'Getting Started',
      navIcon: () => null,
      navOrder: 2,
    },
    {
      path: '/trainer',
      navGroup: 'ML Pipeline',
      navTitle: 'Training',
      navIcon: () => null,
      navOrder: 1,
    },
    {
      path: '/metrics',
      navGroup: 'Monitoring',
      navTitle: 'Metrics',
      navIcon: () => null,
      navOrder: 1,
    },
    {
      path: '/telemetry',
      navGroup: 'Operations',
      navTitle: 'Telemetry',
      navIcon: () => null,
      navOrder: 1,
    },
    {
      path: '/workspaces',
      navGroup: 'Communication',
      navTitle: 'Workspaces',
      navIcon: () => null,
      navOrder: 1,
    },
    {
      path: '/security/policies',
      navGroup: 'Compliance',
      navTitle: 'Policies',
      navIcon: () => null,
      navOrder: 1,
    },
    {
      path: '/security/audit',
      navGroup: 'Compliance',
      navTitle: 'Audit Logs',
      navIcon: () => null,
      navOrder: 2,
      requiredPermissions: ['audit.view'],
    },
    {
      path: '/reports',
      navGroup: 'Administration',
      navTitle: 'Reports',
      navIcon: () => null,
      navOrder: 2,
    },
    {
      path: '/admin',
      navGroup: 'Administration',
      navTitle: 'IT Admin',
      navIcon: () => null,
      navOrder: 1,
      requiredRoles: ['admin'],
    },
    {
      path: '/admin/tenants',
      navGroup: 'Administration',
      navTitle: 'Tenants',
      navIcon: () => null,
      navOrder: 3,
      requiredRoles: ['admin'],
    },
    {
      path: '/labs',
      navGroup: 'Labs',
      navTitle: 'Labs',
      navIcon: () => null,
      navOrder: 1,
    },
  ];

  const canAccessRoute = (
    route: (typeof routes)[number],
    userRole?: string,
    userPermissions?: string[],
  ) => {
    if (route.requiredRoles && route.requiredRoles.length > 0) {
      if (!userRole || !route.requiredRoles.includes(userRole)) {
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
  it('orders navigation groups consistently for operators', () => {
    const groups = generateNavigationGroups('Operator', []);
    const titles = groups.map((group) => group.title);
    expect(titles.slice(0, 4)).toEqual(['Home', 'ML Pipeline', 'Monitoring', 'Operations']);
  });

  it('excludes restricted routes while keeping shared groups for non-admins', () => {
    const operatorGroups = generateNavigationGroups('Operator', []);
    const adminGroup = operatorGroups.find((group) => group.title === 'Administration');
    expect(adminGroup).toBeDefined();
    expect(adminGroup?.items.map((item) => item.label)).toEqual(expect.arrayContaining(['Reports']));
    expect(adminGroup?.items.some((item) => item.label === 'IT Admin')).toBe(false);
  });

  it('includes restricted administration routes for admins', () => {
    const adminGroups = generateNavigationGroups('admin', []);
    const adminGroup = adminGroups.find((group) => group.title === 'Administration');
    expect(adminGroup).toBeDefined();
    expect(adminGroup?.items.map((item) => item.label)).toEqual(
      expect.arrayContaining(['IT Admin', 'Reports', 'Tenants']),
    );
    // Group roles are set from the first route in the group (Reports has no requiredRoles)
    expect(adminGroup?.roles).toBeUndefined();
  });

  it('filters permission-restricted routes when missing permissions', () => {
    const complianceGroups = generateNavigationGroups('Operator', []);
    const complianceGroup = complianceGroups.find((group) => group.title === 'Compliance');
    expect(complianceGroup?.items.map((item) => item.label)).not.toContain('Audit Logs');
  });

  it('shows permission-restricted routes when permissions are present', () => {
    const complianceGroups = generateNavigationGroups('Operator', ['audit.view']);
    const complianceGroup = complianceGroups.find((group) => group.title === 'Compliance');
    expect(complianceGroup?.items.map((item) => item.label)).toEqual(expect.arrayContaining(['Policies', 'Audit Logs']));
  });

  it('filters out routes that are not in the primary spine', () => {
    const groups = generateNavigationGroups('Operator', []);
    const labsGroup = groups.find((group) => group.title === 'Labs');
    expect(labsGroup).toBeUndefined();
  });
});

describe('shouldShowNavGroup', () => {
  it('returns true for groups without role restrictions', () => {
    const adminGroups = generateNavigationGroups('admin', []);
    const adminGroup = adminGroups.find((group) => group.title === 'Administration');
    expect(adminGroup).toBeDefined();
    // Administration group has no role restrictions (roles from first route which is Reports)
    expect(shouldShowNavGroup(adminGroup!, 'Operator')).toBe(true);
  });

  it('returns true when group is unrestricted or matches role', () => {
    const operatorGroups = generateNavigationGroups('Operator', []);
    const operationsGroup = operatorGroups.find((group) => group.title === 'Operations');
    expect(operationsGroup).toBeDefined();
    expect(shouldShowNavGroup(operationsGroup!, 'Operator')).toBe(true);

    const adminGroups = generateNavigationGroups('admin', []);
    const adminGroup = adminGroups.find((group) => group.title === 'Administration');
    expect(adminGroup).toBeDefined();
    expect(shouldShowNavGroup(adminGroup!, 'admin')).toBe(true);
  });
});
