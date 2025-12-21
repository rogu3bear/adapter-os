import { describe, it, expect } from 'vitest';
import {
  ROLES,
  PERMISSIONS,
  hasPermission,
  hasAnyPermission,
  hasAllPermissions,
  hasRole,
  getUserPermissions,
  roleCanPerform,
  getPermissionDescription,
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
  type User,
  type UserRole,
} from '@/utils/rbac';

describe('rbac', () => {
  describe('hasPermission', () => {
    it('should return true when user has the permission', () => {
      expect(hasPermission('admin', PERMISSIONS.ADAPTER_DELETE)).toBe(true);
      expect(hasPermission('operator', PERMISSIONS.ADAPTER_REGISTER)).toBe(true);
      expect(hasPermission('viewer', PERMISSIONS.ADAPTER_VIEW)).toBe(true);
    });

    it('should return false when user does not have the permission', () => {
      expect(hasPermission('viewer', PERMISSIONS.ADAPTER_DELETE)).toBe(false);
      expect(hasPermission('auditor', PERMISSIONS.TRAINING_START)).toBe(false);
      expect(hasPermission('operator', PERMISSIONS.TENANT_MANAGE)).toBe(false);
    });

    it('should return false when userRole is undefined', () => {
      expect(hasPermission(undefined, PERMISSIONS.ADAPTER_VIEW)).toBe(false);
    });

    it('should handle case-insensitive role names', () => {
      expect(hasPermission('ADMIN' as UserRole, PERMISSIONS.ADAPTER_DELETE)).toBe(true);
      expect(hasPermission('Admin' as UserRole, PERMISSIONS.ADAPTER_DELETE)).toBe(true);
    });
  });

  describe('hasAnyPermission', () => {
    it('should return true when user has at least one permission', () => {
      expect(
        hasAnyPermission('operator', [
          PERMISSIONS.ADAPTER_DELETE,
          PERMISSIONS.ADAPTER_REGISTER,
        ])
      ).toBe(true);
    });

    it('should return false when user has none of the permissions', () => {
      expect(
        hasAnyPermission('viewer', [
          PERMISSIONS.ADAPTER_DELETE,
          PERMISSIONS.TRAINING_START,
        ])
      ).toBe(false);
    });

    it('should return true when user has all permissions', () => {
      expect(
        hasAnyPermission('admin', [
          PERMISSIONS.ADAPTER_DELETE,
          PERMISSIONS.TRAINING_START,
        ])
      ).toBe(true);
    });
  });

  describe('hasAllPermissions', () => {
    it('should return true when user has all permissions', () => {
      expect(
        hasAllPermissions('admin', [
          PERMISSIONS.ADAPTER_DELETE,
          PERMISSIONS.TRAINING_START,
        ])
      ).toBe(true);
    });

    it('should return false when user has only some permissions', () => {
      expect(
        hasAllPermissions('operator', [
          PERMISSIONS.ADAPTER_REGISTER,
          PERMISSIONS.TENANT_MANAGE,
        ])
      ).toBe(false);
    });

    it('should return false when user has none of the permissions', () => {
      expect(
        hasAllPermissions('viewer', [
          PERMISSIONS.ADAPTER_DELETE,
          PERMISSIONS.TRAINING_START,
        ])
      ).toBe(false);
    });
  });

  describe('hasRole', () => {
    it('should return true when user has one of the required roles', () => {
      expect(hasRole('admin', ['admin', 'developer'])).toBe(true);
      expect(hasRole('operator', ['operator', 'admin'])).toBe(true);
    });

    it('should return false when user does not have any of the required roles', () => {
      expect(hasRole('viewer', ['admin', 'operator'])).toBe(false);
      expect(hasRole('auditor', ['admin', 'developer'])).toBe(false);
    });

    it('should handle case-insensitive role comparison', () => {
      expect(hasRole('ADMIN' as UserRole, ['admin'])).toBe(true);
      expect(hasRole('admin', ['ADMIN' as UserRole])).toBe(true);
    });

    it('should return true when requiredRoles is empty', () => {
      expect(hasRole('admin', [])).toBe(true);
    });

    it('should return false when userRole is undefined', () => {
      expect(hasRole(undefined, ['admin'])).toBe(false);
    });
  });

  describe('getUserPermissions', () => {
    it('should return all permissions for a role', () => {
      const adminPerms = getUserPermissions('admin');
      expect(adminPerms).toContain(PERMISSIONS.ADAPTER_DELETE);
      expect(adminPerms).toContain(PERMISSIONS.TRAINING_START);
      expect(adminPerms.length).toBeGreaterThan(20);
    });

    it('should return limited permissions for viewer role', () => {
      const viewerPerms = getUserPermissions('viewer');
      expect(viewerPerms).toContain(PERMISSIONS.ADAPTER_VIEW);
      expect(viewerPerms).not.toContain(PERMISSIONS.ADAPTER_DELETE);
    });

    it('should return empty array for undefined role', () => {
      expect(getUserPermissions(undefined)).toEqual([]);
    });
  });

  describe('roleCanPerform', () => {
    it('should be an alias for hasPermission', () => {
      expect(roleCanPerform('admin', PERMISSIONS.ADAPTER_DELETE)).toBe(
        hasPermission('admin', PERMISSIONS.ADAPTER_DELETE)
      );
      expect(roleCanPerform('viewer', PERMISSIONS.ADAPTER_DELETE)).toBe(
        hasPermission('viewer', PERMISSIONS.ADAPTER_DELETE)
      );
    });
  });

  describe('getPermissionDescription', () => {
    it('should return description for known permissions', () => {
      expect(getPermissionDescription(PERMISSIONS.ADAPTER_DELETE)).toBe('Delete adapters');
      expect(getPermissionDescription(PERMISSIONS.TRAINING_START)).toBe('Start training jobs');
      expect(getPermissionDescription(PERMISSIONS.TENANT_MANAGE)).toBe('Manage tenants');
    });

    it('should return the permission key for unknown permissions', () => {
      expect(getPermissionDescription('unknown:permission')).toBe('unknown:permission');
    });
  });

  describe('hasRoleLevel', () => {
    const developerUser: User = {
      id: '1',
      email: 'dev@test.com',
      role: 'developer',
    };

    const adminUser: User = {
      id: '2',
      email: 'admin@test.com',
      role: 'admin',
    };

    const operatorUser: User = {
      id: '3',
      email: 'operator@test.com',
      role: 'operator',
    };

    const viewerUser: User = {
      id: '4',
      email: 'viewer@test.com',
      role: 'viewer',
    };

    it('should return true when user role level is equal to min role', () => {
      expect(hasRoleLevel(operatorUser, 'operator')).toBe(true);
    });

    it('should return true when user role level is higher than min role', () => {
      expect(hasRoleLevel(adminUser, 'operator')).toBe(true);
      expect(hasRoleLevel(developerUser, 'admin')).toBe(true);
    });

    it('should return false when user role level is lower than min role', () => {
      expect(hasRoleLevel(viewerUser, 'operator')).toBe(false);
      expect(hasRoleLevel(operatorUser, 'admin')).toBe(false);
    });

    it('should return false when user is null or undefined', () => {
      expect(hasRoleLevel(null, 'operator')).toBe(false);
      expect(hasRoleLevel(undefined, 'operator')).toBe(false);
    });

    it('should handle case-insensitive roles', () => {
      const upperCaseUser: User = {
        id: '5',
        email: 'upper@test.com',
        role: 'ADMIN' as UserRole,
      };
      expect(hasRoleLevel(upperCaseUser, 'operator')).toBe(true);
    });
  });

  describe('canAdmin', () => {
    it('should return true for admin role', () => {
      const user: User = { id: '1', email: 'admin@test.com', role: 'admin' };
      expect(canAdmin(user)).toBe(true);
    });

    it('should return true for developer role', () => {
      const user: User = { id: '1', email: 'dev@test.com', role: 'developer' };
      expect(canAdmin(user)).toBe(true);
    });

    it('should return false for other roles', () => {
      const operator: User = { id: '1', email: 'op@test.com', role: 'operator' };
      const viewer: User = { id: '2', email: 'view@test.com', role: 'viewer' };
      expect(canAdmin(operator)).toBe(false);
      expect(canAdmin(viewer)).toBe(false);
    });

    it('should return false for null or undefined user', () => {
      expect(canAdmin(null)).toBe(false);
      expect(canAdmin(undefined)).toBe(false);
    });
  });

  describe('canOperate', () => {
    it('should return true for operator and higher roles', () => {
      const operator: User = { id: '1', email: 'op@test.com', role: 'operator' };
      const admin: User = { id: '2', email: 'admin@test.com', role: 'admin' };
      expect(canOperate(operator)).toBe(true);
      expect(canOperate(admin)).toBe(true);
    });

    it('should return false for lower roles', () => {
      const viewer: User = { id: '1', email: 'view@test.com', role: 'viewer' };
      expect(canOperate(viewer)).toBe(false);
    });
  });

  describe('canViewCompliance', () => {
    it('should return true for compliance-capable roles', () => {
      const roles: UserRole[] = ['developer', 'admin', 'operator', 'sre', 'compliance'];
      roles.forEach(role => {
        const user: User = { id: '1', email: 'test@test.com', role };
        expect(canViewCompliance(user)).toBe(true);
      });
    });

    it('should return false for viewer and auditor roles', () => {
      const viewer: User = { id: '1', email: 'view@test.com', role: 'viewer' };
      const auditor: User = { id: '2', email: 'audit@test.com', role: 'auditor' };
      expect(canViewCompliance(viewer)).toBe(false);
      expect(canViewCompliance(auditor)).toBe(false);
    });
  });

  describe('canModifyTenants', () => {
    it('should return true for admin and developer', () => {
      const admin: User = { id: '1', email: 'admin@test.com', role: 'admin' };
      const developer: User = { id: '2', email: 'dev@test.com', role: 'developer' };
      expect(canModifyTenants(admin)).toBe(true);
      expect(canModifyTenants(developer)).toBe(true);
    });

    it('should return false for other roles', () => {
      const operator: User = { id: '1', email: 'op@test.com', role: 'operator' };
      expect(canModifyTenants(operator)).toBe(false);
    });
  });

  describe('canManageAdapters', () => {
    it('should return true for operator and higher roles', () => {
      const operator: User = { id: '1', email: 'op@test.com', role: 'operator' };
      const admin: User = { id: '2', email: 'admin@test.com', role: 'admin' };
      expect(canManageAdapters(operator)).toBe(true);
      expect(canManageAdapters(admin)).toBe(true);
    });

    it('should return false for lower roles', () => {
      const viewer: User = { id: '1', email: 'view@test.com', role: 'viewer' };
      expect(canManageAdapters(viewer)).toBe(false);
    });
  });

  describe('canViewAudits', () => {
    it('should return true for audit-capable roles', () => {
      const roles: UserRole[] = ['developer', 'admin', 'operator', 'sre', 'compliance'];
      roles.forEach(role => {
        const user: User = { id: '1', email: 'test@test.com', role };
        expect(canViewAudits(user)).toBe(true);
      });
    });

    it('should return false for viewer and auditor roles', () => {
      const viewer: User = { id: '1', email: 'view@test.com', role: 'viewer' };
      const auditor: User = { id: '2', email: 'audit@test.com', role: 'auditor' };
      expect(canViewAudits(viewer)).toBe(false);
      expect(canViewAudits(auditor)).toBe(false);
    });
  });

  describe('canExportTelemetry', () => {
    it('should return true for telemetry-capable roles', () => {
      const roles: UserRole[] = ['developer', 'admin', 'operator', 'sre', 'compliance'];
      roles.forEach(role => {
        const user: User = { id: '1', email: 'test@test.com', role };
        expect(canExportTelemetry(user)).toBe(true);
      });
    });

    it('should return false for viewer and auditor roles', () => {
      const viewer: User = { id: '1', email: 'view@test.com', role: 'viewer' };
      const auditor: User = { id: '2', email: 'audit@test.com', role: 'auditor' };
      expect(canExportTelemetry(viewer)).toBe(false);
      expect(canExportTelemetry(auditor)).toBe(false);
    });
  });

  describe('canPromote', () => {
    it('should return true for admin and developer', () => {
      const admin: User = { id: '1', email: 'admin@test.com', role: 'admin' };
      const developer: User = { id: '2', email: 'dev@test.com', role: 'developer' };
      expect(canPromote(admin)).toBe(true);
      expect(canPromote(developer)).toBe(true);
    });

    it('should return false for other roles', () => {
      const operator: User = { id: '1', email: 'op@test.com', role: 'operator' };
      expect(canPromote(operator)).toBe(false);
    });
  });

  describe('getRoleName', () => {
    it('should return human-readable role names', () => {
      expect(getRoleName('developer')).toBe('Developer');
      expect(getRoleName('admin')).toBe('Administrator');
      expect(getRoleName('operator')).toBe('Operator');
      expect(getRoleName('sre')).toBe('Site Reliability Engineer');
      expect(getRoleName('compliance')).toBe('Compliance Officer');
      expect(getRoleName('auditor')).toBe('Auditor');
      expect(getRoleName('viewer')).toBe('Viewer');
    });

    it('should return the role itself for unknown roles', () => {
      expect(getRoleName('unknown' as UserRole)).toBe('unknown');
    });

    it('should handle case-insensitive role names', () => {
      expect(getRoleName('ADMIN' as UserRole)).toBe('Administrator');
    });
  });

  describe('getRoleDescription', () => {
    it('should return descriptions for all roles', () => {
      expect(getRoleDescription('developer')).toContain('Full system access');
      expect(getRoleDescription('admin')).toContain('Full system access');
      expect(getRoleDescription('operator')).toContain('manage adapters');
      expect(getRoleDescription('sre')).toContain('debug');
      expect(getRoleDescription('compliance')).toContain('audit logs');
      expect(getRoleDescription('auditor')).toContain('Read-only');
      expect(getRoleDescription('viewer')).toContain('Read-only');
    });

    it('should return empty string for unknown roles', () => {
      expect(getRoleDescription('unknown' as UserRole)).toBe('');
    });
  });

  describe('ROLES constants', () => {
    it('should have all expected role constants', () => {
      expect(ROLES.DEVELOPER).toBe('developer');
      expect(ROLES.ADMIN).toBe('admin');
      expect(ROLES.OPERATOR).toBe('operator');
      expect(ROLES.SRE).toBe('sre');
      expect(ROLES.COMPLIANCE).toBe('compliance');
      expect(ROLES.AUDITOR).toBe('auditor');
      expect(ROLES.VIEWER).toBe('viewer');
    });
  });

  describe('Role hierarchy', () => {
    it('should maintain correct permission inheritance', () => {
      // Developer should have all permissions that admin has
      const devPerms = getUserPermissions('developer');
      const adminPerms = getUserPermissions('admin');
      adminPerms.forEach(perm => {
        expect(devPerms).toContain(perm);
      });

      // Admin should have all permissions that operator has
      const operatorPerms = getUserPermissions('operator');
      operatorPerms.forEach(perm => {
        expect(adminPerms).toContain(perm);
      });
    });

    it('should ensure viewer has minimal permissions', () => {
      const viewerPerms = getUserPermissions('viewer');
      expect(viewerPerms).not.toContain(PERMISSIONS.ADAPTER_DELETE);
      expect(viewerPerms).not.toContain(PERMISSIONS.TRAINING_START);
      expect(viewerPerms).not.toContain(PERMISSIONS.TENANT_MANAGE);
    });
  });
});
