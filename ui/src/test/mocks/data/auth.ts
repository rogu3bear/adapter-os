/**
 * Auth Data Mock Factories
 *
 * Factory functions for creating mock User, Tenant, and related auth data.
 */

import type { User, UserRole, TenantSummary } from '@/api/types';

/**
 * Create a mock User object with sensible defaults
 *
 * @example
 * ```typescript
 * const user = createMockUser(); // Complete default user
 * const viewer = createMockUser({ role: 'viewer' }); // Override role
 * const noTenant = createMockUser({ tenant_id: undefined }); // No tenant
 * ```
 */
export function createMockUser(overrides: Partial<User> = {}): User {
  return {
    id: 'user-1',
    user_id: 'user-1',
    email: 'test@example.com',
    display_name: 'Test User',
    name: 'Test User',
    role: 'admin' as UserRole,
    tenant_id: 'test-tenant',
    permissions: ['*'],
    mfa_enabled: false,
    admin_tenants: undefined,
    ...overrides,
  };
}

/**
 * Create a mock TenantSummary object
 *
 * @example
 * ```typescript
 * const tenant = createMockTenant(); // Default tenant
 * const custom = createMockTenant({ id: 'my-tenant', name: 'My Org' });
 * ```
 */
export function createMockTenant(overrides: Partial<TenantSummary> = {}): TenantSummary {
  return {
    schema_version: '1.0',
    id: 'test-tenant',
    name: 'Test Tenant',
    status: 'active',
    created_at: new Date().toISOString(),
    ...overrides,
  };
}

/**
 * Create multiple mock tenants for multi-tenant testing
 *
 * @example
 * ```typescript
 * const tenants = createMockTenants(3); // Creates tenant-1, tenant-2, tenant-3
 * ```
 */
export function createMockTenants(count: number): TenantSummary[] {
  return Array.from({ length: count }, (_, i) =>
    createMockTenant({
      id: `tenant-${i + 1}`,
      name: `Tenant ${i + 1}`,
    })
  );
}
