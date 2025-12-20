/**
 * Tests for tenant cache isolation in React Query hooks
 * Ensures query keys include tenant ID to prevent cross-tenant data leaks
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { withTenantKey } from '@/utils/tenant';

// Mock useTenant hook
const mockSelectedTenant = vi.fn();
vi.mock('@/providers/FeatureProviders', () => ({
  useTenant: () => ({ selectedTenant: mockSelectedTenant() }),
}));

describe('withTenantKey utility', () => {
  it('appends tenant ID to query key', () => {
    const result = withTenantKey(['chat', 'categories'], 'tenant-123');
    expect(result).toEqual(['chat', 'categories', 'tenant-123']);
  });

  it('uses sentinel for undefined tenant', () => {
    const result = withTenantKey(['chat', 'categories'], undefined);
    expect(result).toEqual(['chat', 'categories', '__no-tenant__']);
  });

  it('uses sentinel for null tenant', () => {
    const result = withTenantKey(['chat', 'categories'], null);
    expect(result).toEqual(['chat', 'categories', '__no-tenant__']);
  });

  it('uses sentinel for empty string tenant', () => {
    const result = withTenantKey(['chat', 'categories'], '');
    expect(result).toEqual(['chat', 'categories', '__no-tenant__']);
  });

  it('uses sentinel for whitespace-only tenant', () => {
    const result = withTenantKey(['chat', 'categories'], '   ');
    expect(result).toEqual(['chat', 'categories', '__no-tenant__']);
  });

  it('trims whitespace from tenant ID', () => {
    const result = withTenantKey(['chat', 'categories'], '  tenant-123  ');
    expect(result).toEqual(['chat', 'categories', 'tenant-123']);
  });
});

describe('Tenant cache isolation', () => {
  beforeEach(() => {
    mockSelectedTenant.mockReset();
  });

  describe('Query key uniqueness', () => {
    it('generates different keys for different tenants', () => {
      const keyA = withTenantKey(['adapters'], 'tenant-A');
      const keyB = withTenantKey(['adapters'], 'tenant-B');

      expect(keyA).not.toEqual(keyB);
      expect(keyA[keyA.length - 1]).toBe('tenant-A');
      expect(keyB[keyB.length - 1]).toBe('tenant-B');
    });

    it('generates consistent keys for same tenant', () => {
      const key1 = withTenantKey(['adapters'], 'tenant-A');
      const key2 = withTenantKey(['adapters'], 'tenant-A');

      expect(key1).toEqual(key2);
    });

    it('prevents cache hits across tenants', () => {
      // Simulate tenant A's query key
      const tenantAKey = withTenantKey(['training', 'jobs'], 'tenant-A');

      // Simulate tenant B's query key
      const tenantBKey = withTenantKey(['training', 'jobs'], 'tenant-B');

      // These should never match, preventing cross-tenant cache hits
      const keyMatch = JSON.stringify(tenantAKey) === JSON.stringify(tenantBKey);
      expect(keyMatch).toBe(false);
    });
  });

  describe('Chat categories query keys', () => {
    it('includes tenant in categories key', () => {
      const key = withTenantKey(['chat', 'categories'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });

    it('includes tenant in category detail key', () => {
      const key = withTenantKey(['chat', 'categories', 'cat-456'], 'tenant-123');
      expect(key).toEqual(['chat', 'categories', 'cat-456', 'tenant-123']);
    });
  });

  describe('Training query keys', () => {
    it('includes tenant in training jobs key', () => {
      const key = withTenantKey(['training', 'jobs'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });

    it('includes tenant in datasets key', () => {
      const key = withTenantKey(['training', 'datasets'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });

    it('includes tenant in templates key', () => {
      const key = withTenantKey(['training', 'templates'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });
  });

  describe('Adapter query keys', () => {
    it('includes tenant in adapters key', () => {
      const key = withTenantKey(['adapters'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });

    it('includes tenant in adapter-versions key', () => {
      const key = withTenantKey(['adapter-versions'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });

    it('includes tenant in adapter-stacks key', () => {
      const key = withTenantKey(['adapter-stacks'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });
  });

  describe('Chat archive query keys', () => {
    it('includes tenant in archived sessions key', () => {
      const key = withTenantKey(['chat', 'sessions', 'archived'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });

    it('includes tenant in deleted sessions key', () => {
      const key = withTenantKey(['chat', 'sessions', 'trash'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });

    it('includes tenant in chat-sessions invalidation key', () => {
      const key = withTenantKey(['chat-sessions'], 'tenant-123');
      expect(key).toContain('tenant-123');
    });
  });
});

describe('Cache invalidation isolation', () => {
  it('tenant-scoped invalidation does not match other tenants', () => {
    const tenantAInvalidationKey = withTenantKey(['adapters'], 'tenant-A');
    const tenantBCacheKey = withTenantKey(['adapters'], 'tenant-B');

    // Simulating React Query's key matching - exact match required
    const wouldInvalidate = JSON.stringify(tenantAInvalidationKey) === JSON.stringify(tenantBCacheKey);
    expect(wouldInvalidate).toBe(false);
  });

  it('tenant-scoped invalidation matches same tenant', () => {
    const invalidationKey = withTenantKey(['adapters'], 'tenant-A');
    const cacheKey = withTenantKey(['adapters'], 'tenant-A');

    const wouldInvalidate = JSON.stringify(invalidationKey) === JSON.stringify(cacheKey);
    expect(wouldInvalidate).toBe(true);
  });
});
