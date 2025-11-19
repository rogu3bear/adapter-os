/**
 * Role-Based Access Control (RBAC) utilities
 *
 * Provides helper functions for managing role-based permissions throughout the UI.
 * All role comparisons are case-insensitive for robustness.
 */

import type { UserRole } from '@/api/types';

/**
 * RBAC Role definitions
 */
export const ROLES = {
  ADMIN: 'admin' as const,
  OPERATOR: 'operator' as const,
  SRE: 'sre' as const,
  COMPLIANCE: 'compliance' as const,
  AUDITOR: 'auditor' as const,
  VIEWER: 'viewer' as const,
} as const;

/**
 * Permission categories used for RBAC
 */
export const PERMISSIONS = {
  // Adapter Management
  ADAPTER_LIST: 'adapter:list',
  ADAPTER_VIEW: 'adapter:view',
  ADAPTER_REGISTER: 'adapter:register',
  ADAPTER_DELETE: 'adapter:delete',
  ADAPTER_LOAD: 'adapter:load',
  ADAPTER_UNLOAD: 'adapter:unload',

  // Training
  TRAINING_START: 'training:start',
  TRAINING_CANCEL: 'training:cancel',
  TRAINING_VIEW: 'training:view',

  // Policies
  POLICY_VIEW: 'policy:view',
  POLICY_APPLY: 'policy:apply',
  POLICY_VALIDATE: 'policy:validate',
  POLICY_SIGN: 'policy:sign',

  // Promotion
  PROMOTION_EXECUTE: 'promotion:execute',
  PROMOTION_VIEW: 'promotion:view',

  // Audit & Compliance
  AUDIT_VIEW: 'audit:view',
  COMPLIANCE_VIEW: 'compliance:view',

  // Infrastructure
  TENANT_MANAGE: 'tenant:manage',
  NODE_MANAGE: 'node:manage',
  WORKER_MANAGE: 'worker:manage',

  // Inference
  INFERENCE_EXECUTE: 'inference:execute',
} as const;

/**
 * Role to permissions mapping
 *
 * Defines which permissions each role has access to.
 * This serves as the source of truth for RBAC throughout the app.
 */
export const ROLE_PERMISSIONS: Record<UserRole, string[]> = {
  admin: [
    // Admin has all permissions
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.ADAPTER_REGISTER,
    PERMISSIONS.ADAPTER_DELETE,
    PERMISSIONS.ADAPTER_LOAD,
    PERMISSIONS.ADAPTER_UNLOAD,
    PERMISSIONS.TRAINING_START,
    PERMISSIONS.TRAINING_CANCEL,
    PERMISSIONS.TRAINING_VIEW,
    PERMISSIONS.POLICY_VIEW,
    PERMISSIONS.POLICY_APPLY,
    PERMISSIONS.POLICY_VALIDATE,
    PERMISSIONS.POLICY_SIGN,
    PERMISSIONS.PROMOTION_EXECUTE,
    PERMISSIONS.PROMOTION_VIEW,
    PERMISSIONS.AUDIT_VIEW,
    PERMISSIONS.COMPLIANCE_VIEW,
    PERMISSIONS.TENANT_MANAGE,
    PERMISSIONS.NODE_MANAGE,
    PERMISSIONS.WORKER_MANAGE,
    PERMISSIONS.INFERENCE_EXECUTE,
  ],
  operator: [
    // Operator can manage adapters and run training/inference
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.ADAPTER_REGISTER,
    PERMISSIONS.ADAPTER_LOAD,
    PERMISSIONS.ADAPTER_UNLOAD,
    PERMISSIONS.TRAINING_START,
    PERMISSIONS.TRAINING_CANCEL,
    PERMISSIONS.TRAINING_VIEW,
    PERMISSIONS.POLICY_VIEW,
    PERMISSIONS.PROMOTION_VIEW,
    PERMISSIONS.INFERENCE_EXECUTE,
    PERMISSIONS.WORKER_MANAGE,
  ],
  sre: [
    // SRE can view system information and debug
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.AUDIT_VIEW,
    PERMISSIONS.COMPLIANCE_VIEW,
    PERMISSIONS.TRAINING_VIEW,
    PERMISSIONS.PROMOTION_VIEW,
    PERMISSIONS.POLICY_VIEW,
    PERMISSIONS.WORKER_MANAGE,
    PERMISSIONS.NODE_MANAGE,
  ],
  compliance: [
    // Compliance can view audit and compliance information
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.AUDIT_VIEW,
    PERMISSIONS.COMPLIANCE_VIEW,
    PERMISSIONS.POLICY_VIEW,
    PERMISSIONS.PROMOTION_VIEW,
  ],
  auditor: [
    // Auditor is read-only for audit information
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.AUDIT_VIEW,
  ],
  viewer: [
    // Viewer is read-only
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
  ],
};

/**
 * Check if a user has a specific permission
 *
 * @param userRole - The user's role
 * @param permission - The permission to check
 * @returns true if the user has the permission, false otherwise
 */
export function hasPermission(userRole: UserRole | undefined, permission: string): boolean {
  if (!userRole) {
    return false;
  }

  const permissions = ROLE_PERMISSIONS[userRole.toLowerCase() as UserRole];
  return permissions ? permissions.includes(permission) : false;
}

/**
 * Check if a user has any of the specified permissions
 *
 * @param userRole - The user's role
 * @param permissions - Array of permissions to check
 * @returns true if the user has at least one of the permissions, false otherwise
 */
export function hasAnyPermission(userRole: UserRole | undefined, permissions: string[]): boolean {
  return permissions.some(permission => hasPermission(userRole, permission));
}

/**
 * Check if a user has all of the specified permissions
 *
 * @param userRole - The user's role
 * @param permissions - Array of permissions to check
 * @returns true if the user has all of the permissions, false otherwise
 */
export function hasAllPermissions(userRole: UserRole | undefined, permissions: string[]): boolean {
  return permissions.every(permission => hasPermission(userRole, permission));
}

/**
 * Check if a user has one of the specified roles
 *
 * @param userRole - The user's role
 * @param requiredRoles - Array of roles to check against
 * @returns true if the user has one of the required roles, false otherwise
 */
export function hasRole(userRole: UserRole | undefined, requiredRoles: UserRole[]): boolean {
  if (!userRole || requiredRoles.length === 0) {
    return requiredRoles.length === 0;
  }

  const normalizedUserRole = userRole.toLowerCase();
  return requiredRoles.some(role => role.toLowerCase() === normalizedUserRole);
}

/**
 * Get all permissions for a user
 *
 * @param userRole - The user's role
 * @returns Array of permissions the user has
 */
export function getUserPermissions(userRole: UserRole | undefined): string[] {
  if (!userRole) {
    return [];
  }

  return ROLE_PERMISSIONS[userRole.toLowerCase() as UserRole] || [];
}

/**
 * Check if a role can perform a specific action
 *
 * Convenience wrapper for checking if a role (not a specific user) can do something.
 * Useful for determining UI element visibility based on required roles.
 *
 * @param role - The role to check
 * @param permission - The permission to verify
 * @returns true if the role has the permission, false otherwise
 */
export function roleCanPerform(role: UserRole | undefined, permission: string): boolean {
  return hasPermission(role, permission);
}

/**
 * Get a human-readable description of a permission
 *
 * @param permission - The permission key
 * @returns Human-readable description
 */
export function getPermissionDescription(permission: string): string {
  const descriptions: Record<string, string> = {
    'adapter:list': 'View adapters',
    'adapter:view': 'View adapter details',
    'adapter:register': 'Register new adapters',
    'adapter:delete': 'Delete adapters',
    'adapter:load': 'Load adapters',
    'adapter:unload': 'Unload adapters',
    'training:start': 'Start training jobs',
    'training:cancel': 'Cancel training jobs',
    'training:view': 'View training jobs',
    'policy:view': 'View policies',
    'policy:apply': 'Apply policies',
    'policy:validate': 'Validate policies',
    'policy:sign': 'Sign policies',
    'promotion:execute': 'Execute promotions',
    'promotion:view': 'View promotions',
    'audit:view': 'View audit logs',
    'compliance:view': 'View compliance information',
    'tenant:manage': 'Manage tenants',
    'node:manage': 'Manage nodes',
    'worker:manage': 'Manage workers',
    'inference:execute': 'Execute inference',
  };

  return descriptions[permission] || permission;
}
