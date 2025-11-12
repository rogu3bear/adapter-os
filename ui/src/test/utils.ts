import { vi } from 'vitest';
import type { StatusOperationRecord, StatusV2, StatusTenantRecord } from '@/api/status';
import type apiClient from '@/api/client';

/**
 * Build a StatusV2 object for testing with sensible defaults.
 * 
 * @param overrides - Partial StatusV2 to override defaults
 * @returns A complete StatusV2 object
 */
export function buildStatus(overrides: Partial<StatusV2> = {}): StatusV2 {
  const tenants = (overrides.tenants ?? [
    {
      tenantId: 'tenant-1',
      displayName: 'Tenant One',
      isolationLevel: 'strict',
      permissions: ['role:viewer'],
    },
  ]).map(tenant => ({ ...tenant, permissions: [...tenant.permissions] }));

  const operations = (overrides.operations ?? []).map(op => ({ ...op }));

  return {
    schema: 'status.v2',
    version: 2,
    issuedAt: overrides.issuedAt ?? new Date().toISOString(),
    expiresAt: overrides.expiresAt,
    nonce: overrides.nonce ?? `nonce-${Math.random().toString(36).slice(2)}`,
    tenants,
    operations,
    metadata: overrides.metadata,
    signature: {
      algorithm: overrides.signature?.algorithm ?? 'digest-sha256',
      value: overrides.signature?.value ?? '',
      keyId: overrides.signature?.keyId ?? 'test-key',
      issuedAt: overrides.signature?.issuedAt ?? new Date().toISOString(),
    },
  } satisfies StatusV2;
}

/**
 * Build a StatusOperationRecord for testing.
 * 
 * @param opId - Operation ID
 * @param overrides - Partial StatusOperationRecord to override defaults
 * @returns A complete StatusOperationRecord
 */
export function buildOperation(opId: string, overrides: Partial<StatusOperationRecord> = {}): StatusOperationRecord {
  const now = new Date().toISOString();
  return {
    opId,
    tenantId: 'tenant-1',
    command: 'deploy',
    state: 'pending',
    retries: 0,
    lastUpdated: now,
    ...overrides,
  };
}

/**
 * Create a deferred promise that can be resolved externally.
 * Useful for controlling async test flows.
 * 
 * @returns Object with promise and resolve function
 */
export function createDeferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>(res => {
    resolve = res;
  });
  return { promise, resolve };
}

/**
 * Wait for a condition to become true, polling with Promise.resolve().
 * 
 * @param predicate - Function that returns true when condition is met
 * @param maxAttempts - Maximum number of attempts before throwing
 * @throws Error if condition is not met within maxAttempts
 */
export async function waitForCondition(predicate: () => boolean, maxAttempts = 25): Promise<void> {
  for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
    if (predicate()) {
      return;
    }
    await Promise.resolve();
  }
  throw new Error('Condition not satisfied within attempts');
}

/**
 * Create a mock API client with all methods stubbed.
 * Useful for component tests that need API mocking.
 * 
 * @returns Mock API client with vi.fn() stubs
 */
export function createMockApiClient(): typeof apiClient {
  return {
    getToken: vi.fn(() => null),
    setToken: vi.fn(),
    getCurrentUser: vi.fn().mockResolvedValue({
      user_id: 'u-test',
      email: 'test@example.com',
      role: 'viewer',
    }),
    login: vi.fn(),
    logout: vi.fn(),
    listTenants: vi.fn().mockResolvedValue([]),
    getSystemMetrics: vi.fn().mockResolvedValue(null),
    subscribeToMetrics: vi.fn(() => () => {}),
    getTelemetryEvents: vi.fn().mockResolvedValue([]),
    getRecentActivityEvents: vi.fn().mockResolvedValue([]),
    listActivityEvents: vi.fn().mockResolvedValue([]),
    subscribeToActivity: vi.fn(() => () => {}),
    listAlerts: vi.fn().mockResolvedValue([]),
    subscribeToAlerts: vi.fn(() => () => {}),
    listRepositories: vi.fn().mockResolvedValue([]),
    importModel: vi.fn(),
    buildUrl: vi.fn((path: string) => `/api${path}`),
    request: vi.fn(),
    getRequestLog: vi.fn().mockReturnValue([]),
  } as any;
}

/**
 * Create a mock StatusV2 object (alias for buildStatus for consistency).
 * 
 * @param overrides - Partial StatusV2 to override defaults
 * @returns A complete StatusV2 object
 */
export function createMockStatus(overrides: Partial<StatusV2> = {}): StatusV2 {
  return buildStatus(overrides);
}

/**
 * Create a mock tenant record for testing.
 * 
 * @param overrides - Partial StatusTenantRecord to override defaults
 * @returns A complete StatusTenantRecord
 */
export function createMockTenant(overrides: Partial<StatusTenantRecord> = {}): StatusTenantRecord {
  return {
    tenantId: overrides.tenantId ?? 'tenant-1',
    displayName: overrides.displayName ?? 'Test Tenant',
    isolationLevel: overrides.isolationLevel ?? 'strict',
    permissions: overrides.permissions ?? ['role:viewer'],
    labels: overrides.labels,
  };
}


