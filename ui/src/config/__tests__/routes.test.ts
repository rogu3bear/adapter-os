import { describe, expect, it, beforeEach } from 'vitest';
import {
  routes,
  canAccessRoute,
  getRouteByPath,
  matchRoute,
  getBreadcrumbs,
  formatClusterPrefixedLabel,
  type RouteConfig,
} from '../routes';
import { UiMode } from '../ui-mode';
import type { UserRole } from '@/api/types';

/**
 * Comprehensive Route Access Control Tests
 *
 * This test suite ensures proper route configuration and access control for the application.
 * It validates that routes have correct permissions, role requirements, and UI mode filtering.
 *
 * Coverage areas:
 * 1. Route permission matrix validation
 * 2. Role-based route filtering
 * 3. UI mode filtering (User/Builder/Audit)
 * 4. Route path uniqueness
 * 5. Route manifest integrity
 * 6. Protected route authentication
 * 7. Admin-only route access
 * 8. Permission-based access control
 */

describe('Route Configuration Tests', () => {
  describe('Route Manifest Integrity', () => {
    it('should have all routes with required fields', () => {
      routes.forEach((route, index) => {
        expect(route.path, `Route at index ${index} missing path`).toBeDefined();
        expect(route.path, `Route at index ${index} path is empty`).not.toBe('');
        expect(route.component, `Route ${route.path} missing component`).toBeDefined();
        expect(route.cluster, `Route ${route.path} missing cluster`).toBeDefined();
        expect(route.breadcrumb, `Route ${route.path} missing breadcrumb`).toBeDefined();
      });
    });

    it('should have valid cluster values', () => {
      const validClusters = ['Build', 'Run', 'Observe', 'Verify'];
      routes.forEach(route => {
        expect(
          validClusters,
          `Route ${route.path} has invalid cluster: ${route.cluster}`
        ).toContain(route.cluster);
      });
    });

    it('should have valid skeleton variants when defined', () => {
      const validVariants = ['default', 'dashboard', 'table', 'form'];
      routes.forEach(route => {
        if (route.skeletonVariant) {
          expect(
            validVariants,
            `Route ${route.path} has invalid skeleton variant: ${route.skeletonVariant}`
          ).toContain(route.skeletonVariant);
        }
      });
    });

    it('should have breadcrumbs for all routes', () => {
      routes.forEach(route => {
        expect(route.breadcrumb, `Route ${route.path} missing breadcrumb`).toBeDefined();
        expect(route.breadcrumb, `Route ${route.path} breadcrumb is empty`).not.toBe('');
      });
    });
  });

  describe('Route Path Uniqueness', () => {
    it('should not have duplicate route paths', () => {
      const pathCounts = new Map<string, number>();

      routes.forEach(route => {
        const count = pathCounts.get(route.path) || 0;
        pathCounts.set(route.path, count + 1);
      });

      const duplicates = Array.from(pathCounts.entries())
        .filter(([_, count]) => count > 1)
        .map(([path]) => path);

      expect(duplicates, `Duplicate paths found: ${duplicates.join(', ')}`).toHaveLength(0);
    });

    it('should have valid path formats', () => {
      routes.forEach(route => {
        // All paths should start with /
        expect(route.path.startsWith('/'), `Route ${route.path} must start with /`).toBe(true);

        // Path params should be properly formatted
        const paramMatches = route.path.match(/:(\w+)/g);
        if (paramMatches) {
          paramMatches.forEach(param => {
            expect(
              /^:[a-zA-Z]\w*$/.test(param),
              `Invalid param format in ${route.path}: ${param}`
            ).toBe(true);
          });
        }
      });
    });
  });

  describe('Route Permission Matrix Validation', () => {
    it('should have valid UserRole values in requiredRoles', () => {
      const validRoles: UserRole[] = [
        'admin',
        'developer',
        'operator',
        'sre',
        'compliance',
        'auditor',
        'viewer',
      ];

      routes.forEach(route => {
        if (route.requiredRoles) {
          route.requiredRoles.forEach(role => {
            expect(
              validRoles,
              `Route ${route.path} has invalid requiredRole: ${role}`
            ).toContain(role);
          });
        }
      });
    });

    it('should have valid UserRole values in roleVisibility', () => {
      const validRoles: UserRole[] = [
        'admin',
        'developer',
        'operator',
        'sre',
        'compliance',
        'auditor',
        'viewer',
      ];

      routes.forEach(route => {
        if (route.roleVisibility) {
          route.roleVisibility.forEach(role => {
            expect(
              validRoles,
              `Route ${route.path} has invalid roleVisibility: ${role}`
            ).toContain(role);
          });
        }
      });
    });

    it('should have roleVisibility when requiredRoles is defined', () => {
      routes.forEach(route => {
        if (route.requiredRoles && route.requiredRoles.length > 0) {
          expect(
            route.roleVisibility,
            `Route ${route.path} has requiredRoles but no roleVisibility`
          ).toBeDefined();
          expect(
            route.roleVisibility!.length,
            `Route ${route.path} has requiredRoles but empty roleVisibility`
          ).toBeGreaterThan(0);
        }
      });
    });

    it('should have consistent requiredRoles and roleVisibility for sensitive routes', () => {
      // For routes with requiredRoles, roleVisibility should be a subset or equal
      routes.forEach(route => {
        if (route.requiredRoles && route.requiredRoles.length > 0 && route.roleVisibility) {
          // All roles in requiredRoles should be in roleVisibility
          route.requiredRoles.forEach(requiredRole => {
            const inVisibility = route.roleVisibility?.some(
              visRole => visRole.toLowerCase() === requiredRole.toLowerCase()
            );
            expect(
              inVisibility,
              `Route ${route.path}: requiredRole "${requiredRole}" not in roleVisibility`
            ).toBe(true);
          });
        }
      });
    });
  });

  describe('Admin-Only Routes', () => {
    const adminOnlyPaths = [
      '/admin',
      '/admin/tenants',
      '/admin/tenants/:tenantId',
      '/admin/stacks',
      '/admin/stacks/:stackId',
      '/admin/plugins',
      '/admin/settings',
      '/federation',
    ];

    adminOnlyPaths.forEach(path => {
      it(`${path} should require admin role`, () => {
        const route = getRouteByPath(path);
        expect(route, `Route ${path} not found`).toBeDefined();
        expect(route?.requiredRoles, `Route ${path} missing requiredRoles`).toBeDefined();
        expect(route?.requiredRoles, `Route ${path} should include admin`).toContain('admin');
      });

      it(`${path} should only be visible to admin`, () => {
        const route = getRouteByPath(path);
        expect(route?.roleVisibility, `Route ${path} missing roleVisibility`).toBeDefined();
        expect(route?.roleVisibility, `Route ${path} should only show admin`).toEqual(['admin']);
      });
    });

    it('/base-models should only be visible to admin', () => {
      const route = getRouteByPath('/base-models');
      expect(route?.roleVisibility).toEqual(['admin']);
    });
  });

  describe('Protected Routes Authentication', () => {
    it('should have requiresAuth for all non-public routes', () => {
      // Known public and legacy routes that don't require auth
      const publicPaths = ['/help', '/personas'];
      // Dev routes also may not require auth
      const devRoutes = routes.filter(r => r.path.startsWith('/dev/') || r.path.startsWith('/_dev/'));

      routes.forEach(route => {
        const isDevRoute = devRoutes.some(d => d.path === route.path);
        if (!publicPaths.includes(route.path) && !isDevRoute) {
          expect(
            route.requiresAuth,
            `Route ${route.path} should require authentication`
          ).toBe(true);
        }
      });
    });

    it('should have appropriate access controls for authenticated routes', () => {
      routes
        .filter(route => route.requiresAuth)
        .forEach(route => {
          // Authenticated routes should have either requiredRoles, requiredPermissions, or roleVisibility
          const hasAccessControl =
            route.requiredRoles?.length ||
            route.requiredPermissions?.length ||
            route.roleVisibility?.length;

          expect(
            hasAccessControl,
            `Authenticated route ${route.path} has no access controls (requiredRoles/requiredPermissions/roleVisibility)`
          ).toBeTruthy();
        });
    });
  });

  describe('Permission-Based Access Control', () => {
    it('should have valid permission strings', () => {
      const permissionPattern = /^[a-z]+:[a-z_]+$/;

      routes.forEach(route => {
        if (route.requiredPermissions) {
          route.requiredPermissions.forEach(permission => {
            expect(
              permissionPattern.test(permission),
              `Route ${route.path} has invalid permission format: ${permission}`
            ).toBe(true);
          });
        }
      });
    });

    const permissionRoutes = [
      { path: '/inference', permission: 'inference:execute' },
      { path: '/adapters/new', permission: 'adapter:register' },
      { path: '/security/audit', permission: 'audit:view' },
      { path: '/security/compliance', permission: 'audit:view' },
    ];

    permissionRoutes.forEach(({ path, permission }) => {
      it(`${path} should require ${permission} permission`, () => {
        const route = getRouteByPath(path);
        expect(route?.requiredPermissions, `Route ${path} missing requiredPermissions`).toBeDefined();
        expect(
          route?.requiredPermissions,
          `Route ${path} should require ${permission}`
        ).toContain(permission);
      });
    });
  });

  describe('UI Mode Filtering', () => {
    it('should have valid UI modes when defined', () => {
      const validModes = [UiMode.User, UiMode.Builder, UiMode.Kernel, UiMode.Audit];

      routes.forEach(route => {
        if (route.modes && route.modes.length > 0) {
          route.modes.forEach(mode => {
            expect(
              validModes,
              `Route ${route.path} has invalid mode: ${mode}`
            ).toContain(mode);
          });
        }
      });
    });

    it('should have User mode routes', () => {
      const userModeRoutes = routes.filter(route => route.modes?.includes(UiMode.User));
      expect(userModeRoutes.length).toBeGreaterThan(0);
    });

    it('should have Builder mode routes', () => {
      const builderModeRoutes = routes.filter(route => route.modes?.includes(UiMode.Builder));
      expect(builderModeRoutes.length).toBeGreaterThan(0);
    });

    it('should have Audit mode routes', () => {
      const auditModeRoutes = routes.filter(route => route.modes?.includes(UiMode.Audit));
      expect(auditModeRoutes.length).toBeGreaterThan(0);
    });

    const modeExpectations = [
      { path: '/dashboard', modes: [UiMode.User] },
      { path: '/training', modes: [UiMode.Builder] },
      { path: '/adapters', modes: [UiMode.Builder] },
      { path: '/telemetry', modes: [UiMode.Audit] },
      { path: '/replay', modes: [UiMode.Audit] },
      { path: '/security/policies', modes: [UiMode.Audit] },
    ];

    modeExpectations.forEach(({ path, modes }) => {
      it(`${path} should be in ${modes.join(', ')} mode(s)`, () => {
        const route = getRouteByPath(path);
        expect(route?.modes).toEqual(modes);
      });
    });

    it('legacy routes should have empty modes array', () => {
      const legacyPaths = [
        '/owner',
        '/management',
        '/workflow',
        '/personas',
        '/flow/lora',
        '/trainer',
        '/promotion',
        '/monitoring',
        '/reports',
        '/code-intelligence',
        '/metrics/advanced',
        '/help',
      ];

      legacyPaths.forEach(path => {
        const route = getRouteByPath(path);
        if (route) {
          expect(route.modes, `Legacy route ${path} should have empty modes`).toEqual([]);
        }
      });
    });
  });

  describe('Navigation Hierarchy', () => {
    it('should have valid parentPath references', () => {
      routes.forEach(route => {
        if (route.parentPath) {
          // Parent path should either match a route exactly or be a parameterized path
          const parentRoute =
            getRouteByPath(route.parentPath) ||
            routes.find(r => {
              // Check if it's a parameterized parent
              const routeParts = r.path.split('/');
              const parentParts = route.parentPath!.split('/');
              if (routeParts.length !== parentParts.length) return false;
              return routeParts.every(
                (part, i) => part === parentParts[i] || part.startsWith(':')
              );
            });

          expect(
            parentRoute,
            `Route ${route.path} has invalid parentPath: ${route.parentPath}`
          ).toBeDefined();
        }
      });
    });

    it('should not have circular parentPath references', () => {
      routes.forEach(route => {
        const visited = new Set<string>();
        let current = route;
        let depth = 0;
        const maxDepth = 10; // Prevent infinite loops

        while (current.parentPath && depth < maxDepth) {
          expect(
            visited.has(current.path),
            `Circular parentPath detected for route ${route.path}`
          ).toBe(false);

          visited.add(current.path);
          const parent = getRouteByPath(current.parentPath);
          if (!parent) break;

          current = parent;
          depth++;
        }
      });
    });

    it('should have navOrder for routes with navTitle', () => {
      routes
        .filter(route => route.navTitle && route.navGroup)
        .forEach(route => {
          expect(
            route.navOrder,
            `Route ${route.path} has navTitle but missing navOrder`
          ).toBeDefined();
        });
    });

    it('should have navIcon for routes with navTitle', () => {
      routes
        .filter(route => route.navTitle)
        .forEach(route => {
          expect(
            route.navIcon,
            `Route ${route.path} has navTitle but missing navIcon`
          ).toBeDefined();
        });
    });

    it('should have valid navGroup when defined', () => {
      const validNavGroups = ['Build', 'Run', 'Observe', 'Verify'];

      routes.forEach(route => {
        if (route.navGroup) {
          expect(
            validNavGroups,
            `Route ${route.path} has invalid navGroup: ${route.navGroup}`
          ).toContain(route.navGroup);
        }
      });
    });
  });
});

describe('Role-Based Route Filtering', () => {
  const allRoles: UserRole[] = ['admin', 'operator', 'sre', 'compliance', 'auditor', 'viewer'];

  describe('canAccessRoute function', () => {
    it('should allow developer role to access all routes', () => {
      routes.forEach(route => {
        const hasAccess = canAccessRoute(route, 'developer');
        expect(hasAccess, `Developer should access ${route.path}`).toBe(true);
      });
    });

    it('should allow admin to access admin-only routes', () => {
      const adminRoutes = routes.filter(route => route.requiredRoles?.includes('admin'));
      adminRoutes.forEach(route => {
        const hasAccess = canAccessRoute(route, 'admin');
        expect(hasAccess, `Admin should access ${route.path}`).toBe(true);
      });
    });

    it('should deny non-admin users from admin-only routes', () => {
      const nonAdminRoles: UserRole[] = ['operator', 'sre', 'compliance', 'auditor', 'viewer'];
      const adminOnlyRoutes = routes.filter(
        route => route.requiredRoles?.includes('admin') && route.requiredRoles.length === 1
      );

      adminOnlyRoutes.forEach(route => {
        nonAdminRoles.forEach(role => {
          const hasAccess = canAccessRoute(route, role);
          expect(hasAccess, `${role} should not access admin-only route ${route.path}`).toBe(
            false
          );
        });
      });
    });

    it('should respect roleVisibility restrictions', () => {
      const restrictedRoute: RouteConfig = {
        path: '/test-visibility',
        component: () => null,
        requiresAuth: true,
        roleVisibility: ['admin', 'operator'],
        cluster: 'Build',
        breadcrumb: 'Test',
      };

      expect(canAccessRoute(restrictedRoute, 'admin')).toBe(true);
      expect(canAccessRoute(restrictedRoute, 'operator')).toBe(true);
      expect(canAccessRoute(restrictedRoute, 'viewer')).toBe(false);
    });

    it('should respect requiredRoles restrictions', () => {
      const restrictedRoute: RouteConfig = {
        path: '/test-required',
        component: () => null,
        requiresAuth: true,
        requiredRoles: ['admin', 'operator'],
        cluster: 'Build',
        breadcrumb: 'Test',
      };

      expect(canAccessRoute(restrictedRoute, 'admin')).toBe(true);
      expect(canAccessRoute(restrictedRoute, 'operator')).toBe(true);
      expect(canAccessRoute(restrictedRoute, 'viewer')).toBe(false);
    });

    it('should handle permission-based access', () => {
      const permissionRoute: RouteConfig = {
        path: '/test-permission',
        component: () => null,
        requiresAuth: true,
        requiredPermissions: ['test:execute'],
        cluster: 'Run',
        breadcrumb: 'Test',
      };

      expect(canAccessRoute(permissionRoute, 'admin', ['test:execute'])).toBe(true);
      expect(canAccessRoute(permissionRoute, 'admin', ['other:permission'])).toBe(false);
      expect(canAccessRoute(permissionRoute, 'admin', [])).toBe(false);
      expect(canAccessRoute(permissionRoute, 'admin')).toBe(false);
    });

    it('should handle case-insensitive role matching', () => {
      const route = getRouteByPath('/admin')!;
      expect(canAccessRoute(route, 'admin')).toBe(true);
      expect(canAccessRoute(route, 'ADMIN' as UserRole)).toBe(true);
      expect(canAccessRoute(route, 'Admin' as UserRole)).toBe(true);
    });

    it('should deny access when no role provided for protected route', () => {
      const protectedRoute = routes.find(route => route.requiredRoles?.length);
      if (protectedRoute) {
        const hasAccess = canAccessRoute(protectedRoute, undefined);
        expect(hasAccess).toBe(false);
      }
    });
  });

  describe('getAccessibleRoutesForRole helper', () => {
    const getAccessibleRoutesForRole = (role: UserRole, permissions?: string[]): RouteConfig[] => {
      return routes.filter(route => canAccessRoute(route, role, permissions));
    };

    it('should return different routes for different roles', () => {
      const adminRoutes = getAccessibleRoutesForRole('admin');
      const viewerRoutes = getAccessibleRoutesForRole('viewer');

      expect(adminRoutes.length).toBeGreaterThan(viewerRoutes.length);
    });

    it('should return operator-accessible routes', () => {
      const operatorRoutes = getAccessibleRoutesForRole('operator');
      expect(operatorRoutes.length).toBeGreaterThan(0);

      // Operators should access training
      const trainingRoute = operatorRoutes.find(r => r.path === '/training');
      expect(trainingRoute).toBeDefined();
    });

    it('should return viewer-accessible routes', () => {
      const viewerRoutes = getAccessibleRoutesForRole('viewer');
      expect(viewerRoutes.length).toBeGreaterThan(0);

      // Viewers should see dashboard
      const dashboardRoute = viewerRoutes.find(r => r.path === '/dashboard');
      expect(dashboardRoute).toBeDefined();

      // Viewers should NOT see admin routes
      const adminRoute = viewerRoutes.find(r => r.path === '/admin');
      expect(adminRoute).toBeUndefined();
    });

    it('should include routes with no role restrictions', () => {
      allRoles.forEach(role => {
        const accessibleRoutes = getAccessibleRoutesForRole(role);
        const unrestrictedRoutes = routes.filter(
          route => !route.requiredRoles && !route.roleVisibility
        );

        unrestrictedRoutes.forEach(unrestrictedRoute => {
          const isAccessible = accessibleRoutes.some(r => r.path === unrestrictedRoute.path);
          expect(
            isAccessible,
            `${role} should access unrestricted route ${unrestrictedRoute.path}`
          ).toBe(true);
        });
      });
    });
  });

  describe('Role-specific route counts', () => {
    it('should have expected number of admin-accessible routes', () => {
      const adminRoutes = routes.filter(route => canAccessRoute(route, 'admin'));
      // Admin should have access to all routes (developer bypasses all)
      expect(adminRoutes.length).toBeGreaterThan(50);
    });

    it('should have operator-accessible routes less than admin', () => {
      const adminRoutes = routes.filter(route => canAccessRoute(route, 'admin'));
      const operatorRoutes = routes.filter(route => canAccessRoute(route, 'operator'));
      expect(operatorRoutes.length).toBeLessThan(adminRoutes.length);
    });

    it('should have viewer-accessible routes less than operator', () => {
      const operatorRoutes = routes.filter(route => canAccessRoute(route, 'operator'));
      const viewerRoutes = routes.filter(route => canAccessRoute(route, 'viewer'));
      expect(viewerRoutes.length).toBeLessThan(operatorRoutes.length);
    });
  });
});

describe('Route Helper Functions', () => {
  describe('getRouteByPath', () => {
    it('should find route by exact path', () => {
      const route = getRouteByPath('/dashboard');
      expect(route).toBeDefined();
      expect(route?.path).toBe('/dashboard');
    });

    it('should return undefined for non-existent path', () => {
      const route = getRouteByPath('/non-existent-route');
      expect(route).toBeUndefined();
    });

    it('should find parameterized routes', () => {
      const route = getRouteByPath('/adapters/:adapterId');
      expect(route).toBeDefined();
      expect(route?.path).toBe('/adapters/:adapterId');
    });
  });

  describe('matchRoute', () => {
    it('should match exact paths', () => {
      const route = matchRoute('/dashboard');
      expect(route).toBeDefined();
      expect(route?.path).toBe('/dashboard');
    });

    it('should match parameterized paths with actual values', () => {
      const route = matchRoute('/adapters/abc-123');
      expect(route).toBeDefined();
      expect(route?.path).toBe('/adapters/:adapterId');
    });

    it('should match nested parameterized paths', () => {
      const route = matchRoute('/training/jobs/job-456');
      expect(route).toBeDefined();
      expect(route?.path).toBe('/training/jobs/:jobId');
    });

    it('should match multiple parameter segments', () => {
      const route = matchRoute('/training/datasets/dataset-123/chat');
      expect(route).toBeDefined();
      expect(route?.path).toBe('/training/datasets/:datasetId/chat');
    });

    it('should return undefined for non-matching path', () => {
      const route = matchRoute('/completely/invalid/path');
      expect(route).toBeUndefined();
    });

    it('should not match paths with different segment counts', () => {
      const route = matchRoute('/adapters/abc-123/extra/segment');
      // Should not match /adapters/:adapterId
      expect(route?.path).not.toBe('/adapters/:adapterId');
    });
  });

  describe('getBreadcrumbs', () => {
    it('should return breadcrumbs for simple route', () => {
      const breadcrumbs = getBreadcrumbs('/dashboard');
      expect(breadcrumbs).toHaveLength(1);
      expect(breadcrumbs[0]).toEqual({ path: '/dashboard', label: 'Dashboard' });
    });

    it('should return hierarchical breadcrumbs', () => {
      const breadcrumbs = getBreadcrumbs('/training/jobs');
      expect(breadcrumbs.length).toBeGreaterThan(1);
      expect(breadcrumbs[0]).toEqual({ path: '/training', label: 'Training' });
      expect(breadcrumbs[1]).toEqual({ path: '/training/jobs', label: 'Jobs' });
    });

    it('should resolve parameterized breadcrumb paths', () => {
      const breadcrumbs = getBreadcrumbs('/adapters/abc-123', { adapterId: 'abc-123' });
      expect(breadcrumbs.length).toBeGreaterThan(1);
      expect(breadcrumbs[0]).toEqual({ path: '/adapters', label: 'Adapters' });
      expect(breadcrumbs[1]).toEqual({ path: '/adapters/abc-123', label: 'Adapter Detail' });
    });

    it('should handle deeply nested routes', () => {
      const breadcrumbs = getBreadcrumbs('/training/datasets/dataset-123/chat', {
        datasetId: 'dataset-123',
      });
      expect(breadcrumbs.length).toBeGreaterThanOrEqual(3);
    });

    it('should return empty array for non-existent route', () => {
      const breadcrumbs = getBreadcrumbs('/non-existent');
      expect(breadcrumbs).toEqual([]);
    });

    it('should extract params from pathname when not provided', () => {
      const breadcrumbs = getBreadcrumbs('/adapters/abc-123');
      expect(breadcrumbs.length).toBeGreaterThan(0);
      // Should still resolve the path even without explicit params
      const hasResolvedPath = breadcrumbs.some(b => b.path === '/adapters/abc-123');
      expect(hasResolvedPath).toBe(true);
    });
  });

  describe('formatClusterPrefixedLabel', () => {
    it('should prefix label with cluster when route is known', () => {
      expect(formatClusterPrefixedLabel('/training', 'Training')).toBe('Build / Training');
    });

    it('should use custom delimiter', () => {
      expect(formatClusterPrefixedLabel('/training', 'Training', ': ')).toBe('Build: Training');
    });

    it('should fall back to label when route cluster is unknown', () => {
      expect(formatClusterPrefixedLabel('/not-a-route', 'Custom')).toBe('Custom');
    });

    it('should handle parameterized routes', () => {
      const result = formatClusterPrefixedLabel('/adapters/abc-123', 'Adapter');
      expect(result).toBe('Build / Adapter');
    });
  });
});

describe('Specific Route Requirements', () => {
  describe('Training routes', () => {
    it('/training should require admin or operator role', () => {
      const route = getRouteByPath('/training');
      expect(route?.requiredRoles).toBeDefined();
      expect(route?.requiredRoles).toContain('admin');
      expect(route?.requiredRoles).toContain('operator');
    });

    it('/training should be in Builder mode', () => {
      const route = getRouteByPath('/training');
      expect(route?.modes).toBeDefined();
      expect(route?.modes).toContain(UiMode.Builder);
    });

    // Sub-routes inherit access from parent /training route
    const trainingSubPaths = ['/training/jobs', '/training/datasets', '/training/templates'];

    trainingSubPaths.forEach(path => {
      it(`${path} should be visible to admin and operator`, () => {
        const route = getRouteByPath(path);
        expect(route?.roleVisibility).toContain('admin');
        expect(route?.roleVisibility).toContain('operator');
      });
    });
  });

  describe('Telemetry and audit routes', () => {
    const auditPaths = ['/telemetry', '/replay', '/security/audit', '/security/compliance'];

    auditPaths.forEach(path => {
      it(`${path} should be in Audit mode`, () => {
        const route = getRouteByPath(path);
        expect(route?.modes, `${path} should be in Audit mode`).toContain(UiMode.Audit);
      });
    });
  });

  describe('System monitoring routes', () => {
    const systemPaths = ['/system', '/system/nodes', '/system/workers', '/metrics'];

    systemPaths.forEach(path => {
      it(`${path} should be visible to SRE role`, () => {
        const route = getRouteByPath(path);
        expect(route?.roleVisibility, `${path} should be visible to SRE`).toContain('sre');
      });

      it(`${path} should be in User mode`, () => {
        const route = getRouteByPath(path);
        expect(route?.modes, `${path} should be in User mode`).toContain(UiMode.User);
      });
    });
  });

  describe('Inference and chat routes', () => {
    it('/inference should require inference:execute permission', () => {
      const route = getRouteByPath('/inference');
      expect(route?.requiredPermissions).toContain('inference:execute');
    });

    it('/chat should require admin or operator role', () => {
      const route = getRouteByPath('/chat');
      expect(route?.requiredRoles).toContain('admin');
      expect(route?.requiredRoles).toContain('operator');
    });
  });

  describe('Security routes access', () => {
    const securityRoutes = [
      '/security/policies',
      '/security/audit',
      '/security/compliance',
      '/security/evidence',
    ];

    securityRoutes.forEach(path => {
      it(`${path} should be accessible to compliance and auditor roles`, () => {
        const route = getRouteByPath(path);
        expect(route?.roleVisibility).toContain('compliance');
        expect(route?.roleVisibility).toContain('auditor');
      });
    });
  });
});

describe('Route Edge Cases', () => {
  it('should handle routes with no requiredRoles or roleVisibility', () => {
    const openRoute: RouteConfig = {
      path: '/test-open',
      component: () => null,
      requiresAuth: false,
      cluster: 'Run',
      breadcrumb: 'Test',
    };

    // Should be accessible to all roles
    const roles: UserRole[] = ['admin', 'operator', 'viewer'];
    roles.forEach(role => {
      expect(canAccessRoute(openRoute, role)).toBe(true);
    });
  });

  it('should handle routes with both roleVisibility and requiredRoles', () => {
    const route: RouteConfig = {
      path: '/test-both',
      component: () => null,
      requiresAuth: true,
      requiredRoles: ['admin'],
      roleVisibility: ['admin', 'operator'], // Visible to operator but not accessible
      cluster: 'Build',
      breadcrumb: 'Test',
    };

    expect(canAccessRoute(route, 'admin')).toBe(true);
    expect(canAccessRoute(route, 'operator')).toBe(false); // Visible but not accessible
    expect(canAccessRoute(route, 'viewer')).toBe(false);
  });

  it('should handle routes with multiple required permissions', () => {
    const route: RouteConfig = {
      path: '/test-multi-perm',
      component: () => null,
      requiresAuth: true,
      requiredPermissions: ['perm1:action', 'perm2:action'],
      cluster: 'Run',
      breadcrumb: 'Test',
    };

    expect(canAccessRoute(route, 'admin', ['perm1:action'])).toBe(true);
    expect(canAccessRoute(route, 'admin', ['perm2:action'])).toBe(true);
    expect(canAccessRoute(route, 'admin', ['perm1:action', 'perm2:action'])).toBe(true);
    expect(canAccessRoute(route, 'admin', ['other:permission'])).toBe(false);
  });

  it('should handle empty roleVisibility array', () => {
    const route: RouteConfig = {
      path: '/test-empty-visibility',
      component: () => null,
      requiresAuth: true,
      roleVisibility: [],
      cluster: 'Run',
      breadcrumb: 'Test',
    };

    // Empty roleVisibility array still allows access if no requiredRoles
    // The roleVisibility check passes when the array is empty (no restrictions)
    expect(canAccessRoute(route, 'admin')).toBe(true);
    expect(canAccessRoute(route, 'operator')).toBe(true);
  });
});

describe('Route Statistics', () => {
  it('should have a reasonable number of total routes', () => {
    expect(routes.length).toBeGreaterThan(50);
    expect(routes.length).toBeLessThan(200);
  });

  it('should have routes in all clusters', () => {
    const clusters = new Set(routes.map(r => r.cluster));
    expect(clusters.has('Build')).toBe(true);
    expect(clusters.has('Run')).toBe(true);
    expect(clusters.has('Observe')).toBe(true);
    expect(clusters.has('Verify')).toBe(true);
  });

  it('should have routes in all UI modes', () => {
    const userModes = routes.filter(r => r.modes?.includes(UiMode.User));
    const builderModes = routes.filter(r => r.modes?.includes(UiMode.Builder));
    const auditModes = routes.filter(r => r.modes?.includes(UiMode.Audit));

    expect(userModes.length).toBeGreaterThan(0);
    expect(builderModes.length).toBeGreaterThan(0);
    expect(auditModes.length).toBeGreaterThan(0);
  });

  it('should have navigation routes with titles', () => {
    const navRoutes = routes.filter(r => r.navTitle);
    expect(navRoutes.length).toBeGreaterThan(20);
  });

  it('should have protected routes', () => {
    const protectedRoutes = routes.filter(r => r.requiresAuth);
    expect(protectedRoutes.length).toBeGreaterThan(routes.length * 0.8); // Most should be protected
  });
});
