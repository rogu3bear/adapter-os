//! Role-Based Access Control (RBAC) helpers for AdapterOS UI
//!
//! Centralized RBAC logic to ensure consistent role checks across all UI components.
//! Roles are enforced server-side; this module provides UI-level checks for conditional rendering.

export type UserRole = 'admin' | 'operator' | 'compliance' | 'viewer';

export interface User {
  id: string;
  email: string;
  role: UserRole;
  tenant_id?: string;
}

/**
 * Role hierarchy (higher roles inherit permissions from lower roles)
 * Admin > Operator > Compliance > Viewer
 */
const ROLE_HIERARCHY: Record<UserRole, number> = {
  admin: 4,
  operator: 3,
  compliance: 2,
  viewer: 1,
};

/**
 * Check if user has a specific role
 */
export function hasRole(user: User | null | undefined, role: UserRole): boolean {
  if (!user) return false;
  return user.role === role;
}

/**
 * Check if user has any of the specified roles
 */
export function hasAnyRole(user: User | null | undefined, roles: UserRole[]): boolean {
  if (!user) return false;
  return roles.includes(user.role);
}

/**
 * Check if user has a role with at least the specified level
 * (e.g., hasRoleLevel(user, 'operator') returns true for admin and operator)
 */
export function hasRoleLevel(user: User | null | undefined, minRole: UserRole): boolean {
  if (!user) return false;
  const userLevel = ROLE_HIERARCHY[user.role] || 0;
  const minLevel = ROLE_HIERARCHY[minRole] || 0;
  return userLevel >= minLevel;
}

/**
 * Check if user can perform admin operations
 */
export function canAdmin(user: User | null | undefined): boolean {
  return hasRole(user, 'admin');
}

/**
 * Check if user can perform operator operations
 */
export function canOperate(user: User | null | undefined): boolean {
  return hasRoleLevel(user, 'operator');
}

/**
 * Check if user can view compliance data
 */
export function canViewCompliance(user: User | null | undefined): boolean {
  return hasAnyRole(user, ['admin', 'operator', 'compliance']);
}

/**
 * Check if user can modify tenants
 */
export function canModifyTenants(user: User | null | undefined): boolean {
  return hasRole(user, 'admin');
}

/**
 * Check if user can manage adapters
 */
export function canManageAdapters(user: User | null | undefined): boolean {
  return hasRoleLevel(user, 'operator');
}

/**
 * Check if user can view audit logs
 */
export function canViewAudits(user: User | null | undefined): boolean {
  return hasAnyRole(user, ['admin', 'operator', 'compliance']);
}

/**
 * Check if user can export telemetry bundles
 */
export function canExportTelemetry(user: User | null | undefined): boolean {
  return hasAnyRole(user, ['admin', 'operator', 'compliance']);
}

/**
 * Check if user can promote CPIDs
 */
export function canPromote(user: User | null | undefined): boolean {
  return hasRole(user, 'admin');
}

/**
 * Get human-readable role name
 */
export function getRoleName(role: UserRole): string {
  const names: Record<UserRole, string> = {
    admin: 'Administrator',
    operator: 'Operator',
    compliance: 'Compliance Officer',
    viewer: 'Viewer',
  };
  return names[role] || role;
}

/**
 * Get role description
 */
export function getRoleDescription(role: UserRole): string {
  const descriptions: Record<UserRole, string> = {
    admin: 'Full system access including tenant management and CPID promotion',
    operator: 'Can manage adapters, workers, and perform inference operations',
    compliance: 'Can view audit logs, telemetry bundles, and compliance reports',
    viewer: 'Read-only access to system status and metrics',
  };
  return descriptions[role] || '';
}

