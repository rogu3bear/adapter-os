/**
 * RBAC Hook Mock Factories
 *
 * Factory functions for creating mock useRBAC hook return values.
 */

import { vi, type Mock } from 'vitest';
import type { UserRole } from '@/api/types';

/**
 * Return type for useRBAC hook mock
 */
export interface UseRBACMockReturn {
  can: Mock<[string], boolean>;
  userRole: UserRole;
  permissions: string[];
  hasRole: Mock<[string], boolean>;
}

/**
 * Options for createUseRBACMock factory
 */
export interface UseRBACMockOptions {
  /** User role (default: 'admin') */
  userRole?: UserRole;
  /** Permissions array (default: ['*']) */
  permissions?: string[];
  /** Default return value for can() (default: true) */
  canDefault?: boolean;
  /** Specific permission overrides for can() */
  canOverrides?: Record<string, boolean>;
}

/**
 * Create a mock return value for useRBAC hook
 *
 * @example
 * ```typescript
 * // Admin with all permissions
 * const admin = createUseRBACMock();
 *
 * // Viewer with limited permissions
 * const viewer = createUseRBACMock({
 *   userRole: 'viewer',
 *   permissions: ['read:adapters', 'read:stacks'],
 *   canDefault: false,
 *   canOverrides: {
 *     'read:adapters': true,
 *     'write:adapters': false,
 *   },
 * });
 *
 * // Test permission check
 * const rbac = createUseRBACMock();
 * rbac.can('admin:users'); // returns true (default)
 * ```
 */
export function createUseRBACMock(options: UseRBACMockOptions = {}): UseRBACMockReturn {
  const userRole = options.userRole ?? 'admin';
  const permissions = options.permissions ?? ['*'];
  const canDefault = options.canDefault ?? true;
  const canOverrides = options.canOverrides ?? {};

  const can = vi.fn().mockImplementation((permission: string) => {
    // Check specific overrides first
    if (permission in canOverrides) {
      return canOverrides[permission];
    }
    // Check if permission is in the list
    if (permissions.includes(permission) || permissions.includes('*')) {
      return true;
    }
    // Fall back to default
    return canDefault;
  });

  const hasRole = vi.fn().mockImplementation((role: string) => role === userRole);

  return {
    can,
    userRole,
    permissions,
    hasRole,
  };
}
