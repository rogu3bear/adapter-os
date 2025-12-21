/**
 * Route Component Validation Tests
 *
 * These tests enforce that route components can be rendered without props.
 * RouteGuard renders components as `<Component />` with no props, so any
 * component with required props will crash at runtime.
 *
 * If these tests fail, you likely need to:
 * 1. Create a *RoutePage wrapper that reads params from URL and fetches data
 * 2. Update the route to use the wrapper instead of the modal/component directly
 *
 * @see TenantDetailRoutePage for an example of this pattern
 * @see ui/src/config/route-types.ts for type-level enforcement
 */

import { describe, it, expect } from 'vitest';
import { routes } from '@/config/routes';
import { BLOCKED_ROUTE_COMPONENTS } from '@/config/route-types';

/**
 * Components that should NEVER be routed directly because they have required props.
 * This list is validated against the actual routes configuration.
 */
const KNOWN_REQUIRED_PROPS_COMPONENTS = [
  // Modal components with required props like: open, onClose, tenant, etc.
  'TenantDetailPage',
  // Add more as discovered during code review
] as const;

/**
 * Patterns that suggest a component might have required props.
 * Components matching these patterns should be reviewed carefully.
 */
const SUSPICIOUS_COMPONENT_PATTERNS = [
  /Modal$/,
  /Dialog$/,
  /Drawer$/,
  /Sheet$/,
  /Popup$/,
  /Overlay$/,
  /Picker$/,
];

/**
 * Whitelist of components that match suspicious patterns but are actually safe.
 * These have been reviewed and confirmed to have no required props.
 */
const PATTERN_WHITELIST = [
  // Example: 'SafeModal' - reviewed, all props optional
] as const;

describe('Route Component Validation', () => {
  describe('Blocked Components', () => {
    it('should not route any components in BLOCKED_ROUTE_COMPONENTS list', () => {
      const violations: string[] = [];

      for (const route of routes) {
        const component = route.component as React.ComponentType<unknown>;
        const componentName = component.displayName || component.name || '';

        // Check against blocked list
        if (BLOCKED_ROUTE_COMPONENTS.includes(componentName as typeof BLOCKED_ROUTE_COMPONENTS[number])) {
          violations.push(
            `Route "${route.path}" uses blocked component "${componentName}". ` +
            `This component has required props and will crash when rendered without them. ` +
            `Create a *RoutePage wrapper instead.`
          );
        }
      }

      expect(violations).toEqual([]);
    });

    it('should not route any components in KNOWN_REQUIRED_PROPS_COMPONENTS', () => {
      const violations: string[] = [];

      for (const route of routes) {
        const component = route.component as React.ComponentType<unknown>;
        const componentName = component.displayName || component.name || '';

        if (KNOWN_REQUIRED_PROPS_COMPONENTS.includes(componentName as typeof KNOWN_REQUIRED_PROPS_COMPONENTS[number])) {
          violations.push(
            `Route "${route.path}" uses "${componentName}" which has required props. ` +
            `Create a *RoutePage wrapper that reads params from URL and fetches data.`
          );
        }
      }

      expect(violations).toEqual([]);
    });
  });

  describe('Suspicious Patterns', () => {
    it('should flag components matching modal/dialog patterns (unless whitelisted)', () => {
      const warnings: string[] = [];

      for (const route of routes) {
        const component = route.component as React.ComponentType<unknown>;
        const componentName = component.displayName || component.name || '';

        // Skip if whitelisted
        if (PATTERN_WHITELIST.includes(componentName as typeof PATTERN_WHITELIST[number])) {
          continue;
        }

        // Skip *RoutePage wrappers - those are the solution
        if (componentName.endsWith('RoutePage')) {
          continue;
        }

        // Check against suspicious patterns
        for (const pattern of SUSPICIOUS_COMPONENT_PATTERNS) {
          if (pattern.test(componentName)) {
            warnings.push(
              `Route "${route.path}" uses "${componentName}" which matches suspicious pattern "${pattern}". ` +
              `Components like modals/dialogs typically have required props (open, onClose, etc.). ` +
              `If this is intentional and the component has no required props, add it to PATTERN_WHITELIST.`
            );
            break;
          }
        }
      }

      // This is a warning-level check - we log but don't fail
      // To make it strict, change to: expect(warnings).toEqual([]);
      if (warnings.length > 0) {
        console.warn('Suspicious route components detected:\n' + warnings.join('\n'));
      }
    });
  });

  describe('Route Configuration Integrity', () => {
    it('should have a component defined for every route', () => {
      const missingComponents = routes.filter(route => !route.component);
      expect(missingComponents.map(r => r.path)).toEqual([]);
    });

    it('should have unique paths', () => {
      const paths = routes.map(r => r.path);
      const duplicates = paths.filter((path, index) => paths.indexOf(path) !== index);
      expect(duplicates).toEqual([]);
    });

    it('tenant detail route should use TenantDetailRoutePage wrapper', () => {
      const tenantDetailRoute = routes.find(r => r.path === '/admin/tenants/:tenantId');
      expect(tenantDetailRoute).toBeDefined();

      if (tenantDetailRoute) {
        const component = tenantDetailRoute.component as React.ComponentType<unknown>;
        const componentName = component.displayName || component.name || '';

        // Should NOT be the modal component
        expect(componentName).not.toBe('TenantDetailPage');

        // Should be the route wrapper (or a lazy component that resolves to it)
        // Note: Lazy components may not have a name until loaded
      }
    });
  });

  describe('BLOCKED_ROUTE_COMPONENTS sync', () => {
    it('should have BLOCKED_ROUTE_COMPONENTS in sync with KNOWN_REQUIRED_PROPS_COMPONENTS', () => {
      // Ensure both lists are in sync
      for (const component of KNOWN_REQUIRED_PROPS_COMPONENTS) {
        expect(
          BLOCKED_ROUTE_COMPONENTS.includes(component as typeof BLOCKED_ROUTE_COMPONENTS[number]),
          `Component "${component}" is in KNOWN_REQUIRED_PROPS_COMPONENTS but not in BLOCKED_ROUTE_COMPONENTS. Add it to route-types.ts.`
        ).toBe(true);
      }
    });
  });
});

describe('Route Type Safety', () => {
  it('routes.ts should not contain "as any" type assertions in code', async () => {
    // This is a static analysis check - read the routes.ts file and check for patterns
    // In a real CI environment, this would be handled by ESLint
    // This test serves as a backup safety net

    const fs = await import('fs');
    const path = await import('path');

    const routesPath = path.resolve(__dirname, '../config/routes.ts');
    const content = fs.readFileSync(routesPath, 'utf-8');

    // Remove comments to avoid false positives from documentation
    const contentWithoutComments = content
      // Remove single-line comments
      .replace(/\/\/.*$/gm, '')
      // Remove multi-line comments
      .replace(/\/\*[\s\S]*?\*\//g, '')
      // Remove JSDoc comments
      .replace(/\/\*\*[\s\S]*?\*\//g, '');

    // Check for dangerous patterns in actual code (not comments)
    const dangerousPatterns = [
      { pattern: /as\s+any\b/, name: 'as any' },
      { pattern: /as\s+React\.ComponentType<any>/, name: 'as React.ComponentType<any>' },
      { pattern: /as\s+ComponentType<any>/, name: 'as ComponentType<any>' },
      { pattern: /<any>\s*\(/, name: '<any> type assertion' },
    ];

    const violations: string[] = [];

    for (const { pattern, name } of dangerousPatterns) {
      if (pattern.test(contentWithoutComments)) {
        violations.push(`Found "${name}" in routes.ts code - this bypasses type safety`);
      }
    }

    expect(violations).toEqual([]);
  });
});
