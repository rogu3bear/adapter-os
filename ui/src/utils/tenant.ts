export const TENANT_SWITCH_EVENT = 'aos:tenant-switched';
export const TENANT_ACCESS_DENIED_EVENT = 'aos:tenant-access-denied';

/**
 * Append the tenant id to a query key to ensure cache isolation.
 * Falls back to a sentinel when tenant is not yet known to avoid cross-tenant reuse.
 */
export function withTenantKey<T extends readonly unknown[]>(
  keyParts: T,
  tenantId?: string | null,
): readonly unknown[] {
  const tenantSegment = tenantId && tenantId.trim() ? tenantId.trim() : '__no-tenant__';
  return [...keyParts, tenantSegment] as const;
}
