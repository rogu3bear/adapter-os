import { describe, it, expect } from 'vitest';
import {
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
  ROLES,
  PERMISSIONS,
  ROLE_PERMISSIONS,
  type User,
} from '../utils/rbac';

describe('hasPermission', () => {
  it('returns true for valid role/permission', () => {
    expect(hasPermission('admin', PERMISSIONS.ADAPTER_DELETE)).toBe(true);
    expect(hasPermission('operator', PERMISSIONS.TRAINING_START)).toBe(true);
    expect(hasPermission('viewer', PERMISSIONS.ADAPTER_LIST)).toBe(true);
  });

  it('returns false for invalid permission', () => {
    expect(hasPermission('viewer', PERMISSIONS.ADAPTER_DELETE)).toBe(false);
    expect(hasPermission('auditor', PERMISSIONS.TRAINING_START)).toBe(false);
    expect(hasPermission('compliance', PERMISSIONS.TENANT_MANAGE)).toBe(false);
  });

  it('returns false for undefined role', () => {
    expect(hasPermission(undefined, PERMISSIONS.ADAPTER_LIST)).toBe(false);
  });

  it('handles case-insensitive roles', () => {
    expect(hasPermission('ADMIN' as any, PERMISSIONS.ADAPTER_DELETE)).toBe(true);
    expect(hasPermission('Admin' as any, PERMISSIONS.TENANT_MANAGE)).toBe(true);
  });
});

describe('hasAnyPermission', () => {
  it('returns true when user has at least one permission (OR logic)', () => {
    expect(hasAnyPermission('viewer', [
      PERMISSIONS.ADAPTER_DELETE,
      PERMISSIONS.ADAPTER_LIST,
    ])).toBe(true);
  });

  it('returns false when user has none of the permissions', () => {
    expect(hasAnyPermission('viewer', [
      PERMISSIONS.ADAPTER_DELETE,
      PERMISSIONS.TENANT_MANAGE,
    ])).toBe(false);
  });

  it('returns true when user has all permissions', () => {
    expect(hasAnyPermission('admin', [
      PERMISSIONS.ADAPTER_DELETE,
      PERMISSIONS.TENANT_MANAGE,
    ])).toBe(true);
  });

  it('returns false for empty permissions array', () => {
    expect(hasAnyPermission('admin', [])).toBe(false);
  });
});

describe('hasAllPermissions', () => {
  it('returns true when user has all permissions (AND logic)', () => {
    expect(hasAllPermissions('admin', [
      PERMISSIONS.ADAPTER_DELETE,
      PERMISSIONS.TENANT_MANAGE,
      PERMISSIONS.POLICY_SIGN,
    ])).toBe(true);
  });

  it('returns false when user is missing at least one permission', () => {
    expect(hasAllPermissions('operator', [
      PERMISSIONS.TRAINING_START,
      PERMISSIONS.TENANT_MANAGE, // operator doesn't have this
    ])).toBe(false);
  });

  it('returns true for empty permissions array', () => {
    expect(hasAllPermissions('viewer', [])).toBe(true);
  });
});

describe('getUserPermissions', () => {
  it('returns all permissions for admin role', () => {
    const permissions = getUserPermissions('admin');
    expect(permissions).toContain(PERMISSIONS.ADAPTER_DELETE);
    expect(permissions).toContain(PERMISSIONS.TENANT_MANAGE);
    expect(permissions).toContain(PERMISSIONS.POLICY_SIGN);
    expect(permissions.length).toBe(ROLE_PERMISSIONS.admin.length);
  });

  it('returns limited permissions for viewer role', () => {
    const permissions = getUserPermissions('viewer');
    expect(permissions).toContain(PERMISSIONS.ADAPTER_LIST);
    expect(permissions).toContain(PERMISSIONS.ADAPTER_VIEW);
    expect(permissions.length).toBeGreaterThanOrEqual(2);
  });

  it('returns empty array for undefined role', () => {
    expect(getUserPermissions(undefined)).toEqual([]);
  });
});

describe('hasRole', () => {
  it('returns true when user has one of the required roles', () => {
    expect(hasRole('admin', ['admin', 'operator'])).toBe(true);
    expect(hasRole('operator', ['admin', 'operator'])).toBe(true);
  });

  it('returns false when user does not have required role', () => {
    expect(hasRole('viewer', ['admin', 'operator'])).toBe(false);
  });

  it('handles case-insensitive comparison', () => {
    expect(hasRole('ADMIN' as any, ['admin'])).toBe(true);
  });

  it('returns true for empty required roles', () => {
    expect(hasRole('viewer', [])).toBe(true);
  });

  it('returns true for undefined role with empty required roles', () => {
    expect(hasRole(undefined, [])).toBe(true);
  });
});

describe('Role hierarchy', () => {
  const createUser = (role: string): User => ({
    id: '1',
    email: 'test@example.com',
    role: role as any,
  });

  describe('hasRoleLevel', () => {
    it('admin has highest level (6)', () => {
      const admin = createUser('admin');
      expect(hasRoleLevel(admin, 'admin')).toBe(true);
      expect(hasRoleLevel(admin, 'operator')).toBe(true);
      expect(hasRoleLevel(admin, 'sre')).toBe(true);
      expect(hasRoleLevel(admin, 'compliance')).toBe(true);
      expect(hasRoleLevel(admin, 'auditor')).toBe(true);
      expect(hasRoleLevel(admin, 'viewer')).toBe(true);
    });

    it('operator has level 5', () => {
      const operator = createUser('operator');
      expect(hasRoleLevel(operator, 'admin')).toBe(false);
      expect(hasRoleLevel(operator, 'operator')).toBe(true);
      expect(hasRoleLevel(operator, 'sre')).toBe(true);
      expect(hasRoleLevel(operator, 'viewer')).toBe(true);
    });

    it('sre has level 4', () => {
      const sre = createUser('sre');
      expect(hasRoleLevel(sre, 'admin')).toBe(false);
      expect(hasRoleLevel(sre, 'operator')).toBe(false);
      expect(hasRoleLevel(sre, 'sre')).toBe(true);
      expect(hasRoleLevel(sre, 'compliance')).toBe(true);
      expect(hasRoleLevel(sre, 'viewer')).toBe(true);
    });

    it('compliance has level 3', () => {
      const compliance = createUser('compliance');
      expect(hasRoleLevel(compliance, 'sre')).toBe(false);
      expect(hasRoleLevel(compliance, 'compliance')).toBe(true);
      expect(hasRoleLevel(compliance, 'auditor')).toBe(true);
      expect(hasRoleLevel(compliance, 'viewer')).toBe(true);
    });

    it('auditor has level 2', () => {
      const auditor = createUser('auditor');
      expect(hasRoleLevel(auditor, 'compliance')).toBe(false);
      expect(hasRoleLevel(auditor, 'auditor')).toBe(true);
      expect(hasRoleLevel(auditor, 'viewer')).toBe(true);
    });

    it('viewer has lowest level (1)', () => {
      const viewer = createUser('viewer');
      expect(hasRoleLevel(viewer, 'auditor')).toBe(false);
      expect(hasRoleLevel(viewer, 'viewer')).toBe(true);
    });

    it('returns false for null/undefined user', () => {
      expect(hasRoleLevel(null, 'viewer')).toBe(false);
      expect(hasRoleLevel(undefined, 'viewer')).toBe(false);
    });
  });
});

describe('Permission descriptions', () => {
  it('returns description for known permissions', () => {
    expect(getPermissionDescription(PERMISSIONS.ADAPTER_LIST)).toBe('View adapters');
    expect(getPermissionDescription(PERMISSIONS.ADAPTER_DELETE)).toBe('Delete adapters');
    expect(getPermissionDescription(PERMISSIONS.TENANT_MANAGE)).toBe('Manage tenants');
    expect(getPermissionDescription(PERMISSIONS.INFERENCE_EXECUTE)).toBe('Execute inference');
  });

  it('returns permission key for unknown permissions', () => {
    expect(getPermissionDescription('unknown:permission')).toBe('unknown:permission');
  });
});

describe('Convenience functions', () => {
  const createUser = (role: string): User => ({
    id: '1',
    email: 'test@example.com',
    role: role as any,
  });

  describe('canAdmin', () => {
    it('returns true only for admin', () => {
      expect(canAdmin(createUser('admin'))).toBe(true);
      expect(canAdmin(createUser('operator'))).toBe(false);
      expect(canAdmin(createUser('viewer'))).toBe(false);
      expect(canAdmin(null)).toBe(false);
    });
  });

  describe('canOperate', () => {
    it('returns true for admin and operator', () => {
      expect(canOperate(createUser('admin'))).toBe(true);
      expect(canOperate(createUser('operator'))).toBe(true);
      expect(canOperate(createUser('sre'))).toBe(false);
      expect(canOperate(createUser('viewer'))).toBe(false);
      expect(canOperate(null)).toBe(false);
    });
  });

  describe('canViewCompliance', () => {
    it('returns true for admin, operator, sre, compliance', () => {
      expect(canViewCompliance(createUser('admin'))).toBe(true);
      expect(canViewCompliance(createUser('operator'))).toBe(true);
      expect(canViewCompliance(createUser('sre'))).toBe(true);
      expect(canViewCompliance(createUser('compliance'))).toBe(true);
      expect(canViewCompliance(createUser('auditor'))).toBe(false);
      expect(canViewCompliance(createUser('viewer'))).toBe(false);
      expect(canViewCompliance(null)).toBe(false);
    });
  });

  describe('canModifyTenants', () => {
    it('returns true only for admin', () => {
      expect(canModifyTenants(createUser('admin'))).toBe(true);
      expect(canModifyTenants(createUser('operator'))).toBe(false);
      expect(canModifyTenants(null)).toBe(false);
    });
  });

  describe('canManageAdapters', () => {
    it('returns true for admin and operator', () => {
      expect(canManageAdapters(createUser('admin'))).toBe(true);
      expect(canManageAdapters(createUser('operator'))).toBe(true);
      expect(canManageAdapters(createUser('sre'))).toBe(false);
      expect(canManageAdapters(null)).toBe(false);
    });
  });

  describe('canViewAudits', () => {
    it('returns true for admin, operator, sre, compliance', () => {
      expect(canViewAudits(createUser('admin'))).toBe(true);
      expect(canViewAudits(createUser('operator'))).toBe(true);
      expect(canViewAudits(createUser('sre'))).toBe(true);
      expect(canViewAudits(createUser('compliance'))).toBe(true);
      expect(canViewAudits(createUser('auditor'))).toBe(false);
      expect(canViewAudits(null)).toBe(false);
    });
  });

  describe('canExportTelemetry', () => {
    it('returns true for admin, operator, sre, compliance', () => {
      expect(canExportTelemetry(createUser('admin'))).toBe(true);
      expect(canExportTelemetry(createUser('operator'))).toBe(true);
      expect(canExportTelemetry(createUser('sre'))).toBe(true);
      expect(canExportTelemetry(createUser('compliance'))).toBe(true);
      expect(canExportTelemetry(createUser('auditor'))).toBe(false);
      expect(canExportTelemetry(null)).toBe(false);
    });
  });

  describe('canPromote', () => {
    it('returns true only for admin', () => {
      expect(canPromote(createUser('admin'))).toBe(true);
      expect(canPromote(createUser('operator'))).toBe(false);
      expect(canPromote(null)).toBe(false);
    });
  });
});

describe('roleCanPerform', () => {
  it('is an alias for hasPermission', () => {
    expect(roleCanPerform('admin', PERMISSIONS.TENANT_MANAGE)).toBe(true);
    expect(roleCanPerform('viewer', PERMISSIONS.TENANT_MANAGE)).toBe(false);
  });
});

describe('getRoleName', () => {
  it('returns human-readable role names', () => {
    expect(getRoleName('admin')).toBe('Administrator');
    expect(getRoleName('operator')).toBe('Operator');
    expect(getRoleName('sre')).toBe('Site Reliability Engineer');
    expect(getRoleName('compliance')).toBe('Compliance Officer');
    expect(getRoleName('auditor')).toBe('Auditor');
    expect(getRoleName('viewer')).toBe('Viewer');
  });

  it('returns original role for unknown roles', () => {
    expect(getRoleName('unknown' as any)).toBe('unknown');
  });
});

describe('getRoleDescription', () => {
  it('returns role descriptions', () => {
    expect(getRoleDescription('admin')).toContain('Full system access');
    expect(getRoleDescription('operator')).toContain('manage adapters');
    expect(getRoleDescription('sre')).toContain('debug');
    expect(getRoleDescription('compliance')).toContain('audit logs');
    expect(getRoleDescription('auditor')).toContain('Read-only');
    expect(getRoleDescription('viewer')).toContain('Read-only');
  });

  it('returns empty string for unknown roles', () => {
    expect(getRoleDescription('unknown' as any)).toBe('');
  });
});

describe('ROLES constant', () => {
  it('contains all role constants', () => {
    expect(ROLES.ADMIN).toBe('admin');
    expect(ROLES.OPERATOR).toBe('operator');
    expect(ROLES.SRE).toBe('sre');
    expect(ROLES.COMPLIANCE).toBe('compliance');
    expect(ROLES.AUDITOR).toBe('auditor');
    expect(ROLES.VIEWER).toBe('viewer');
  });
});

describe('PERMISSIONS constant', () => {
  it('contains all permission constants', () => {
    expect(PERMISSIONS.ADAPTER_LIST).toBe('adapter:list');
    expect(PERMISSIONS.TENANT_MANAGE).toBe('tenant:manage');
    expect(PERMISSIONS.INFERENCE_EXECUTE).toBe('inference:execute');
  });
});

describe('Role permission assignments', () => {
  it('admin has all permissions', () => {
    const adminPerms = ROLE_PERMISSIONS.admin;
    expect(adminPerms).toContain(PERMISSIONS.ADAPTER_DELETE);
    expect(adminPerms).toContain(PERMISSIONS.TENANT_MANAGE);
    expect(adminPerms).toContain(PERMISSIONS.POLICY_SIGN);
    expect(adminPerms).toContain(PERMISSIONS.NODE_MANAGE);
  });

  it('operator has operational permissions but not admin-only', () => {
    const operatorPerms = ROLE_PERMISSIONS.operator;
    expect(operatorPerms).toContain(PERMISSIONS.ADAPTER_REGISTER);
    expect(operatorPerms).toContain(PERMISSIONS.TRAINING_START);
    expect(operatorPerms).toContain(PERMISSIONS.INFERENCE_EXECUTE);
    expect(operatorPerms).not.toContain(PERMISSIONS.ADAPTER_DELETE);
    expect(operatorPerms).not.toContain(PERMISSIONS.TENANT_MANAGE);
  });

  it('sre has infrastructure and audit permissions', () => {
    const srePerms = ROLE_PERMISSIONS.sre;
    expect(srePerms).toContain(PERMISSIONS.AUDIT_VIEW);
    expect(srePerms).toContain(PERMISSIONS.NODE_MANAGE);
    expect(srePerms).toContain(PERMISSIONS.WORKER_MANAGE);
    expect(srePerms).not.toContain(PERMISSIONS.TRAINING_START);
  });

  it('compliance has audit and compliance view permissions', () => {
    const compliancePerms = ROLE_PERMISSIONS.compliance;
    expect(compliancePerms).toContain(PERMISSIONS.AUDIT_VIEW);
    expect(compliancePerms).toContain(PERMISSIONS.COMPLIANCE_VIEW);
    expect(compliancePerms).toContain(PERMISSIONS.POLICY_VIEW);
    expect(compliancePerms).not.toContain(PERMISSIONS.TRAINING_START);
  });

  it('auditor has minimal read permissions', () => {
    const auditorPerms = ROLE_PERMISSIONS.auditor;
    expect(auditorPerms).toContain(PERMISSIONS.ADAPTER_LIST);
    expect(auditorPerms).toContain(PERMISSIONS.AUDIT_VIEW);
    expect(auditorPerms.length).toBeGreaterThanOrEqual(3);
  });

  it('viewer has only list and view permissions', () => {
    const viewerPerms = ROLE_PERMISSIONS.viewer;
    expect(viewerPerms).toContain(PERMISSIONS.ADAPTER_LIST);
    expect(viewerPerms).toContain(PERMISSIONS.ADAPTER_VIEW);
    expect(viewerPerms.length).toBeGreaterThanOrEqual(2);
  });
});
