import { describe, expect, it, vi } from 'vitest';

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
      path: '/policies',
      navGroup: 'Compliance',
      navTitle: 'Policies',
      navIcon: () => null,
      navOrder: 1,
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
      requiredRoles: ['Admin'],
    },
    {
      path: '/tenants',
      navGroup: 'Administration',
      navTitle: 'Tenants',
      navIcon: () => null,
      navOrder: 3,
      requiredRoles: ['Admin'],
    },
  ];

  const canAccessRoute = (route: (typeof routes)[number], userRole?: string) => {
    if (!route.requiredRoles || route.requiredRoles.length === 0) {
      return true;
    }
    return userRole ? route.requiredRoles.includes(userRole) : false;
  };

  return { routes, canAccessRoute };
});

import { generateNavigationGroups, shouldShowNavGroup } from '@/utils/navigation';

describe('generateNavigationGroups', () => {
  it('orders navigation groups consistently for operators', () => {
    const groups = generateNavigationGroups('Operator');
    const titles = groups.map((group) => group.title);
    expect(titles.slice(0, 4)).toEqual(['Home', 'ML Pipeline', 'Monitoring', 'Operations']);
  });

  it('excludes restricted routes while keeping shared groups for non-admins', () => {
    const operatorGroups = generateNavigationGroups('Operator');
    const adminGroup = operatorGroups.find((group) => group.title === 'Administration');
    expect(adminGroup).toBeDefined();
    expect(adminGroup?.items.map((item) => item.label)).toEqual(expect.arrayContaining(['Reports']));
    expect(adminGroup?.items.some((item) => item.label === 'IT Admin')).toBe(false);
  });

  it('includes restricted administration routes for admins', () => {
    const adminGroups = generateNavigationGroups('Admin');
    const adminGroup = adminGroups.find((group) => group.title === 'Administration');
    expect(adminGroup).toBeDefined();
    expect(adminGroup?.items.map((item) => item.label)).toEqual(
      expect.arrayContaining(['IT Admin', 'Reports', 'Tenants']),
    );
    expect(adminGroup?.roles).toEqual(['Admin']);
  });
});

describe('shouldShowNavGroup', () => {
  it('returns false when user lacks required role', () => {
    const adminGroups = generateNavigationGroups('Admin');
    const adminGroup = adminGroups.find((group) => group.title === 'Administration');
    expect(adminGroup).toBeDefined();
    expect(shouldShowNavGroup(adminGroup!, 'Operator')).toBe(false);
  });

  it('returns true when group is unrestricted or matches role', () => {
    const operatorGroups = generateNavigationGroups('Operator');
    const operationsGroup = operatorGroups.find((group) => group.title === 'Operations');
    expect(operationsGroup).toBeDefined();
    expect(shouldShowNavGroup(operationsGroup!, 'Operator')).toBe(true);

    const adminGroups = generateNavigationGroups('Admin');
    const adminGroup = adminGroups.find((group) => group.title === 'Administration');
    expect(adminGroup).toBeDefined();
    expect(shouldShowNavGroup(adminGroup!, 'Admin')).toBe(true);
  });
});
