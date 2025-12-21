/**
 * @deprecated This module is deprecated. Import from '@/utils/rbac' instead.
 *
 * The RBAC system has been consolidated into a single source of truth at:
 * ui/src/utils/rbac.ts
 *
 * This file re-exports from utils/rbac.ts for backward compatibility.
 * Please update imports to use '@/utils/rbac' directly.
 */

export {
  // Types
  type User,
  type UserRole,

  // Constants
  ROLES,
  PERMISSIONS,
  ROLE_PERMISSIONS,

  // Permission-based functions
  hasPermission,
  hasAnyPermission,
  hasAllPermissions,
  hasRole,
  getUserPermissions,
  roleCanPerform,
  getPermissionDescription,

  // Role-level functions (legacy compatibility)
  hasRoleLevel,
  canAdmin,
  canOperate,
  canViewCompliance,
  canModifyTenants,
  canManageAdapters,
  canViewAudits,
  canExportTelemetry,
  canPromote,
  getRoleName,
  getRoleDescription,
} from '@/utils/rbac';
