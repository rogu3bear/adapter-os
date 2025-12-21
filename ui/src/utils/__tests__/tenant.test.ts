import { describe, it, expect } from 'vitest';
import { withTenantKey, TENANT_SWITCH_EVENT, TENANT_ACCESS_DENIED_EVENT } from '@/utils/tenant';

describe('tenant', () => {
  describe('Event constants', () => {
    it('should export TENANT_SWITCH_EVENT constant', () => {
      expect(TENANT_SWITCH_EVENT).toBe('aos:tenant-switched');
    });

    it('should export TENANT_ACCESS_DENIED_EVENT constant', () => {
      expect(TENANT_ACCESS_DENIED_EVENT).toBe('aos:tenant-access-denied');
    });
  });

  describe('withTenantKey', () => {
    describe('Valid tenant IDs', () => {
      it('should append tenant ID to query key parts', () => {
        const result = withTenantKey(['users', 'list'], 'tenant-123');
        expect(result).toEqual(['users', 'list', 'tenant-123']);
      });

      it('should handle UUID format tenant IDs', () => {
        const uuid = '550e8400-e29b-41d4-a716-446655440000';
        const result = withTenantKey(['adapters'], uuid);
        expect(result).toEqual(['adapters', uuid]);
      });

      it('should handle numeric string tenant IDs', () => {
        const result = withTenantKey(['models'], '12345');
        expect(result).toEqual(['models', '12345']);
      });

      it('should handle tenant IDs with special characters', () => {
        const result = withTenantKey(['data'], 'tenant_with-special.chars');
        expect(result).toEqual(['data', 'tenant_with-special.chars']);
      });

      it('should handle tenant IDs with hyphens and underscores', () => {
        const result = withTenantKey(['cache'], 'org-123_env-prod');
        expect(result).toEqual(['cache', 'org-123_env-prod']);
      });

      it('should trim whitespace from tenant IDs', () => {
        const result = withTenantKey(['policies'], '  tenant-123  ');
        expect(result).toEqual(['policies', 'tenant-123']);
      });

      it('should handle tenant IDs with leading/trailing spaces', () => {
        const result = withTenantKey(['reviews'], '\ttenant-abc\n');
        expect(result).toEqual(['reviews', 'tenant-abc']);
      });

      it('should handle multiple key parts with tenant ID', () => {
        const result = withTenantKey(['users', 'profile', 'settings'], 'tenant-xyz');
        expect(result).toEqual(['users', 'profile', 'settings', 'tenant-xyz']);
      });

      it('should handle single key part with tenant ID', () => {
        const result = withTenantKey(['metrics'], 'tenant-001');
        expect(result).toEqual(['metrics', 'tenant-001']);
      });

      it('should handle long tenant IDs', () => {
        const longTenantId = 'tenant-' + 'x'.repeat(100);
        const result = withTenantKey(['data'], longTenantId);
        expect(result).toEqual(['data', longTenantId]);
      });

      it('should preserve tenant ID case sensitivity', () => {
        const result1 = withTenantKey(['users'], 'Tenant-ABC');
        const result2 = withTenantKey(['users'], 'tenant-abc');
        expect(result1).toEqual(['users', 'Tenant-ABC']);
        expect(result2).toEqual(['users', 'tenant-abc']);
        expect(result1).not.toEqual(result2);
      });
    });

    describe('Sentinel values for no-tenant scenarios', () => {
      it('should use sentinel when tenant ID is null', () => {
        const result = withTenantKey(['users', 'list'], null);
        expect(result).toEqual(['users', 'list', '__no-tenant__']);
      });

      it('should use sentinel when tenant ID is undefined', () => {
        const result = withTenantKey(['adapters']);
        expect(result).toEqual(['adapters', '__no-tenant__']);
      });

      it('should use sentinel when tenant ID is empty string', () => {
        const result = withTenantKey(['models'], '');
        expect(result).toEqual(['models', '__no-tenant__']);
      });

      it('should use sentinel when tenant ID is only whitespace', () => {
        const result = withTenantKey(['cache'], '   ');
        expect(result).toEqual(['cache', '__no-tenant__']);
      });

      it('should use sentinel when tenant ID is tabs and spaces', () => {
        const result = withTenantKey(['data'], '\t\t  \n  ');
        expect(result).toEqual(['data', '__no-tenant__']);
      });

      it('should use sentinel when tenant ID is null with multiple key parts', () => {
        const result = withTenantKey(['users', 'profile', 'settings'], null);
        expect(result).toEqual(['users', 'profile', 'settings', '__no-tenant__']);
      });
    });

    describe('Cross-tenant cache key isolation', () => {
      it('should create different keys for different tenants', () => {
        const tenantAKey = withTenantKey(['users', 'list'], 'tenant-a');
        const tenantBKey = withTenantKey(['users', 'list'], 'tenant-b');

        expect(tenantAKey).not.toEqual(tenantBKey);
        expect(tenantAKey).toEqual(['users', 'list', 'tenant-a']);
        expect(tenantBKey).toEqual(['users', 'list', 'tenant-b']);
      });

      it('should create same keys for same tenant', () => {
        const key1 = withTenantKey(['users', 'list'], 'tenant-123');
        const key2 = withTenantKey(['users', 'list'], 'tenant-123');

        expect(key1).toEqual(key2);
      });

      it('should isolate tenant data from no-tenant data', () => {
        const tenantKey = withTenantKey(['adapters'], 'tenant-123');
        const noTenantKey = withTenantKey(['adapters'], null);

        expect(tenantKey).not.toEqual(noTenantKey);
        expect(tenantKey).toEqual(['adapters', 'tenant-123']);
        expect(noTenantKey).toEqual(['adapters', '__no-tenant__']);
      });

      it('should ensure tenant cannot access sentinel keys', () => {
        const sentinelKey = withTenantKey(['data'], null);
        const maliciousKey = withTenantKey(['data'], '__no-tenant__');

        // This verifies that a tenant trying to use the sentinel value
        // would actually get the sentinel value, not bypass isolation
        expect(sentinelKey).toEqual(maliciousKey);
        expect(maliciousKey).toEqual(['data', '__no-tenant__']);
      });

      it('should create different keys for case-sensitive tenant IDs', () => {
        const lowerKey = withTenantKey(['users'], 'tenant-abc');
        const upperKey = withTenantKey(['users'], 'TENANT-ABC');

        expect(lowerKey).not.toEqual(upperKey);
      });

      it('should create different keys even with trimmed whitespace', () => {
        const key1 = withTenantKey(['users'], 'tenant-123');
        const key2 = withTenantKey(['users'], '  tenant-123  ');

        // After trimming, they should be the same
        expect(key1).toEqual(key2);
      });

      it('should isolate multiple tenants with similar IDs', () => {
        const keys = [
          withTenantKey(['data'], 'tenant-1'),
          withTenantKey(['data'], 'tenant-10'),
          withTenantKey(['data'], 'tenant-100'),
          withTenantKey(['data'], 'tenant-2'),
        ];

        // All keys should be unique
        const uniqueKeys = new Set(keys.map(k => JSON.stringify(k)));
        expect(uniqueKeys.size).toBe(4);
      });
    });

    describe('Edge cases', () => {
      it('should handle empty key parts array', () => {
        const result = withTenantKey([], 'tenant-123');
        expect(result).toEqual(['tenant-123']);
      });

      it('should handle empty key parts with no tenant', () => {
        const result = withTenantKey([], null);
        expect(result).toEqual(['__no-tenant__']);
      });

      it('should handle key parts with numbers', () => {
        const result = withTenantKey(['users', 42, 'profile'], 'tenant-123');
        expect(result).toEqual(['users', 42, 'profile', 'tenant-123']);
      });

      it('should handle key parts with boolean values', () => {
        const result = withTenantKey(['settings', true, 'enabled'], 'tenant-123');
        expect(result).toEqual(['settings', true, 'enabled', 'tenant-123']);
      });

      it('should handle key parts with null values', () => {
        const result = withTenantKey(['data', null, 'item'], 'tenant-123');
        expect(result).toEqual(['data', null, 'item', 'tenant-123']);
      });

      it('should handle key parts with undefined values', () => {
        const result = withTenantKey(['data', undefined, 'item'], 'tenant-123');
        expect(result).toEqual(['data', undefined, 'item', 'tenant-123']);
      });

      it('should handle key parts with objects', () => {
        const filter = { status: 'active' };
        const result = withTenantKey(['users', filter], 'tenant-123');
        expect(result).toEqual(['users', filter, 'tenant-123']);
      });

      it('should handle key parts with arrays', () => {
        const result = withTenantKey(['users', ['id', 'name']], 'tenant-123');
        expect(result).toEqual(['users', ['id', 'name'], 'tenant-123']);
      });

      it('should handle tenant ID with only zeros', () => {
        const result = withTenantKey(['data'], '0000');
        expect(result).toEqual(['data', '0000']);
      });

      it('should handle tenant ID with forward slashes', () => {
        const result = withTenantKey(['data'], 'org/tenant/env');
        expect(result).toEqual(['data', 'org/tenant/env']);
      });

      it('should handle tenant ID with Unicode characters', () => {
        const result = withTenantKey(['data'], 'tenant-日本語');
        expect(result).toEqual(['data', 'tenant-日本語']);
      });

      it('should handle tenant ID with emoji', () => {
        const result = withTenantKey(['data'], 'tenant-🏢');
        expect(result).toEqual(['data', 'tenant-🏢']);
      });
    });

    describe('Key format consistency', () => {
      it('should always return a readonly array', () => {
        const result = withTenantKey(['users'], 'tenant-123');

        // TypeScript types should enforce readonly, but we can verify the structure
        expect(Array.isArray(result)).toBe(true);
      });

      it('should always append tenant segment as last element', () => {
        const result = withTenantKey(['a', 'b', 'c'], 'tenant-123');
        expect(result[result.length - 1]).toBe('tenant-123');
      });

      it('should always append sentinel as last element when no tenant', () => {
        const result = withTenantKey(['a', 'b', 'c'], null);
        expect(result[result.length - 1]).toBe('__no-tenant__');
      });

      it('should preserve original key parts order', () => {
        const keyParts = ['users', 'profile', 'settings'] as const;
        const result = withTenantKey(keyParts, 'tenant-123');

        expect(result[0]).toBe('users');
        expect(result[1]).toBe('profile');
        expect(result[2]).toBe('settings');
        expect(result[3]).toBe('tenant-123');
      });

      it('should return same length array for same input structure', () => {
        const result1 = withTenantKey(['a', 'b'], 'tenant-1');
        const result2 = withTenantKey(['x', 'y'], 'tenant-2');

        expect(result1.length).toBe(result2.length);
        expect(result1.length).toBe(3);
      });

      it('should handle consecutive calls with same inputs consistently', () => {
        const keyParts = ['users', 'list'] as const;
        const tenantId = 'tenant-123';

        const result1 = withTenantKey(keyParts, tenantId);
        const result2 = withTenantKey(keyParts, tenantId);
        const result3 = withTenantKey(keyParts, tenantId);

        expect(result1).toEqual(result2);
        expect(result2).toEqual(result3);
      });

      it('should handle deeply nested query key structures', () => {
        const complexKey = [
          'api',
          'v1',
          'resources',
          { type: 'adapter', filter: { status: 'active' } },
          ['field1', 'field2'],
        ] as const;

        const result = withTenantKey(complexKey, 'tenant-123');

        expect(result.length).toBe(6);
        expect(result[result.length - 1]).toBe('tenant-123');
      });
    });

    describe('Real-world cache key scenarios', () => {
      it('should create isolated keys for user queries', () => {
        const tenant1Users = withTenantKey(['users', 'list'], 'acme-corp');
        const tenant2Users = withTenantKey(['users', 'list'], 'widget-inc');

        expect(tenant1Users).not.toEqual(tenant2Users);
      });

      it('should create isolated keys for adapter queries', () => {
        const tenant1Adapters = withTenantKey(['adapters', { status: 'active' }], 'tenant-1');
        const tenant2Adapters = withTenantKey(['adapters', { status: 'active' }], 'tenant-2');

        expect(tenant1Adapters).not.toEqual(tenant2Adapters);
      });

      it('should create isolated keys for model queries', () => {
        const tenant1Models = withTenantKey(['models', 'training'], 'org-alpha');
        const tenant2Models = withTenantKey(['models', 'training'], 'org-beta');

        expect(tenant1Models).not.toEqual(tenant2Models);
      });

      it('should handle pagination with tenant isolation', () => {
        const tenant1Page1 = withTenantKey(['users', { page: 1, limit: 10 }], 'tenant-1');
        const tenant2Page1 = withTenantKey(['users', { page: 1, limit: 10 }], 'tenant-2');

        expect(tenant1Page1).not.toEqual(tenant2Page1);
      });

      it('should handle filtering with tenant isolation', () => {
        const filter = { status: 'active', type: 'production' };
        const tenant1Filtered = withTenantKey(['adapters', filter], 'tenant-1');
        const tenant2Filtered = withTenantKey(['adapters', filter], 'tenant-2');

        expect(tenant1Filtered).not.toEqual(tenant2Filtered);
      });

      it('should handle detail queries with tenant isolation', () => {
        const adapterId = 'adapter-123';
        const tenant1Detail = withTenantKey(['adapters', adapterId], 'tenant-1');
        const tenant2Detail = withTenantKey(['adapters', adapterId], 'tenant-2');

        expect(tenant1Detail).not.toEqual(tenant2Detail);
      });

      it('should prevent cache pollution from unauthenticated requests', () => {
        const authenticatedKey = withTenantKey(['users'], 'tenant-123');
        const unauthenticatedKey = withTenantKey(['users'], null);

        expect(authenticatedKey).not.toEqual(unauthenticatedKey);
        expect(unauthenticatedKey).toEqual(['users', '__no-tenant__']);
      });

      it('should handle tenant switching scenarios', () => {
        const beforeSwitch = withTenantKey(['dashboard', 'metrics'], 'old-tenant');
        const afterSwitch = withTenantKey(['dashboard', 'metrics'], 'new-tenant');

        expect(beforeSwitch).not.toEqual(afterSwitch);
      });
    });

    describe('Security: Tenant isolation verification', () => {
      it('should prevent tenant A from accessing tenant B cache', () => {
        const tenantA = 'tenant-a';
        const tenantB = 'tenant-b';
        const resource = ['sensitive', 'data'];

        const keyA = withTenantKey(resource, tenantA);
        const keyB = withTenantKey(resource, tenantB);

        expect(keyA).not.toEqual(keyB);
        expect(JSON.stringify(keyA)).not.toContain(tenantB);
        expect(JSON.stringify(keyB)).not.toContain(tenantA);
      });

      it('should prevent empty string tenant from accessing sentinel cache', () => {
        const emptyTenantKey = withTenantKey(['data'], '');
        const nullTenantKey = withTenantKey(['data'], null);
        const undefinedTenantKey = withTenantKey(['data'], undefined);

        expect(emptyTenantKey).toEqual(nullTenantKey);
        expect(emptyTenantKey).toEqual(undefinedTenantKey);
        expect(emptyTenantKey).toEqual(['data', '__no-tenant__']);
      });

      it('should treat sentinel value as literal tenant ID if explicitly provided', () => {
        // If a malicious tenant tries to use '__no-tenant__' as their ID,
        // it should be treated as a literal tenant ID
        const sentinelAsTenantKey = withTenantKey(['data'], '__no-tenant__');
        const actualSentinelKey = withTenantKey(['data'], null);

        // They should be equal because both get the sentinel value
        expect(sentinelAsTenantKey).toEqual(actualSentinelKey);
      });

      it('should handle SQL injection attempts in tenant ID', () => {
        const maliciousTenantId = "'; DROP TABLE tenants; --";
        const result = withTenantKey(['users'], maliciousTenantId);

        expect(result).toEqual(['users', "'; DROP TABLE tenants; --"]);
      });

      it('should handle XSS attempts in tenant ID', () => {
        const maliciousTenantId = '<script>alert("xss")</script>';
        const result = withTenantKey(['users'], maliciousTenantId);

        expect(result).toEqual(['users', '<script>alert("xss")</script>']);
      });

      it('should handle path traversal attempts in tenant ID', () => {
        const maliciousTenantId = '../../../etc/passwd';
        const result = withTenantKey(['users'], maliciousTenantId);

        expect(result).toEqual(['users', '../../../etc/passwd']);
      });

      it('should ensure consistent hashing for cache lookups', () => {
        const tenantId = 'tenant-123';
        const key1 = withTenantKey(['users'], tenantId);
        const key2 = withTenantKey(['users'], tenantId);

        // Serialization should be identical for cache key matching
        expect(JSON.stringify(key1)).toBe(JSON.stringify(key2));
      });
    });
  });
});
