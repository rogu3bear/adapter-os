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
      requiredPermissions: ['audit.view'],
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
      requiredPermissions: ['audit.view'],
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

    const verifyWithPerms = generateNavigationGroups('auditor', ['audit.view'], UiMode.Audit);
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
