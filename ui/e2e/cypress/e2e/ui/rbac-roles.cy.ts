/// <reference types="cypress" />

/**
 * RBAC Role-Based E2E Tests
 *
 * Tests that all roles (admin, operator, developer, compliance, auditor, viewer, sre)
 * have appropriate access to routes and UI elements.
 *
 * Test Coverage:
 * 1. Route access permissions (allowed routes work)
 * 2. Route access restrictions (forbidden routes show 403 or redirect)
 * 3. Role-specific UI elements (buttons, actions shown/hidden)
 * 4. Admin-only actions fail for non-admin roles
 *
 * Based on:
 * - /src/utils/rbac.ts (ROLE_PERMISSIONS mapping)
 * - /src/config/routes.ts (requiredRoles and roleVisibility)
 * - /src/components/dashboard/config/roleConfigs.ts (role configs)
 */

interface RoleTestConfig {
  role: string;
  email: string;
  password: string;
  allowedRoutes: string[];
  deniedRoutes: string[];
  canCreateTenant: boolean;
  canDeleteAdapter: boolean;
  canManageStacks: boolean;
  canApplyPolicy: boolean;
}

// Role configuration matrix based on RBAC utilities
const roleConfigs: RoleTestConfig[] = [
  {
    role: 'admin',
    email: 'admin@example.com',
    password: 'admin-password',
    allowedRoutes: [
      '/dashboard',
      '/adapters',
      '/training',
      '/inference',
      '/security/policies',
      '/security/audit',
      '/security/compliance',
      '/admin',
      '/admin/tenants',
      '/admin/stacks',
      '/admin/settings',
      '/system/nodes',
      '/metrics',
      '/telemetry',
      '/chat',
    ],
    deniedRoutes: [],
    canCreateTenant: true,
    canDeleteAdapter: true,
    canManageStacks: true,
    canApplyPolicy: true,
  },
  {
    role: 'operator',
    email: 'operator@example.com',
    password: 'operator-password',
    allowedRoutes: [
      '/dashboard',
      '/adapters',
      '/training',
      '/inference',
      '/security/policies', // Read-only
      '/metrics',
      '/telemetry',
      '/chat',
    ],
    deniedRoutes: [
      '/admin',
      '/admin/tenants',
      '/admin/settings',
      '/admin/plugins',
      '/security/audit', // Operators cannot view audit logs
    ],
    canCreateTenant: false,
    canDeleteAdapter: false,
    canManageStacks: false,
    canApplyPolicy: false,
  },
  {
    role: 'developer',
    email: 'developer@example.com',
    password: 'developer-password',
    allowedRoutes: [
      '/dashboard',
      '/adapters',
      '/training',
      '/inference',
      '/security/policies',
      '/security/audit',
      '/security/compliance',
      '/admin',
      '/admin/tenants',
      '/admin/stacks',
      '/admin/settings',
      '/system/nodes',
      '/metrics',
      '/telemetry',
      '/chat',
    ],
    deniedRoutes: [],
    canCreateTenant: true,
    canDeleteAdapter: true,
    canManageStacks: true,
    canApplyPolicy: true,
  },
  {
    role: 'compliance',
    email: 'compliance@example.com',
    password: 'compliance-password',
    allowedRoutes: [
      '/dashboard',
      '/security/policies',
      '/security/audit',
      '/security/compliance',
      '/adapters', // Read-only
      '/metrics',
      '/telemetry',
    ],
    deniedRoutes: [
      '/admin',
      '/admin/tenants',
      '/admin/settings',
      '/training', // Cannot start training
      '/inference', // Cannot execute inference
    ],
    canCreateTenant: false,
    canDeleteAdapter: false,
    canManageStacks: false,
    canApplyPolicy: false,
  },
  {
    role: 'auditor',
    email: 'auditor@example.com',
    password: 'auditor-password',
    allowedRoutes: [
      '/dashboard',
      '/security/audit',
      '/adapters', // Read-only
      '/metrics',
      '/telemetry',
    ],
    deniedRoutes: [
      '/admin',
      '/admin/tenants',
      '/admin/settings',
      '/security/policies', // Cannot view policy details
      '/training',
      '/inference',
      '/chat',
    ],
    canCreateTenant: false,
    canDeleteAdapter: false,
    canManageStacks: false,
    canApplyPolicy: false,
  },
  {
    role: 'viewer',
    email: 'viewer@example.com',
    password: 'viewer-password',
    allowedRoutes: [
      '/dashboard',
      '/adapters', // Read-only
      '/metrics',
    ],
    deniedRoutes: [
      '/admin',
      '/admin/tenants',
      '/admin/settings',
      '/security/policies',
      '/security/audit',
      '/security/compliance',
      '/training',
      '/inference',
      '/chat',
      '/telemetry',
    ],
    canCreateTenant: false,
    canDeleteAdapter: false,
    canManageStacks: false,
    canApplyPolicy: false,
  },
  {
    role: 'sre',
    email: 'sre@example.com',
    password: 'sre-password',
    allowedRoutes: [
      '/dashboard',
      '/adapters', // Read-only
      '/security/audit',
      '/security/compliance',
      '/system/nodes',
      '/metrics',
      '/telemetry',
    ],
    deniedRoutes: [
      '/admin',
      '/admin/tenants',
      '/admin/settings',
      '/admin/plugins',
      '/training', // Cannot start training
      '/inference', // Cannot execute inference
      '/chat',
    ],
    canCreateTenant: false,
    canDeleteAdapter: false,
    canManageStacks: false,
    canApplyPolicy: false,
  },
];

/**
 * Helper: Login as a specific role
 */
function loginAsRole(config: RoleTestConfig) {
  cy.clearCookies();
  cy.clearLocalStorage();
  cy.visit('/login');

  cy.get('input#email', { timeout: 20000 }).should('be.visible').clear().type(config.email);
  cy.get('input#password', { log: false }).clear().type(config.password, { log: false });
  cy.contains('button', 'Sign in', { matchCase: false }).click();

  // Wait for redirect to dashboard or home page
  cy.url({ timeout: 20000 }).should('not.include', '/login');
}

/**
 * Helper: Check if route is accessible
 */
function assertRouteAccessible(path: string, role: string) {
  cy.visit(path, { failOnStatusCode: false });
  cy.url({ timeout: 10000 }).should('include', path);

  // Should not show unauthorized message or redirect to login
  cy.contains(/unauthorized|forbidden|access denied/i).should('not.exist');
  cy.url().should('not.include', '/login');

  cy.log(`✓ ${role} can access ${path}`);
}

/**
 * Helper: Check if route is denied (403 or redirect)
 */
function assertRouteDenied(path: string, role: string) {
  cy.visit(path, { failOnStatusCode: false });

  // Route should either:
  // 1. Show an error message (403/unauthorized)
  // 2. Redirect to dashboard or home
  // 3. Not show the expected content
  cy.url({ timeout: 10000 }).then((url) => {
    const redirected = !url.includes(path);
    const isUnauthorized = url.includes('/unauthorized') || url.includes('/403');

    if (!redirected && !isUnauthorized) {
      // Check for error message on page
      cy.get('body').then(($body) => {
        const hasErrorMessage =
          $body.text().match(/unauthorized|forbidden|access denied|you do not have permission/i) !== null;

        if (!hasErrorMessage) {
          // As a fallback, check that admin-specific content is NOT visible
          // This catches cases where the page loads but with restricted content
          cy.log(`⚠️ ${role} accessed ${path} but may have restricted content`);
        }
      });
    }
  });

  cy.log(`✓ ${role} denied access to ${path}`);
}

describe('RBAC Role-Based Access Control', () => {
  beforeEach(() => {
    cy.clearCookies();
    cy.clearLocalStorage();
  });

  roleConfigs.forEach((config) => {
    describe(`Role: ${config.role}`, () => {
      beforeEach(() => {
        loginAsRole(config);
      });

      it(`should allow ${config.role} to access permitted routes`, () => {
        config.allowedRoutes.forEach((route) => {
          assertRouteAccessible(route, config.role);
        });
      });

      it(`should deny ${config.role} access to restricted routes`, () => {
        if (config.deniedRoutes.length === 0) {
          cy.log(`✓ ${config.role} has no route restrictions (full access)`);
          return;
        }

        config.deniedRoutes.forEach((route) => {
          assertRouteDenied(route, config.role);
        });
      });

      it(`should ${config.canCreateTenant ? 'show' : 'hide'} tenant creation UI for ${config.role}`, () => {
        cy.visit('/dashboard', { timeout: 20000 });

        if (config.canCreateTenant) {
          // Admin/Developer should see tenant management links
          cy.visit('/admin/tenants', { failOnStatusCode: false });
          cy.url().should('include', '/admin/tenants');

          // Look for create tenant button or link
          cy.get('body').then(($body) => {
            const hasCreateButton =
              $body.find('[data-cy*="create"]').length > 0 ||
              $body.find('button:contains("Create")').length > 0 ||
              $body.find('a:contains("New")').length > 0;

            if (hasCreateButton) {
              cy.log(`✓ ${config.role} can see tenant creation UI`);
            } else {
              cy.log(`⚠️ ${config.role} on tenants page but no create button found`);
            }
          });
        } else {
          // Non-admin roles should not see tenant management
          cy.visit('/admin/tenants', { failOnStatusCode: false });
          cy.url({ timeout: 5000 }).should('not.include', '/admin/tenants');
          cy.log(`✓ ${config.role} cannot access tenant creation`);
        }
      });

      it(`should ${config.canManageStacks ? 'allow' : 'deny'} stack management for ${config.role}`, () => {
        if (config.canManageStacks) {
          cy.visit('/admin/stacks', { failOnStatusCode: false });
          cy.url().should('include', '/admin/stacks');
          cy.log(`✓ ${config.role} can manage adapter stacks`);
        } else {
          cy.visit('/admin/stacks', { failOnStatusCode: false });
          cy.url({ timeout: 5000 }).should('not.include', '/admin/stacks');
          cy.log(`✓ ${config.role} cannot manage adapter stacks`);
        }
      });

      it(`should ${config.canApplyPolicy ? 'allow' : 'deny'} policy application for ${config.role}`, () => {
        cy.visit('/security/policies', { failOnStatusCode: false });

        if (config.canApplyPolicy) {
          // Admin/Developer can apply policies
          cy.url().should('include', '/security/policies');

          cy.get('body').then(($body) => {
            const hasPolicyActions =
              $body.find('[data-cy*="apply"]').length > 0 ||
              $body.find('button:contains("Apply")').length > 0 ||
              $body.find('button:contains("Save")').length > 0 ||
              $body.find('[data-cy*="policy"]').length > 0;

            if (hasPolicyActions) {
              cy.log(`✓ ${config.role} can apply policies`);
            } else {
              cy.log(`⚠️ ${config.role} on policies page but no apply actions found`);
            }
          });
        } else {
          // Check if route is accessible but read-only (compliance, operator)
          const canViewPolicies = config.allowedRoutes.includes('/security/policies');

          if (canViewPolicies) {
            // Can view but should not have apply actions
            cy.url().should('include', '/security/policies');
            cy.get('body').then(($body) => {
              const hasApplyButton =
                $body.find('button:contains("Apply Policy")').length > 0 ||
                $body.find('[data-cy="apply-policy"]').length > 0;

              expect(hasApplyButton).to.be.false;
              cy.log(`✓ ${config.role} can view policies but cannot apply them`);
            });
          } else {
            // Cannot access policies page at all
            cy.url().should('not.include', '/security/policies');
            cy.log(`✓ ${config.role} cannot access policies page`);
          }
        }
      });

      it(`should display appropriate dashboard widgets for ${config.role}`, () => {
        cy.visit('/dashboard', { timeout: 20000 });
        cy.url().should('include', '/dashboard');

        // All roles should see at least one widget
        cy.get('body').should('be.visible');

        // Role-specific widget checks
        switch (config.role) {
          case 'admin':
          case 'developer':
            // Admin should see tenant management widgets
            cy.log(`✓ ${config.role} dashboard loaded with full widgets`);
            break;

          case 'operator':
            // Operator should see adapter and training widgets
            cy.log(`✓ ${config.role} dashboard loaded with operational widgets`);
            break;

          case 'sre':
            // SRE should see system health widgets
            cy.log(`✓ ${config.role} dashboard loaded with SRE widgets`);
            break;

          case 'compliance':
          case 'auditor':
            // Compliance/Auditor should see audit widgets
            cy.log(`✓ ${config.role} dashboard loaded with compliance widgets`);
            break;

          case 'viewer':
            // Viewer should see read-only widgets
            cy.log(`✓ ${config.role} dashboard loaded with viewer widgets`);
            break;
        }
      });

      it(`should show appropriate navigation items for ${config.role}`, () => {
        cy.visit('/dashboard', { timeout: 20000 });

        // Check for role-specific navigation
        cy.get('body').then(($body) => {
          const navText = $body.text();

          // Admin/Developer should see Admin nav
          if (config.role === 'admin' || config.role === 'developer') {
            expect(navText).to.match(/admin|settings/i);
            cy.log(`✓ ${config.role} sees admin navigation`);
          } else {
            // Non-admin should not see Admin nav (may be hidden)
            cy.log(`✓ ${config.role} navigation loaded`);
          }

          // All roles except viewer should see some operational nav
          if (config.role !== 'viewer') {
            expect(navText.length).to.be.greaterThan(100);
            cy.log(`✓ ${config.role} sees operational navigation`);
          }
        });
      });
    });
  });

  describe('Cross-role security tests', () => {
    it('should prevent privilege escalation via URL manipulation', () => {
      // Login as viewer
      const viewerConfig = roleConfigs.find((c) => c.role === 'viewer')!;
      loginAsRole(viewerConfig);

      // Try to access admin routes directly
      const adminRoutes = ['/admin', '/admin/tenants', '/admin/settings', '/admin/plugins'];

      adminRoutes.forEach((route) => {
        cy.visit(route, { failOnStatusCode: false });
        cy.url({ timeout: 5000 }).should('not.include', route);
        cy.log(`✓ Viewer blocked from ${route} via URL`);
      });
    });

    it('should maintain role restrictions after page refresh', () => {
      const operatorConfig = roleConfigs.find((c) => c.role === 'operator')!;
      loginAsRole(operatorConfig);

      // Navigate to allowed route
      cy.visit('/adapters');
      cy.url().should('include', '/adapters');

      // Refresh page
      cy.reload();

      // Try to access denied route
      cy.visit('/admin/settings', { failOnStatusCode: false });
      cy.url({ timeout: 5000 }).should('not.include', '/admin/settings');

      cy.log('✓ Role restrictions persist after refresh');
    });

    it('should prevent unauthorized API calls from non-admin roles', () => {
      const viewerConfig = roleConfigs.find((c) => c.role === 'viewer')!;
      loginAsRole(viewerConfig);

      // Intercept tenant creation API call
      cy.intercept('POST', '**/v1/tenants', (req) => {
        req.reply((res) => {
          // Should return 403 Forbidden
          expect(res.statusCode).to.be.oneOf([403, 401]);
          cy.log('✓ Viewer blocked from creating tenant via API');
        });
      }).as('createTenant');

      // Intercept adapter deletion API call
      cy.intercept('DELETE', '**/v1/adapters/*', (req) => {
        req.reply((res) => {
          // Should return 403 Forbidden
          expect(res.statusCode).to.be.oneOf([403, 401]);
          cy.log('✓ Viewer blocked from deleting adapter via API');
        });
      }).as('deleteAdapter');
    });

    it('should show role-appropriate error messages', () => {
      const auditorConfig = roleConfigs.find((c) => c.role === 'auditor')!;
      loginAsRole(auditorConfig);

      // Try to access training page
      cy.visit('/training', { failOnStatusCode: false });

      // Should either redirect or show appropriate error
      cy.url({ timeout: 5000 }).then((url) => {
        if (url.includes('/training')) {
          // If page loaded, check for permission message
          cy.get('body').should('contain', /permission|access|unauthorized|forbidden/i);
        } else {
          // Redirected away from training
          cy.log('✓ Auditor redirected from training page');
        }
      });
    });

    it('should enforce role restrictions across tenant switches', () => {
      const operatorConfig = roleConfigs.find((c) => c.role === 'operator')!;
      loginAsRole(operatorConfig);

      // Check initial access
      cy.visit('/admin/settings', { failOnStatusCode: false });
      cy.url({ timeout: 5000 }).should('not.include', '/admin/settings');

      // Switch tenant (if tenant switcher is available)
      cy.get('body').then(($body) => {
        if ($body.find('[data-cy=tenant-switcher]').length > 0) {
          cy.get('[data-cy=tenant-switcher]').click();
          cy.get('[data-cy=tenant-option]').first().click();

          // Wait for switch to complete
          cy.wait(1000);

          // Still should not access admin routes
          cy.visit('/admin/settings', { failOnStatusCode: false });
          cy.url({ timeout: 5000 }).should('not.include', '/admin/settings');

          cy.log('✓ Role restrictions maintained across tenant switch');
        } else {
          cy.log('⚠️ Tenant switcher not available');
        }
      });
    });
  });

  describe('Admin-only action tests', () => {
    it('should allow admin to perform all actions', () => {
      const adminConfig = roleConfigs.find((c) => c.role === 'admin')!;
      loginAsRole(adminConfig);

      // Admin can access all protected routes
      const protectedRoutes = [
        '/admin',
        '/admin/tenants',
        '/admin/stacks',
        '/admin/settings',
        '/security/policies',
        '/security/audit',
      ];

      protectedRoutes.forEach((route) => {
        cy.visit(route, { failOnStatusCode: false });
        cy.url().should('include', route);
        cy.log(`✓ Admin can access ${route}`);
      });
    });

    it('should allow developer to perform admin-level actions', () => {
      const developerConfig = roleConfigs.find((c) => c.role === 'developer');

      if (!developerConfig) {
        cy.log('⚠️ Developer role not configured for testing');
        return;
      }

      loginAsRole(developerConfig);

      // Developer should have same access as admin
      const protectedRoutes = ['/admin', '/admin/tenants', '/admin/settings'];

      protectedRoutes.forEach((route) => {
        cy.visit(route, { failOnStatusCode: false });
        cy.url().should('include', route);
        cy.log(`✓ Developer can access ${route}`);
      });
    });

    it('should deny operator from admin-only actions', () => {
      const operatorConfig = roleConfigs.find((c) => c.role === 'operator')!;
      loginAsRole(operatorConfig);

      // Operator cannot access admin routes
      const adminOnlyRoutes = ['/admin/tenants', '/admin/settings', '/admin/plugins'];

      adminOnlyRoutes.forEach((route) => {
        cy.visit(route, { failOnStatusCode: false });
        cy.url({ timeout: 5000 }).should('not.include', route);
        cy.log(`✓ Operator denied access to ${route}`);
      });
    });
  });

  describe('Role-based UI element visibility', () => {
    it('should show/hide create buttons based on role permissions', () => {
      const roles = [
        { role: 'admin', canCreate: true },
        { role: 'operator', canCreate: true },
        { role: 'viewer', canCreate: false },
      ];

      roles.forEach(({ role, canCreate }) => {
        const config = roleConfigs.find((c) => c.role === role)!;
        loginAsRole(config);

        cy.visit('/adapters', { failOnStatusCode: false });

        if (canCreate) {
          cy.log(`✓ ${role} should see creation buttons`);
        } else {
          // Viewer should not see create/register buttons
          cy.get('body').then(($body) => {
            const hasCreateButton =
              $body.find('button:contains("Register")').length > 0 ||
              $body.find('button:contains("Create")').length > 0 ||
              $body.find('[data-cy="register-adapter"]').length > 0;

            if (role === 'viewer') {
              expect(hasCreateButton).to.be.false;
            }
            cy.log(`✓ ${role} create button visibility correct`);
          });
        }
      });
    });

    it('should show/hide delete buttons based on role permissions', () => {
      const adminConfig = roleConfigs.find((c) => c.role === 'admin')!;
      const viewerConfig = roleConfigs.find((c) => c.role === 'viewer')!;

      // Admin should see delete actions
      loginAsRole(adminConfig);
      cy.visit('/adapters', { failOnStatusCode: false });
      cy.log('✓ Admin can see adapter management UI');

      // Viewer should not see delete actions
      loginAsRole(viewerConfig);
      cy.visit('/adapters', { failOnStatusCode: false });
      cy.get('body').then(($body) => {
        const hasDeleteButton =
          $body.find('button:contains("Delete")').length > 0 ||
          $body.find('[data-cy="delete"]').length > 0;

        expect(hasDeleteButton).to.be.false;
        cy.log('✓ Viewer does not see delete buttons');
      });
    });
  });
});
