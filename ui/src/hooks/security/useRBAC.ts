/**
 * useRBAC Hook - Role-Based Access Control utilities for React components
 *
 * Provides easy access to role checking and permission verification within components.
 */

import { useAuth } from '@/providers/CoreProviders';
import type { UserRole } from '@/api/types';
import {
  hasPermission,
  hasAnyPermission,
  hasAllPermissions,
  hasRole,
  getUserPermissions,
} from '@/utils/rbac';

/**
 * Hook for accessing user role and permission information
 *
 * @returns Object with role checking utilities
 *
 * @example
 * const { userRole, can } = useRBAC();
 *
 * if (can('adapter:register')) {
 *   return <RegisterAdapterButton />;
 * }
 */
export function useRBAC() {
  const { user } = useAuth();
  const userRole = user?.role;

  return {
    /**
     * The current user's role
     */
    userRole,

    /**
     * Check if user has a specific permission
     * @param permission - The permission to check
     * @returns true if user has the permission
     */
    can: (permission: string): boolean => hasPermission(userRole, permission),

    /**
     * Check if user has any of the specified permissions
     * @param permissions - Array of permissions to check
     * @returns true if user has at least one permission
     */
    canAny: (permissions: string[]): boolean => hasAnyPermission(userRole, permissions),

    /**
     * Check if user has all of the specified permissions
     * @param permissions - Array of permissions to check
     * @returns true if user has all permissions
     */
    canAll: (permissions: string[]): boolean => hasAllPermissions(userRole, permissions),

    /**
     * Check if user has one of the specified roles
     * @param roles - Array of roles to check
     * @returns true if user has one of the roles
     */
    hasRole: (roles: UserRole[]): boolean => hasRole(userRole, roles),

    /**
     * Get all permissions for the current user
     * @returns Array of permission strings
     */
    getPermissions: (): string[] => getUserPermissions(userRole),

    /**
     * Check if user is authenticated
     */
    isAuthenticated: (): boolean => !!user,

    /**
     * Get the current user object (can be undefined)
     */
    getUser: () => user,
  };
}

/**
 * Hook to check if user can access certain routes
 *
 * @param requiredRoles - Array of roles that can access
 * @returns true if current user can access
 *
 * @example
 * const canAccessAdminPanel = useCanAccess(['admin']);
 */
export function useCanAccess(requiredRoles: UserRole[]): boolean {
  const { userRole, hasRole: userHasRole } = useRBAC();

  if (requiredRoles.length === 0) {
    return true;
  }

  if (!userRole) {
    return false;
  }

  return userHasRole(requiredRoles);
}

/**
 * Hook to get a reason why user cannot access something
 *
 * @param requiredRoles - Array of required roles
 * @param permission - Optional permission to check
 * @returns Message explaining why access is denied, or null if access is granted
 *
 * @example
 * const reason = useAccessDenialReason(['admin', 'operator']);
 * if (reason) {
 *   return <ErrorMessage>{reason}</ErrorMessage>;
 * }
 */
export function useAccessDenialReason(
  requiredRoles?: UserRole[],
  permission?: string
): string | null {
  const { userRole, can } = useRBAC();

  if (!userRole) {
    return 'You must be logged in to access this resource';
  }

  if (permission && !can(permission)) {
    return `Your role (${userRole}) does not have the required permission`;
  }

  if (requiredRoles && requiredRoles.length > 0) {
    const hasRequiredRole = requiredRoles.some(
      role => role.toLowerCase() === userRole.toLowerCase()
    );

    if (!hasRequiredRole) {
      return `Your role (${userRole}) does not have access to this resource. Required roles: ${requiredRoles.join(', ')}`;
    }
  }

  return null;
}
