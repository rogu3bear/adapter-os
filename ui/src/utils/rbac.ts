/**
 * Role-Based Access Control (RBAC) utilities
 *
 * Provides helper functions for managing role-based permissions throughout the UI.
 * All role comparisons are case-insensitive for robustness.
 */

import type { UserRole } from '@/api/types';

// Re-export UserRole for convenience
export type { UserRole };

/**
 * User interface for RBAC checks
 */
export interface User {
  id: string;
  email: string;
  role: UserRole;
  tenant_id?: string;
}

/**
 * Role hierarchy (higher roles inherit permissions from lower roles)
 * Used for hasRoleLevel checks
 */
const ROLE_HIERARCHY: Record<string, number> = {
  developer: 7, // Highest - full access to everything
  admin: 6,
  operator: 5,
  sre: 4,
  compliance: 3,
  auditor: 2,
  viewer: 1,
};

/**
 * RBAC Role definitions
 */
export const ROLES = {
  DEVELOPER: 'developer' as const,
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
  TRAINING_VIEW_LOGS: 'training:view-logs',

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
  NODE_VIEW: 'node:view',
  WORKER_MANAGE: 'worker:manage',
  WORKER_SPAWN: 'worker:spawn',
  WORKER_VIEW: 'worker:view',

  // Inference
  INFERENCE_EXECUTE: 'inference:execute',

  // Activity
  ACTIVITY_CREATE: 'activity:create',
  ACTIVITY_VIEW: 'activity:view',

  // Contacts
  CONTACT_MANAGE: 'contact:manage',
  CONTACT_VIEW: 'contact:view',

  // Notifications
  NOTIFICATION_MANAGE: 'notification:manage',
  NOTIFICATION_VIEW: 'notification:view',

  // Workspaces
  WORKSPACE_MANAGE: 'workspace:manage',
  WORKSPACE_MEMBER_MANAGE: 'workspace:member-manage',
  WORKSPACE_RESOURCE_MANAGE: 'workspace:resource-manage',
  WORKSPACE_VIEW: 'workspace:view',

  // Datasets
  DATASET_DELETE: 'dataset:delete',
  DATASET_UPLOAD: 'dataset:upload',
  DATASET_VALIDATE: 'dataset:validate',
  DATASET_VIEW: 'dataset:view',

  // Code Intelligence
  CODE_SCAN: 'code:scan',
  CODE_VIEW: 'code:view',

  // Federation
  FEDERATION_VIEW: 'federation:view',

  // Git
  GIT_MANAGE: 'git:manage',
  GIT_VIEW: 'git:view',

  // Monitoring & Metrics
  MONITORING_MANAGE: 'monitoring:manage',
  METRICS_VIEW: 'metrics:view',

  // Dashboard
  DASHBOARD_MANAGE: 'dashboard:manage',
  DASHBOARD_VIEW: 'dashboard:view',

  // Plans
  PLAN_VIEW: 'plan:view',

  // Replay
  REPLAY_MANAGE: 'replay:manage',

  // Telemetry
  TELEMETRY_VIEW: 'telemetry:view',
} as const;

/**
 * Role to permissions mapping
 *
 * Defines which permissions each role has access to.
 * This serves as the source of truth for RBAC throughout the app.
 */
export const ROLE_PERMISSIONS: Record<UserRole, string[]> = {
  developer: [
    // Developer has all permissions (super-role for full system access)
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.ADAPTER_REGISTER,
    PERMISSIONS.ADAPTER_DELETE,
    PERMISSIONS.ADAPTER_LOAD,
    PERMISSIONS.ADAPTER_UNLOAD,
    PERMISSIONS.TRAINING_START,
    PERMISSIONS.TRAINING_CANCEL,
    PERMISSIONS.TRAINING_VIEW,
    PERMISSIONS.TRAINING_VIEW_LOGS,
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
    PERMISSIONS.NODE_VIEW,
    PERMISSIONS.WORKER_MANAGE,
    PERMISSIONS.WORKER_SPAWN,
    PERMISSIONS.WORKER_VIEW,
    PERMISSIONS.INFERENCE_EXECUTE,
    PERMISSIONS.ACTIVITY_CREATE,
    PERMISSIONS.ACTIVITY_VIEW,
    PERMISSIONS.CONTACT_MANAGE,
    PERMISSIONS.CONTACT_VIEW,
    PERMISSIONS.NOTIFICATION_MANAGE,
    PERMISSIONS.NOTIFICATION_VIEW,
    PERMISSIONS.WORKSPACE_MANAGE,
    PERMISSIONS.WORKSPACE_MEMBER_MANAGE,
    PERMISSIONS.WORKSPACE_RESOURCE_MANAGE,
    PERMISSIONS.WORKSPACE_VIEW,
    PERMISSIONS.DATASET_DELETE,
    PERMISSIONS.DATASET_UPLOAD,
    PERMISSIONS.DATASET_VALIDATE,
    PERMISSIONS.DATASET_VIEW,
    PERMISSIONS.CODE_SCAN,
    PERMISSIONS.CODE_VIEW,
    PERMISSIONS.FEDERATION_VIEW,
    PERMISSIONS.GIT_MANAGE,
    PERMISSIONS.GIT_VIEW,
    PERMISSIONS.MONITORING_MANAGE,
    PERMISSIONS.METRICS_VIEW,
    PERMISSIONS.DASHBOARD_MANAGE,
    PERMISSIONS.DASHBOARD_VIEW,
    PERMISSIONS.PLAN_VIEW,
    PERMISSIONS.REPLAY_MANAGE,
    PERMISSIONS.TELEMETRY_VIEW,
  ],
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
    PERMISSIONS.TRAINING_VIEW_LOGS,
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
    PERMISSIONS.NODE_VIEW,
    PERMISSIONS.WORKER_MANAGE,
    PERMISSIONS.WORKER_SPAWN,
    PERMISSIONS.WORKER_VIEW,
    PERMISSIONS.INFERENCE_EXECUTE,
    PERMISSIONS.ACTIVITY_CREATE,
    PERMISSIONS.ACTIVITY_VIEW,
    PERMISSIONS.CONTACT_MANAGE,
    PERMISSIONS.CONTACT_VIEW,
    PERMISSIONS.NOTIFICATION_MANAGE,
    PERMISSIONS.NOTIFICATION_VIEW,
    PERMISSIONS.WORKSPACE_MANAGE,
    PERMISSIONS.WORKSPACE_MEMBER_MANAGE,
    PERMISSIONS.WORKSPACE_RESOURCE_MANAGE,
    PERMISSIONS.WORKSPACE_VIEW,
    PERMISSIONS.DATASET_DELETE,
    PERMISSIONS.DATASET_UPLOAD,
    PERMISSIONS.DATASET_VALIDATE,
    PERMISSIONS.DATASET_VIEW,
    PERMISSIONS.CODE_SCAN,
    PERMISSIONS.CODE_VIEW,
    PERMISSIONS.FEDERATION_VIEW,
    PERMISSIONS.GIT_MANAGE,
    PERMISSIONS.GIT_VIEW,
    PERMISSIONS.MONITORING_MANAGE,
    PERMISSIONS.METRICS_VIEW,
    PERMISSIONS.DASHBOARD_MANAGE,
    PERMISSIONS.DASHBOARD_VIEW,
    PERMISSIONS.PLAN_VIEW,
    PERMISSIONS.REPLAY_MANAGE,
    PERMISSIONS.TELEMETRY_VIEW,
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
    PERMISSIONS.TRAINING_VIEW_LOGS,
    PERMISSIONS.POLICY_VIEW,
    PERMISSIONS.PROMOTION_VIEW,
    PERMISSIONS.INFERENCE_EXECUTE,
    PERMISSIONS.WORKER_MANAGE,
    PERMISSIONS.WORKER_SPAWN,
    PERMISSIONS.WORKER_VIEW,
    PERMISSIONS.NODE_VIEW,
    PERMISSIONS.ACTIVITY_CREATE,
    PERMISSIONS.ACTIVITY_VIEW,
    PERMISSIONS.NOTIFICATION_VIEW,
    PERMISSIONS.WORKSPACE_VIEW,
    PERMISSIONS.DATASET_UPLOAD,
    PERMISSIONS.DATASET_VALIDATE,
    PERMISSIONS.DATASET_VIEW,
    PERMISSIONS.CODE_SCAN,
    PERMISSIONS.CODE_VIEW,
    PERMISSIONS.GIT_MANAGE,
    PERMISSIONS.GIT_VIEW,
    PERMISSIONS.METRICS_VIEW,
    PERMISSIONS.DASHBOARD_VIEW,
    PERMISSIONS.PLAN_VIEW,
    PERMISSIONS.TELEMETRY_VIEW,
  ],
  sre: [
    // SRE can view system information and debug
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.AUDIT_VIEW,
    PERMISSIONS.COMPLIANCE_VIEW,
    PERMISSIONS.TRAINING_VIEW,
    PERMISSIONS.TRAINING_VIEW_LOGS,
    PERMISSIONS.PROMOTION_VIEW,
    PERMISSIONS.POLICY_VIEW,
    PERMISSIONS.WORKER_MANAGE,
    PERMISSIONS.WORKER_VIEW,
    PERMISSIONS.NODE_MANAGE,
    PERMISSIONS.NODE_VIEW,
    PERMISSIONS.ACTIVITY_VIEW,
    PERMISSIONS.NOTIFICATION_VIEW,
    PERMISSIONS.WORKSPACE_VIEW,
    PERMISSIONS.DATASET_VIEW,
    PERMISSIONS.CODE_VIEW,
    PERMISSIONS.FEDERATION_VIEW,
    PERMISSIONS.GIT_VIEW,
    PERMISSIONS.MONITORING_MANAGE,
    PERMISSIONS.METRICS_VIEW,
    PERMISSIONS.DASHBOARD_VIEW,
    PERMISSIONS.PLAN_VIEW,
    PERMISSIONS.REPLAY_MANAGE,
    PERMISSIONS.TELEMETRY_VIEW,
  ],
  compliance: [
    // Compliance can view audit and compliance information
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.AUDIT_VIEW,
    PERMISSIONS.COMPLIANCE_VIEW,
    PERMISSIONS.POLICY_VIEW,
    PERMISSIONS.PROMOTION_VIEW,
    PERMISSIONS.TRAINING_VIEW,
    PERMISSIONS.ACTIVITY_VIEW,
    PERMISSIONS.NOTIFICATION_VIEW,
    PERMISSIONS.WORKSPACE_VIEW,
    PERMISSIONS.DATASET_VIEW,
    PERMISSIONS.CODE_VIEW,
    PERMISSIONS.FEDERATION_VIEW,
    PERMISSIONS.GIT_VIEW,
    PERMISSIONS.METRICS_VIEW,
    PERMISSIONS.DASHBOARD_VIEW,
    PERMISSIONS.PLAN_VIEW,
    PERMISSIONS.TELEMETRY_VIEW,
  ],
  auditor: [
    // Auditor is read-only for audit information
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.AUDIT_VIEW,
    PERMISSIONS.ACTIVITY_VIEW,
    PERMISSIONS.TELEMETRY_VIEW,
    PERMISSIONS.METRICS_VIEW,
  ],
  viewer: [
    // Viewer is read-only
    PERMISSIONS.ADAPTER_LIST,
    PERMISSIONS.ADAPTER_VIEW,
    PERMISSIONS.ACTIVITY_VIEW,
    PERMISSIONS.NOTIFICATION_VIEW,
    PERMISSIONS.WORKSPACE_VIEW,
    PERMISSIONS.DATASET_VIEW,
    PERMISSIONS.METRICS_VIEW,
    PERMISSIONS.DASHBOARD_VIEW,
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
    'training:view-logs': 'View training logs',
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
    'node:view': 'View nodes',
    'worker:manage': 'Manage workers',
    'worker:spawn': 'Spawn workers',
    'worker:view': 'View workers',
    'inference:execute': 'Execute inference',
    'activity:create': 'Create activity entries',
    'activity:view': 'View activity entries',
    'contact:manage': 'Manage contacts',
    'contact:view': 'View contacts',
    'notification:manage': 'Manage notifications',
    'notification:view': 'View notifications',
    'workspace:manage': 'Manage workspaces',
    'workspace:member-manage': 'Manage workspace members',
    'workspace:resource-manage': 'Manage workspace resources',
    'workspace:view': 'View workspaces',
    'dataset:delete': 'Delete datasets',
    'dataset:upload': 'Upload datasets',
    'dataset:validate': 'Validate datasets',
    'dataset:view': 'View datasets',
    'code:scan': 'Scan code repositories',
    'code:view': 'View code intelligence',
    'federation:view': 'View federation status',
    'git:manage': 'Manage git repositories',
    'git:view': 'View git repositories',
    'monitoring:manage': 'Manage monitoring rules',
    'metrics:view': 'View metrics',
    'dashboard:manage': 'Manage dashboards',
    'dashboard:view': 'View dashboards',
    'plan:view': 'View plans',
    'replay:manage': 'Manage replay sessions',
    'telemetry:view': 'View telemetry',
  };

  return descriptions[permission] || permission;
}

/**
 * Check if user has a role with at least the specified level
 * (e.g., hasRoleLevel(user, 'operator') returns true for admin, operator, and sre)
 *
 * @param user - The user object
 * @param minRole - The minimum role required
 * @returns true if the user's role level is >= minRole level
 */
export function hasRoleLevel(user: User | null | undefined, minRole: UserRole): boolean {
  if (!user) return false;
  const userLevel = ROLE_HIERARCHY[user.role.toLowerCase()] || 0;
  const minLevel = ROLE_HIERARCHY[minRole.toLowerCase()] || 0;
  return userLevel >= minLevel;
}

/**
 * Check if user can perform admin operations
 */
export function canAdmin(user: User | null | undefined): boolean {
  if (!user) return false;
  const role = user.role.toLowerCase();
  return role === 'admin' || role === 'developer';
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
  if (!user) return false;
  const role = user.role.toLowerCase();
  return ['developer', 'admin', 'operator', 'sre', 'compliance'].includes(role);
}

/**
 * Check if user can modify tenants
 */
export function canModifyTenants(user: User | null | undefined): boolean {
  if (!user) return false;
  const role = user.role.toLowerCase();
  return role === 'admin' || role === 'developer';
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
  if (!user) return false;
  const role = user.role.toLowerCase();
  return ['developer', 'admin', 'operator', 'sre', 'compliance'].includes(role);
}

/**
 * Check if user can export telemetry bundles
 */
export function canExportTelemetry(user: User | null | undefined): boolean {
  if (!user) return false;
  const role = user.role.toLowerCase();
  return ['developer', 'admin', 'operator', 'sre', 'compliance'].includes(role);
}

/**
 * Check if user can promote CPIDs
 */
export function canPromote(user: User | null | undefined): boolean {
  if (!user) return false;
  const role = user.role.toLowerCase();
  return role === 'admin' || role === 'developer';
}

/**
 * Get human-readable role name
 */
export function getRoleName(role: UserRole): string {
  const names: Record<string, string> = {
    developer: 'Developer',
    admin: 'Administrator',
    operator: 'Operator',
    sre: 'Site Reliability Engineer',
    compliance: 'Compliance Officer',
    auditor: 'Auditor',
    viewer: 'Viewer',
  };
  return names[role.toLowerCase()] || role;
}

/**
 * Get role description
 */
export function getRoleDescription(role: UserRole): string {
  const descriptions: Record<string, string> = {
    developer: 'Full system access to all features, pages, and UI modes regardless of restrictions',
    admin: 'Full system access including tenant management and CPID promotion',
    operator: 'Can manage adapters, workers, and perform inference operations',
    sre: 'Can view system information, debug, and manage infrastructure',
    compliance: 'Can view audit logs, telemetry bundles, and compliance reports',
    auditor: 'Read-only access to audit information',
    viewer: 'Read-only access to system status and metrics',
  };
  return descriptions[role.toLowerCase()] || '';
}
