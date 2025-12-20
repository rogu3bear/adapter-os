/**
 * Adapter Data Mock Factories
 *
 * Factory functions for creating mock Adapter and AdapterStack data.
 */

import type { AdapterStack } from '@/api/types';
import type { AdapterLoadingItem } from '@/hooks/model-loading/types';

/**
 * Create a mock AdapterLoadingItem
 *
 * @example
 * ```typescript
 * const adapter = createMockAdapterLoadingItem(); // Ready adapter
 * const loading = createMockAdapterLoadingItem({ isLoading: true, state: 'cold' });
 * const failed = createMockAdapterLoadingItem({ hasError: true, errorMessage: 'OOM' });
 * ```
 */
export function createMockAdapterLoadingItem(
  overrides: Partial<AdapterLoadingItem> = {}
): AdapterLoadingItem {
  const state = overrides.state ?? 'warm';
  const isReady = state === 'warm' || state === 'hot' || state === 'resident';

  return {
    adapterId: 'adapter-1',
    name: 'Test Adapter',
    state,
    isLoading: false,
    hasError: false,
    errorMessage: undefined,
    memoryMb: 512,
    lastUpdated: Date.now(),
    isReady,
    ...overrides,
  };
}

/**
 * Create a mock AdapterStack
 *
 * @example
 * ```typescript
 * const stack = createMockAdapterStack(); // Default stack with 2 adapters
 * const empty = createMockAdapterStack({ adapter_ids: [] });
 * ```
 */
export function createMockAdapterStack(
  overrides: Partial<AdapterStack> = {}
): AdapterStack {
  return {
    id: 'stack-1',
    name: 'Test Stack',
    adapter_ids: ['adapter-1', 'adapter-2'],
    description: 'Test stack description',
    lifecycle_state: 'active',
    tenant_id: 'default-tenant',
    version: 1,
    is_active: true,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    ...overrides,
  };
}

/**
 * Create multiple mock adapter loading items
 *
 * @example
 * ```typescript
 * const adapters = createMockAdapterLoadingItems(3); // 3 ready adapters
 * const mixed = createMockAdapterLoadingItems(3, { state: 'cold' }); // 3 cold adapters
 * ```
 */
export function createMockAdapterLoadingItems(
  count: number,
  overrides: Partial<AdapterLoadingItem> = {}
): AdapterLoadingItem[] {
  return Array.from({ length: count }, (_, i) =>
    createMockAdapterLoadingItem({
      adapterId: `adapter-${i + 1}`,
      name: `Adapter ${i + 1}`,
      ...overrides,
    })
  );
}
