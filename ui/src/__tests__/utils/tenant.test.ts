import { describe, it, expect } from 'vitest';
import { withTenantKey, TENANT_SWITCH_EVENT, TENANT_ACCESS_DENIED_EVENT } from '@/utils/tenant';

describe('tenant utilities', () => {
  describe('constants', () => {
    it('exports TENANT_SWITCH_EVENT constant', () => {
      expect(TENANT_SWITCH_EVENT).toBe('aos:tenant-switched');
    });

    it('exports TENANT_ACCESS_DENIED_EVENT constant', () => {
      expect(TENANT_ACCESS_DENIED_EVENT).toBe('aos:tenant-access-denied');
    });
  });

  describe('withTenantKey', () => {
    it('appends tenant id to query key', () => {
      const result = withTenantKey(['adapters', 'list'], 'tenant-123');
      expect(result).toEqual(['adapters', 'list', 'tenant-123']);
    });

    it('appends tenant id to empty array', () => {
      const result = withTenantKey([], 'tenant-123');
      expect(result).toEqual(['tenant-123']);
    });

    it('handles single element key parts', () => {
      const result = withTenantKey(['datasets'], 'tenant-456');
      expect(result).toEqual(['datasets', 'tenant-456']);
    });

    it('handles multi-element key parts', () => {
      const result = withTenantKey(['adapters', 'detail', 'adapter-1'], 'tenant-789');
      expect(result).toEqual(['adapters', 'detail', 'adapter-1', 'tenant-789']);
    });

    it('uses sentinel for null tenant id', () => {
      const result = withTenantKey(['adapters', 'list'], null);
      expect(result).toEqual(['adapters', 'list', '__no-tenant__']);
    });

    it('uses sentinel for undefined tenant id', () => {
      const result = withTenantKey(['adapters', 'list'], undefined);
      expect(result).toEqual(['adapters', 'list', '__no-tenant__']);
    });

    it('uses sentinel for empty string tenant id', () => {
      const result = withTenantKey(['adapters', 'list'], '');
      expect(result).toEqual(['adapters', 'list', '__no-tenant__']);
    });

    it('uses sentinel for whitespace-only tenant id', () => {
      const result = withTenantKey(['adapters', 'list'], '   ');
      expect(result).toEqual(['adapters', 'list', '__no-tenant__']);
    });

    it('trims whitespace from tenant id', () => {
      const result = withTenantKey(['adapters', 'list'], '  tenant-123  ');
      expect(result).toEqual(['adapters', 'list', 'tenant-123']);
    });

    it('handles tenant id with leading whitespace', () => {
      const result = withTenantKey(['adapters', 'list'], '  tenant-123');
      expect(result).toEqual(['adapters', 'list', 'tenant-123']);
    });

    it('handles tenant id with trailing whitespace', () => {
      const result = withTenantKey(['adapters', 'list'], 'tenant-123  ');
      expect(result).toEqual(['adapters', 'list', 'tenant-123']);
    });

    it('handles key parts with various data types', () => {
      const result = withTenantKey(['adapters', 123, true, { id: 'test' }], 'tenant-123');
      expect(result).toEqual(['adapters', 123, true, { id: 'test' }, 'tenant-123']);
    });

    it('preserves readonly array type', () => {
      const keyParts = ['adapters', 'list'] as const;
      const result = withTenantKey(keyParts, 'tenant-123');
      // Type check - result should be readonly
      expect(result).toEqual(['adapters', 'list', 'tenant-123']);
    });

    it('handles special characters in tenant id', () => {
      const result = withTenantKey(['adapters'], 'tenant-123-456_abc');
      expect(result).toEqual(['adapters', 'tenant-123-456_abc']);
    });

    it('handles numeric-like tenant id string', () => {
      const result = withTenantKey(['adapters'], '12345');
      expect(result).toEqual(['adapters', '12345']);
    });

    it('ensures cache isolation between different tenants', () => {
      const key1 = withTenantKey(['adapters', 'list'], 'tenant-1');
      const key2 = withTenantKey(['adapters', 'list'], 'tenant-2');
      const key3 = withTenantKey(['adapters', 'list'], null);

      // All keys should be different
      expect(key1).not.toEqual(key2);
      expect(key1).not.toEqual(key3);
      expect(key2).not.toEqual(key3);
    });

    it('ensures same tenant produces same key', () => {
      const key1 = withTenantKey(['adapters', 'list'], 'tenant-123');
      const key2 = withTenantKey(['adapters', 'list'], 'tenant-123');
      expect(key1).toEqual(key2);
    });

    it('ensures undefined and null produce same sentinel key', () => {
      const key1 = withTenantKey(['adapters', 'list'], undefined);
      const key2 = withTenantKey(['adapters', 'list'], null);
      expect(key1).toEqual(key2);
    });
  });
});
